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
#[ignore = "re-enabled in Plan 2 Task 4 (rollback of v2 tables)"]
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

/// Rolling back a block in which a box is both created and spent.
///
/// This is the case where undo bookkeeping is easiest to get wrong: box X never existed
/// before the block and is already spent by the end of it, so a naive rollback that only
/// restores previously-spent boxes (or only removes created ones) leaves a stale row behind.
/// The store fingerprint catches any such residue, in any table.
#[test]
#[ignore = "re-enabled in Plan 2 Task 4 (rollback of v2 tables)"]
fn apply_then_rollback_with_same_block_create_and_spend() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("x.redb")).unwrap();
    s.seed_for_tests(1865999, fixture(1866000).header.parent_id.0)
        .unwrap();
    s.apply_batch(
        &[fixture(1866000), fixture(1866001), fixture(1866002)],
        true,
    )
    .unwrap();
    let before = s.fingerprint().unwrap();

    // Synthetic height 1866003: fixture 1866000's header rewritten to follow 1866002, with
    // two txs — A creates box X, B spends X and creates Y. Box/tx ids need not be
    // consensus-valid; the store never recomputes them.
    let mut b = fixture(1866000);
    b.header.height = 1866003;
    b.header.parent_id = fixture(1866002).header.id;
    b.header.id = xp_types::HeaderId([0xB3u8; 32]);

    let template = b.txs[0].outputs[0].clone();
    let mut x = template.clone();
    x.id = xp_types::BoxId([0x11u8; 32]);
    x.value = 1_000_000;
    x.tx_id = xp_types::TxId([0xAAu8; 32]);
    x.index = 0;
    let mut y = template.clone();
    y.id = xp_types::BoxId([0x22u8; 32]);
    y.value = 900_000;
    y.tx_id = xp_types::TxId([0xBBu8; 32]);
    y.index = 0;

    let tx_a = xp_wire::DecodedTx {
        id: xp_types::TxId([0xAAu8; 32]),
        inputs: vec![],
        data_inputs: vec![],
        outputs: vec![x.clone()],
        size: 100,
    };
    let tx_b = xp_wire::DecodedTx {
        id: xp_types::TxId([0xBBu8; 32]),
        inputs: vec![x.id],
        data_inputs: vec![],
        outputs: vec![y],
        size: 100,
    };
    b.txs = vec![tx_a, tx_b];

    s.apply_batch(&[b], true).unwrap();
    assert_eq!(s.indexed_height().unwrap(), Some(1866003));
    assert_ne!(s.fingerprint().unwrap(), before);

    s.rollback_to(1866002).unwrap();
    assert_eq!(s.indexed_height().unwrap(), Some(1866002));
    assert_eq!(s.fingerprint().unwrap(), before);
}
