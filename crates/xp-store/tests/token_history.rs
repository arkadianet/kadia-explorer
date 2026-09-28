use redb::ReadableTable;
use xp_store::read::Dir;
use xp_store::tables::*;
use xp_store::token_history::*;
use xp_store::{Reader, Store, StoreError};
use xp_types::{BoxId, HeaderId, TxId};
use xp_wire::{DecodedBlock, DecodedBox, DecodedTx};

fn output(n: u8, value: u64, tokens: Vec<([u8; 32], u64)>) -> DecodedBox {
    let tree = vec![0, 8, 0xd3];
    DecodedBox {
        id: BoxId([n; 32]),
        value,
        tree_hash: xp_wire::tree_hash(&tree),
        tree_bytes: tree,
        creation_height: 0,
        tx_id: TxId([0; 32]),
        index: 0,
        tokens,
        registers_json: "{}".into(),
        id_verified: false,
        size: 100,
    }
}
fn block(
    height: u32,
    n: u8,
    parent: [u8; 32],
    tid: u8,
    input: u8,
    mut outputs: Vec<DecodedBox>,
) -> DecodedBlock {
    let mut b =
        xp_wire::decode_block(include_str!("../../../tests/fixtures/blocks/1866000.json")).unwrap();
    for (index, out) in outputs.iter_mut().enumerate() {
        out.tx_id = TxId([tid; 32]);
        out.index = index as u16;
    }
    b.header.height = height;
    b.header.id = HeaderId([n; 32]);
    b.header.parent_id = HeaderId(parent);
    b.txs = vec![DecodedTx {
        id: TxId([tid; 32]),
        inputs: vec![BoxId([input; 32])],
        data_inputs: vec![],
        outputs,
        size: 100,
    }];
    b
}
fn blocks() -> Vec<DecodedBlock> {
    vec![
        block(
            1,
            100,
            [0; 32],
            10,
            1,
            vec![
                output(2, 6000, vec![([1; 32], 70)]),
                output(3, 4000, vec![([1; 32], 30)]),
            ],
        ),
        block(2, 101, [100; 32], 11, 2, vec![output(4, 6000, vec![])]), // complete token burn
        block(
            3,
            102,
            [101; 32],
            12,
            3,
            vec![output(0, 4000, vec![([1; 32], 30)])],
        ), // id behind backfill cursor
    ]
}
fn fixture() -> (tempfile::TempDir, Store) {
    let d = tempfile::tempdir().unwrap();
    let s = Store::open(&d.path().join("history.redb")).unwrap();
    s.seed_genesis(&[output(1, 10000, vec![])]).unwrap();
    s.apply_batch(&blocks()[..2], true).unwrap();
    (d, s)
}
fn edit(path: &std::path::Path, f: impl FnOnce(&redb::WriteTransaction)) {
    let db = redb::Database::open(path).unwrap();
    let t = db.begin_write().unwrap();
    f(&t);
    t.commit().unwrap();
}
fn erase_aux(path: &std::path::Path) {
    edit(path, |t| {
        for table in [TOKEN_TXS, TX_TOKENS, TOKEN_HISTORY_META] {
            t.delete_table(table).unwrap();
        }
    });
}
fn items(s: &Store) -> Vec<[u8; 32]> {
    Reader::new(s)
        .unwrap()
        .token_txs_bounded_admitted(&[1; 32], None, 100, Dir::Asc, None, |_| Ok(()))
        .unwrap()
        .items
        .into_iter()
        .map(|(id, _)| id)
        .collect()
}
fn finish(s: &Store) -> TokenHistoryStatus {
    for _ in 0..100 {
        let status = s.backfill_token_history_batch(1).unwrap();
        if status.ready {
            return status;
        }
    }
    panic!("backfill did not terminate")
}

#[test]
fn fresh_live_index_includes_mint_and_full_burn_once_and_undo_restores_it() {
    let (_d, s) = fixture();
    assert_eq!(items(&s), vec![[10; 32], [11; 32]]);
    let before = s.fingerprint().unwrap();
    s.apply_batch(&blocks()[2..], true).unwrap();
    assert_eq!(items(&s), vec![[10; 32], [11; 32], [12; 32]]);
    s.rollback_to(2).unwrap();
    assert_eq!(items(&s), vec![[10; 32], [11; 32]]);
    assert_eq!(s.fingerprint().unwrap(), before);
    s.rollback_to(0).unwrap();
    assert!(items(&s).is_empty());
}

#[test]
fn populated_backfill_restarts_and_tracks_spends_and_forks_behind_cursor() {
    let (d, s) = fixture();
    let original = s.fingerprint().unwrap();
    drop(s);
    let path = d.path().join("history.redb");
    erase_aux(&path);
    let s = Store::open(&path).unwrap();
    assert!(
        !Reader::new(&s)
            .unwrap()
            .token_history_status()
            .unwrap()
            .ready
    );
    assert!(matches!(
        Reader::new(&s).unwrap().token_txs_bounded_admitted(
            &[1; 32],
            None,
            1,
            Dir::Asc,
            None,
            |_| Ok(())
        ),
        Err(StoreError::TokenHistoryNotReady)
    ));
    assert_eq!(s.backfill_token_history_batch(1).unwrap().scanned_boxes, 1);
    drop(s);
    let s = Store::open(&path).unwrap();
    assert_eq!(
        Reader::new(&s)
            .unwrap()
            .token_history_status()
            .unwrap()
            .scanned_boxes,
        1
    );
    s.apply_batch(&blocks()[2..], true).unwrap();
    s.backfill_token_history_batch(1).unwrap();
    s.rollback_to(2).unwrap();
    assert_eq!(s.fingerprint().unwrap(), original);
    let alternative = block(
        3,
        103,
        [101; 32],
        13,
        3,
        vec![output(0, 4000, vec![([1; 32], 30)])],
    );
    s.apply_batch(&[alternative], true).unwrap();
    let before = s.fingerprint().unwrap();
    finish(&s);
    assert_eq!(items(&s), vec![[10; 32], [11; 32], [13; 32]]);
    assert_eq!(s.fingerprint().unwrap(), before);
}

#[test]
fn version_clear_interleaves_apply_rollback_without_recertifying_gaps() {
    let (d, s) = fixture();
    drop(s);
    let path = d.path().join("history.redb");
    edit(&path, |t| {
        let mut meta = t.open_table(TOKEN_HISTORY_META).unwrap();
        let mut state = meta
            .get(b"state".as_slice())
            .unwrap()
            .unwrap()
            .value()
            .to_vec();
        state[3] = 99;
        meta.insert(b"state".as_slice(), state.as_slice()).unwrap();
    });
    let s = Store::open(&path).unwrap();
    assert_eq!(s.backfill_token_history_batch(1).unwrap().phase, "clearing");
    s.apply_batch(&blocks()[2..], true).unwrap();
    s.rollback_to(2).unwrap();
    assert!(
        !Reader::new(&s)
            .unwrap()
            .token_history_status()
            .unwrap()
            .ready
    );
    let before = s.fingerprint().unwrap();
    finish(&s);
    assert_eq!(items(&s), vec![[10; 32], [11; 32]]);
    assert_eq!(s.fingerprint().unwrap(), before);
}

#[test]
fn live_full_burn_of_already_scanned_box_is_indexed_while_building() {
    let (d, s) = fixture();
    drop(s);
    let path = d.path().join("history.redb");
    erase_aux(&path);
    let s = Store::open(&path).unwrap();
    for _ in 0..3 {
        assert!(!s.backfill_token_history_batch(1).unwrap().ready);
    }
    assert_eq!(
        Reader::new(&s)
            .unwrap()
            .token_history_status()
            .unwrap()
            .scanned_boxes,
        3
    );
    // Box3 was already scanned as unspent. The token-free output is behind the cursor,
    // so only the live resolved-input hook can add this complete burn's membership.
    let burn = block(3, 102, [101; 32], 12, 3, vec![output(0, 4000, vec![])]);
    s.apply_batch(&[burn], true).unwrap();
    finish(&s);
    assert_eq!(items(&s), vec![[10; 32], [11; 32], [12; 32]]);
}

#[test]
fn stale_prewrite_anchor_and_same_height_replacement_force_rebuild() {
    let (d, s) = fixture();
    let path = d.path().join("history.redb");
    drop(s);
    let old = {
        let db = redb::Database::open(&path).unwrap();
        let read = db.begin_read().unwrap();
        let table = read.open_table(TOKEN_HISTORY_META).unwrap();
        let bytes = table
            .get(b"state".as_slice())
            .unwrap()
            .unwrap()
            .value()
            .to_vec();
        bytes
    };
    let s = Store::open(&path).unwrap();
    s.apply_batch(&blocks()[2..], true).unwrap();
    drop(s);
    edit(&path, |t| {
        t.open_table(TOKEN_HISTORY_META)
            .unwrap()
            .insert(b"state".as_slice(), old.as_slice())
            .unwrap();
        t.open_table(TOKEN_TXS)
            .unwrap()
            .remove(xp_store::keys::k_hash_gidx(&[1; 32], 2).as_slice())
            .unwrap();
        let mut reverse = [0; 40];
        reverse[..8].copy_from_slice(&2u64.to_be_bytes());
        reverse[8..].copy_from_slice(&[1; 32]);
        t.open_table(TX_TOKENS)
            .unwrap()
            .remove(reverse.as_slice())
            .unwrap();
    });
    let s = Store::open(&path).unwrap();
    assert!(
        !Reader::new(&s)
            .unwrap()
            .token_history_status()
            .unwrap()
            .ready
    );
    s.rollback_to(2).unwrap();
    s.apply_batch(
        &[block(
            3,
            103,
            [101; 32],
            13,
            3,
            vec![output(0, 4000, vec![([1; 32], 30)])],
        )],
        true,
    )
    .unwrap();
    assert!(
        !Reader::new(&s)
            .unwrap()
            .token_history_status()
            .unwrap()
            .ready
    );
    finish(&s);
    assert_eq!(items(&s), vec![[10; 32], [11; 32], [13; 32]]);
}

#[test]
fn row_byte_and_token_work_bounds_fail_atomically_and_remain_unready() {
    for oversize_bytes in [true, false] {
        let (d, s) = fixture();
        drop(s);
        let path = d.path().join("history.redb");
        erase_aux(&path);
        edit(&path, |t| {
            let mut boxes = t.open_table(BOXES).unwrap();
            let mut row = xp_store::rows::BoxRow::decode(
                boxes.get([1; 32].as_slice()).unwrap().unwrap().value(),
            )
            .unwrap();
            if oversize_bytes {
                row.registers_json = "x".repeat(MAX_HISTORY_ROW_BYTES);
            } else {
                row.tokens = vec![([1; 32], 1); MAX_HISTORY_BATCH_WORK];
            }
            boxes
                .insert([1; 32].as_slice(), row.encode().as_slice())
                .unwrap();
        });
        let s = Store::open(&path).unwrap();
        let before = s.fingerprint().unwrap();
        assert!(matches!(
            s.backfill_token_history_batch(500),
            Err(StoreError::ReadLimit(_))
        ));
        let status = Reader::new(&s).unwrap().token_history_status().unwrap();
        assert!(!status.ready);
        assert_eq!(status.scanned_boxes, 0);
        assert_eq!(s.fingerprint().unwrap(), before);
    }
}

#[test]
fn genesis_token_only_adds_spend_and_partial_unknown_inputs_do_not_invent_membership() {
    let d = tempfile::tempdir().unwrap();
    let s = Store::open(&d.path().join("genesis.redb")).unwrap();
    s.seed_genesis(&[output(1, 10000, vec![([1; 32], 10)])])
        .unwrap();
    assert!(items(&s).is_empty());
    s.apply_batch(
        &[block(
            1,
            100,
            [0; 32],
            10,
            1,
            vec![output(2, 10000, vec![])],
        )],
        true,
    )
    .unwrap();
    assert_eq!(items(&s), vec![[10; 32]]);
    drop(s);
    let path = d.path().join("genesis.redb");
    erase_aux(&path);
    let s = Store::open(&path).unwrap();
    finish(&s);
    assert_eq!(items(&s), vec![[10; 32]]);
    let partial = Store::open(&d.path().join("partial.redb")).unwrap();
    partial.seed_for_tests(0, [0; 32]).unwrap();
    partial
        .apply_batch(
            &[block(
                1,
                100,
                [0; 32],
                10,
                99,
                vec![output(2, 10000, vec![])],
            )],
            true,
        )
        .unwrap();
    assert!(items(&partial).is_empty());
}
