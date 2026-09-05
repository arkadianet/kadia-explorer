use xp_store::Store;
use xp_wire::decode_block;

fn fixture(h: u32) -> xp_wire::DecodedBlock {
    decode_block(
        &std::fs::read_to_string(format!(
            "{}/../../tests/fixtures/blocks/{h}.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn apply_then_rollback_is_identity() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("x.redb")).unwrap();
    s.seed_for_tests(1865999, fixture(1866000).header.parent_id.0)
        .unwrap();
    s.apply_batch(&[fixture(1866000)], true).unwrap();
    let before = s.fingerprint().unwrap();
    s.apply_batch(&[fixture(1866001), fixture(1866002)], true)
        .unwrap();
    assert_ne!(s.fingerprint().unwrap(), before);
    s.rollback_to(1866000).unwrap();
    assert_eq!(s.fingerprint().unwrap(), before);
    assert_eq!(s.indexed_height().unwrap(), Some(1866000));
    // and re-applying works (gidx counters restored)
    s.apply_batch(&[fixture(1866001)], true).unwrap();
}

#[test]
fn rollback_beyond_window_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("x.redb")).unwrap();
    s.seed_for_tests(1865999, fixture(1866000).header.parent_id.0)
        .unwrap();
    s.apply_batch(&[fixture(1866000)], true).unwrap();
    let err = s
        .rollback_to(1865999 - xp_store::ROLLBACK_WINDOW - 5)
        .unwrap_err();
    assert!(matches!(err, xp_store::StoreError::ReindexRequired(_)));
}
