//! Replay: XP_STATE_REPLAY=artifacts/state-machine/seed-N.min.txt cargo test -p
//! xp-store --test state_machine seeded_histories -- --nocapture
//! All expectations live in support/model.rs; only this driver calls Store transitions.
#[path = "support/encoding.rs"]
mod encoding;
#[path = "support/logical.rs"]
mod logical;
#[path = "support/model.rs"]
mod model;
use logical::{difference, snapshot};
use model::{id, output, Model};
use redb::ReadableTable;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Instant,
};
use xp_store::{tables::UNDO, Store, StoreError, ROLLBACK_WINDOW};
use xp_types::{BoxId, HeaderId, TxId};
use xp_wire::{DecodedBlock, DecodedHeader, DecodedTx};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}
#[derive(Clone, Debug)]
enum Op {
    Apply(u64, u64),
    Rollback(u64),
    Reopen,
}
fn history(seed: u64) -> Vec<Op> {
    let mut r = Rng(seed + 0x12345678);
    (0..32)
        .map(|i| match i % 8 {
            2 | 6 => Op::Rollback(1 + r.next() % 3),
            4 if i == 4 => Op::Reopen,
            4 => Op::Rollback(1),
            1 | 5 => Op::Apply(2, r.next()),
            _ => Op::Apply(1, r.next()),
        })
        .collect()
}
fn block(m: &Model, salt: u64, empty: bool) -> DecodedBlock {
    let h = m.height + 1;
    let mut r = Rng(if salt == 0 { 1 } else { salt });
    let mut b = DecodedBlock {
        header: DecodedHeader {
            id: HeaderId(id(salt)),
            parent_id: HeaderId(m.tip),
            height: h,
            timestamp: h as u64 * 120000,
            difficulty: 17,
            miner_pk: [0; 33],
            votes: [0; 3],
            version: 3,
            raw_json: "{}".into(),
        },
        txs: vec![],
        size: 300,
    };
    if empty {
        return b;
    }
    let live = m.boxes.iter().find(|(_, b)| b.spent.is_none());
    let mut input = live.map_or(id(555), |(id, _)| *id);
    let mut tokens = live.map_or(vec![], |(_, b)| b.tokens.clone());
    // Every block has a same-block spend. Keep only one live box, bounding the
    // model's work while varying addresses, amounts, ids, mint/burn and tx count.
    for i in 0..2u64 {
        let n = r.next() & !3;
        let mut o = output(n, 1 + r.next() % 3, h, 1000 + r.next() % 100_000);
        if tokens.is_empty() {
            tokens = vec![(input, 10 + r.next() % 100)];
        } else if r.next().is_multiple_of(3) {
            tokens.clear();
        } else {
            tokens[0].1 = tokens[0].1.saturating_sub(1).max(1);
        }
        o.tokens = tokens.clone();
        // IDs are namespaced by command/block/tx, independent of overlapping
        // PRNG subsequences used to choose amounts and addresses.
        let mut unique = id(salt);
        unique[8..12].copy_from_slice(&h.to_be_bytes());
        unique[12..16].copy_from_slice(&(i as u32).to_be_bytes());
        unique[16] = 1;
        o.id = BoxId(unique);
        unique[16] = 2;
        o.tx_id = TxId(unique);
        let tx = DecodedTx {
            id: o.tx_id,
            inputs: vec![BoxId(input)],
            data_inputs: if i == 0 { vec![BoxId(id(777))] } else { vec![] },
            outputs: vec![o.clone()],
            size: 100 + i as u32,
        };
        input = o.id.0;
        b.txs.push(tx);
    }
    b
}
fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn temp() -> tempfile::TempDir {
    let dir = workspace().join("artifacts/state-machine/tmp");
    std::fs::create_dir_all(&dir).unwrap();
    tempfile::tempdir_in(dir).unwrap()
}
fn start(path: &Path, partial: bool, base: u32) -> (Store, Model) {
    let s = Store::open(path).unwrap();
    if partial {
        s.seed_for_tests(base, id(9)).unwrap();
    } else {
        s.seed_genesis(&[output(100, 1, 0, 1_000_000)]).unwrap();
    }
    (s, Model::new(partial, base))
}
fn run(seed: u64, ops: &[Op], inject: bool) -> Result<(), String> {
    // Assertions and unexpected store/read errors also produce replay artifacts.
    std::panic::catch_unwind(|| run_inner(seed, ops, inject)).unwrap_or_else(|panic| {
        let message = panic
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| panic.downcast_ref::<&str>().map(|s| (*s).to_owned()))
            .unwrap_or_else(|| "non-string panic".into());
        Err(format!("panic: {message}"))
    })
}
fn run_inner(seed: u64, ops: &[Op], inject: bool) -> Result<(), String> {
    let dir = temp();
    let path = dir.path().join("state.redb");
    let partial = seed.is_multiple_of(2);
    let (mut s, mut m) = start(&path, partial, if partial { 2000 } else { 0 });
    difference(&snapshot(&s), &m.expected())?;
    let mut saw_mint = false;
    let mut saw_burn = false;
    let mut saw_same_block_spend = false;
    let mut reused_gidx = false;
    let mut rolled_back = false;
    let mut fingerprints = BTreeMap::new();
    fingerprints.insert(m.height, s.fingerprint().unwrap());
    for (step, op) in ops.iter().enumerate() {
        match op {
            Op::Apply(count, salt) => {
                let mut blocks = vec![];
                for j in 0..*count {
                    let b = block(&m, salt.wrapping_add(j), false);
                    for tx in &b.txs {
                        saw_mint |= tx.outputs.iter().any(|o| {
                            o.tokens
                                .iter()
                                .any(|(token, _)| Some(*token) == tx.inputs.first().map(|i| i.0))
                        });
                        // Every generated input carries tokens unless this tx mints.
                        saw_burn |= tx.outputs.iter().any(|o| o.tokens.is_empty());
                    }
                    saw_same_block_spend |= b
                        .txs
                        .windows(2)
                        .any(|pair| pair[1].inputs.contains(&pair[0].outputs[0].id));
                    reused_gidx |= rolled_back;
                    rolled_back = false;
                    m.advance(&b);
                    blocks.push(b);
                }
                s.apply_batch(&blocks, false)
                    .map_err(|e| format!("apply: {e}"))?;
                if inject {
                    let indexed_before = s.fingerprint().unwrap();
                    drop(s);
                    damage(&path);
                    s = Store::open(&path).unwrap();
                    assert_eq!(
                        s.fingerprint().unwrap(),
                        indexed_before,
                        "UNDO-only mutation must be invisible to the existing fingerprint"
                    );
                }
            }
            Op::Rollback(depth) => {
                let target = m.height.saturating_sub(*depth as u32).max(m.base);
                s.rollback_to(target)
                    .map_err(|e| format!("rollback: {e}"))?;
                if target < m.height {
                    let previous_next = m.next_box;
                    m.rewind(target);
                    assert!(m.next_box < previous_next);
                    rolled_back = true;
                }
                if let Some(fp) = fingerprints.get(&target) {
                    // A pristine genesis store has no tx counter; rollback writes zero.
                    if target != 0 && s.fingerprint().unwrap() != *fp {
                        return Err("fingerprint identity".into());
                    }
                }
                fingerprints.retain(|h, _| *h <= target);
            }
            Op::Reopen => {
                let before = s.fingerprint().unwrap();
                drop(s);
                s = Store::open(&path).map_err(|e| format!("reopen: {e}"))?;
                if before != s.fingerprint().unwrap() {
                    return Err("reopen fingerprint".into());
                }
            }
        }
        difference(&snapshot(&s), &m.expected())
            .map_err(|e| format!("{e} step={step} op={op:?}"))?;
        fingerprints.insert(m.height, s.fingerprint().unwrap());
    }
    if ops.len() == 32 {
        assert!(
            saw_mint && saw_burn && saw_same_block_spend && reused_gidx,
            "generated history lost required transition coverage"
        );
    }
    Ok(())
}
fn damage(path: &Path) {
    let db = redb::Database::create(path).unwrap();
    let tx = db.begin_write().unwrap();
    {
        let mut t = tx.open_table(UNDO).unwrap();
        let (k, mut v) = {
            let (k, v) = t.iter().unwrap().next_back().unwrap().unwrap();
            (k.value().to_vec(), v.value().to_vec())
        };
        // Change one byte of the last register gidx (or empty row's last count).
        // This leaves normal generated undo structurally decodable, but wrong.
        *v.last_mut().unwrap() ^= 1;
        t.insert(k.as_slice(), v.as_slice()).unwrap();
    }
    tx.commit().unwrap();
}
fn signature(error: &str) -> &str {
    error
        .split(" key=")
        .next()
        .unwrap()
        .split(" step=")
        .next()
        .unwrap()
}
fn minimize(mut ops: Vec<Op>, mut fails: impl FnMut(&[Op]) -> bool) -> Vec<Op> {
    // Delta debugging followed by single deletions: result is 1-minimal for
    // the same failure signature. Commands regenerate valid inputs from state,
    // so deletion cannot leave dangling tx ids or invalid branch parents.
    let mut chunk = ops.len().div_ceil(2);
    while chunk > 0 {
        let mut i = 0;
        while i < ops.len() {
            let mut candidate = ops.clone();
            candidate.drain(i..(i + chunk).min(candidate.len()));
            if fails(&candidate) {
                ops = candidate;
                i = 0;
            } else {
                i += chunk;
            }
        }
        chunk /= 2;
    }
    ops
}
fn save(seed: u64, ops: &[Op], error: &str, suffix: &str) -> PathBuf {
    let path = workspace().join(format!("artifacts/state-machine/seed-{seed}.{suffix}.txt"));
    let mut text = format!("seed {seed}\n# {error}\n");
    for op in ops {
        text += &match op {
            Op::Apply(n, s) => format!("apply {n} {s}\n"),
            Op::Rollback(n) => format!("rollback {n}\n"),
            Op::Reopen => "reopen\n".into(),
        };
    }
    std::fs::write(&path, text).unwrap();
    path
}
fn read(path: &Path) -> (u64, Vec<Op>) {
    let text = std::fs::read_to_string(path).unwrap();
    let mut seed = 0;
    let mut ops = vec![];
    for l in text.lines().filter(|l| !l.starts_with('#')) {
        let p: Vec<_> = l.split_whitespace().collect();
        match p.as_slice() {
            ["seed", s] => seed = s.parse().unwrap(),
            ["apply", n, s] => ops.push(Op::Apply(n.parse().unwrap(), s.parse().unwrap())),
            ["rollback", n] => ops.push(Op::Rollback(n.parse().unwrap())),
            ["reopen"] => ops.push(Op::Reopen),
            _ => panic!("bad replay line {l}"),
        }
    }
    (seed, ops)
}
#[test]
fn seeded_histories() {
    let started = Instant::now();
    let histories = if let Some(p) = std::env::var_os("XP_STATE_REPLAY") {
        vec![read(Path::new(&p))]
    } else {
        (0..256).map(|s| (s, history(s))).collect()
    };
    std::thread::scope(|scope| {
        let mut workers = vec![];
        for chunk in histories.chunks(histories.len().div_ceil(4)) {
            workers.push(scope.spawn(move || {
                for (seed, ops) in chunk {
                    if let Err(error) = run(*seed, ops, false) {
                        // Save full replay FIRST, before any shrinking work.
                        save(*seed, ops, &error, "full");
                        let small = minimize(ops.clone(), |candidate| {
                            run(*seed, candidate, false)
                                .is_err_and(|e| signature(&e) == signature(&error))
                        });
                        let path = save(*seed, &small, &error, "min");
                        panic!("seed={seed} {error}; minimized replay: {}", path.display());
                    }
                }
            }));
        }
        for worker in workers {
            worker.join().unwrap();
        }
    });
    eprintln!(
        "state model: {} seeds x 32 transitions; wall={:.3}s",
        histories.len(),
        started.elapsed().as_secs_f64()
    );
}
#[test]
fn undo_mutation_and_minimized_replay() {
    let ops = history(7);
    let error = run(7, &ops, true).unwrap_err();
    assert!(error.starts_with("table=undo"), "{error}");
    let small = minimize(ops, |c| {
        run(7, c, true).is_err_and(|e| signature(&e) == signature(&error))
    });
    assert_eq!(small.len(), 1);
    assert!(matches!(small[0], Op::Apply(..)));
    let p = save(7, &small, &error, "mutation-proof");
    let (seed, replayed) = read(&p);
    assert!(run(seed, &replayed, false).is_ok());
    assert!(run(seed, &replayed, true)
        .unwrap_err()
        .starts_with("table=undo"));
    eprintln!(
        "UNDO mutation detected; 32 commands minimized to 1, replay confirmed: {}",
        p.display()
    );
}
#[test]
fn undo_retention_boundaries() {
    let started = Instant::now();
    for count in [ROLLBACK_WINDOW - 1, ROLLBACK_WINDOW, ROLLBACK_WINDOW + 1] {
        let dir = temp();
        let path = dir.path().join("retention.redb");
        let (s, mut m) = start(&path, true, 2000);
        // Include nonempty independent undo payloads at both ends, cheap empty
        // blocks in between: test actual pruning with no lowered production window.
        let mut blocks = vec![];
        for j in 0..count {
            let b = block(&m, 9000 + j as u64, j != 0 && j != count - 1);
            m.advance(&b);
            blocks.push(b);
        }
        s.apply_batch(&blocks, false).unwrap();
        difference(&snapshot(&s), &m.expected()).unwrap();
        let expected_low = (m.base + 1).max(m.height - ROLLBACK_WINDOW);
        assert_eq!(
            m.undo.keys().copied().collect::<Vec<_>>(),
            (expected_low..=m.height).collect::<Vec<_>>()
        );
        let before = snapshot(&s);
        let fp = s.fingerprint().unwrap();
        // Just beyond the public depth cap (may still have an extra retained row).
        let outside = m.height - ROLLBACK_WINDOW - 1;
        assert!(matches!(
            s.rollback_to(outside),
            Err(StoreError::ReindexRequired(_))
        ));
        assert_eq!(snapshot(&s), before);
        assert_eq!(s.fingerprint().unwrap(), fp);
        let target = m.height.saturating_sub(ROLLBACK_WINDOW).max(m.base);
        s.rollback_to(target).unwrap();
        m.rewind(target);
        difference(&snapshot(&s), &m.expected()).unwrap();
        if count == ROLLBACK_WINDOW + 1 {
            // Consume the extra retained row with a second call, then step outside
            // available history. This exercises missing UNDO, not only depth guard.
            s.rollback_to(m.base).unwrap();
            m.rewind(m.base);
            difference(&snapshot(&s), &m.expected()).unwrap();
            let fp = s.fingerprint().unwrap();
            let before = snapshot(&s);
            assert!(matches!(
                s.rollback_to(m.base - 1),
                Err(StoreError::ReindexRequired(_))
            ));
            assert_eq!(snapshot(&s), before);
            assert_eq!(s.fingerprint().unwrap(), fp);
        }
    }
    // A fourth height is necessary to actually evict the first row when starting
    // at a seed: W+1 rows fit, W+2 prune. The model must not resurrect that row.
    let dir = temp();
    let (s, mut m) = start(&dir.path().join("pruned.redb"), true, 2000);
    let mut blocks = vec![];
    for j in 0..ROLLBACK_WINDOW + 2 {
        let b = block(&m, 19000 + j as u64, j != 0);
        m.advance(&b);
        blocks.push(b);
    }
    s.apply_batch(&blocks, false).unwrap();
    difference(&snapshot(&s), &m.expected()).unwrap();
    let indexed_full = s.fingerprint().unwrap();
    assert!(!m.undo.contains_key(&(m.base + 1)));
    let target = m.height - ROLLBACK_WINDOW;
    s.rollback_to(target).unwrap();
    m.rewind(target);
    difference(&snapshot(&s), &m.expected()).unwrap();
    s.rollback_to(m.base + 1).unwrap();
    m.rewind(m.base + 1);
    difference(&snapshot(&s), &m.expected()).unwrap();
    let before = snapshot(&s);
    let fp = s.fingerprint().unwrap();
    assert!(matches!(
        s.rollback_to(m.base),
        Err(StoreError::ReindexRequired(_))
    ));
    assert_eq!(snapshot(&s), before);
    assert_eq!(s.fingerprint().unwrap(), fp);
    // Reapply the original chain after pruning. Indexed bytes return exactly;
    // UNDO expectations are rebuilt independently, not copied from the store.
    s.apply_batch(&blocks[1..], false).unwrap();
    for b in &blocks[1..] {
        m.advance(b);
    }
    assert_eq!(s.fingerprint().unwrap(), indexed_full);
    difference(&snapshot(&s), &m.expected()).unwrap();
    eprintln!(
        "retention boundaries wall={:.3}s",
        started.elapsed().as_secs_f64()
    );
}
