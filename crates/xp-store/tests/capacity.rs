#[allow(dead_code)]
#[path = "support/logical.rs"]
mod logical;
use logical::snapshot;
use xp_store::{Store, StoreError};

fn blocks() -> Vec<xp_wire::DecodedBlock> {
    (1866000..=1866002)
        .map(|h| {
            let mut b = xp_wire::decode_block(
                &std::fs::read_to_string(format!(
                    "{}/../../tests/fixtures/blocks/{h}.json",
                    env!("CARGO_MANIFEST_DIR")
                ))
                .unwrap(),
            )
            .unwrap();
            for o in b.txs.iter_mut().flat_map(|t| &mut t.outputs) {
                o.registers_json = r#"{"R4":"0402","R5":"0402"}"#.into();
            }
            b
        })
        .collect()
}
fn additions(b: &[xp_wire::DecodedBlock]) -> u64 {
    b.iter()
        .flat_map(|b| &b.txs)
        .map(|t| t.outputs.len() as u64 * 2)
        .sum()
}
#[test]
fn boundaries_batch_restart_rollback_and_resume_preserve_all_bytes() {
    let b = blocks();
    let n = additions(&b[1..]);
    let first = additions(&b[..1]);
    for ceiling in [first + n - 1, first + n, first + n + 1] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.redb");
        let s = Store::open_with_register_index_ceiling(&path, Some(ceiling)).unwrap();
        s.seed_for_tests(1865999, b[0].header.parent_id.0).unwrap();
        s.apply_batch(&b[..1], true).unwrap();
        assert_eq!(s.register_index_entries().unwrap(), first);
        assert_eq!(s.cached_register_index_entries(), first);
        let before = snapshot(&s);
        assert!(!before["undo"].is_empty());
        let result = s.apply_batch(&b[1..], true);
        if ceiling <= first + n {
            assert!(
                matches!(result, Err(StoreError::RegisterCapacity { existing, additional, ceiling: c }) if existing == first && additional == n && c == ceiling)
            );
            assert_eq!(snapshot(&s), before); // exact raw bytes, INCLUDING UNDO
            assert_eq!(s.indexed_height().unwrap(), Some(1866000));
            drop(s);
            assert!(
                matches!(Store::open_with_register_index_ceiling(&path, Some(first - 1)), Err(StoreError::RegisterCapacityConfig { entries, .. }) if entries == first)
            );
            let s = Store::open_with_register_index_ceiling(&path, Some(first + n + 1)).unwrap();
            assert_eq!(snapshot(&s), before);
            s.apply_batch(&b[1..], true).unwrap();
            assert_eq!(s.register_index_entries().unwrap(), first + n);
            assert_eq!(s.cached_register_index_entries(), first + n);
            s.rollback_to(1866000).unwrap();
            assert_eq!(s.register_index_entries().unwrap(), first);
            assert_eq!(s.cached_register_index_entries(), first);
            drop(s);
            let s = Store::open_with_register_index_ceiling(&path, Some(first + n + 1)).unwrap();
            s.apply_batch(&b[1..], true).unwrap();
            assert_eq!(s.register_index_entries().unwrap(), first + n);
            assert_eq!(s.cached_register_index_entries(), first + n);
        } else {
            result.unwrap();
            // Same raw register value on every output still creates two distinct gidx rows.
            assert_eq!(s.register_index_entries().unwrap(), first + n);
            assert_eq!(s.cached_register_index_entries(), first + n);
        }
    }
}
#[test]
fn genesis_is_atomic_counted_and_idempotent_after_restart() {
    let mut boxes =
        xp_wire::decode_genesis_boxes(include_str!("../../../tests/fixtures/genesis.json"))
            .unwrap();
    for o in &mut boxes {
        o.registers_json = r#"{"R4":"0402"}"#.into();
    }
    let n = boxes.len() as u64;
    for ceiling in [n - 1, n, n + 1] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.redb");
        let s = Store::open_with_register_index_ceiling(&path, Some(ceiling)).unwrap();
        let before = snapshot(&s);
        let result = s.seed_genesis(&boxes);
        if ceiling <= n {
            assert!(matches!(result, Err(StoreError::RegisterCapacity { .. })));
            assert_eq!(snapshot(&s), before);
            assert!(!s.genesis_seeded().unwrap());
        } else {
            result.unwrap();
            assert_eq!(s.register_index_entries().unwrap(), n);
            assert_eq!(s.cached_register_index_entries(), n);
            let before = snapshot(&s);
            drop(s);
            let s = Store::open_with_register_index_ceiling(&path, Some(n)).unwrap();
            s.seed_genesis(&boxes).unwrap();
            assert_eq!(snapshot(&s), before);
        }
    }
}

#[test]
fn unconfigured_reopens_occupied_store_and_accepts_previously_rejected_batch() {
    let b = blocks();
    let first = additions(&b[..1]);
    let total = additions(&b);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.redb");
    let s = Store::open_with_register_index_ceiling(&path, Some(total)).unwrap();
    s.seed_for_tests(1865999, b[0].header.parent_id.0).unwrap();
    s.apply_batch(&b[..1], true).unwrap();
    let before = snapshot(&s);
    assert!(matches!(
        s.apply_batch(&b[1..], true),
        Err(StoreError::RegisterCapacity { .. })
    ));
    assert_eq!(snapshot(&s), before);
    drop(s);
    assert!(matches!(
        Store::open_with_register_index_ceiling(&path, Some(first - 1)),
        Err(StoreError::RegisterCapacityConfig { .. })
    ));
    let s = Store::open(&path).unwrap();
    assert_eq!(s.register_index_ceiling(), None);
    assert_eq!(snapshot(&s), before);
    s.apply_batch(&b[1..], true).unwrap();
    assert_eq!(s.register_index_entries().unwrap(), total);
    drop(s);
    let s = Store::open(&path).unwrap();
    assert_eq!(s.register_index_entries().unwrap(), total);
}
