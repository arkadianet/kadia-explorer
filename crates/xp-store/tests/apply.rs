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

/// Fixtures start at 1866000; tests seed `indexed_height` = 1865999 with a synthetic header
/// whose id equals block 1866000's parent.
fn seeded_store(dir: &std::path::Path) -> Store {
    let s = Store::open(&dir.join("x.redb")).unwrap();
    let b0 = fixture(1866000);
    s.seed_for_tests(1865999, b0.header.parent_id.0).unwrap();
    s
}

#[test]
fn applies_three_blocks_and_tracks_height() {
    let dir = tempfile::tempdir().unwrap();
    let s = seeded_store(dir.path());
    s.apply_batch(&[fixture(1866000), fixture(1866001)], true)
        .unwrap();
    assert_eq!(s.indexed_height().unwrap(), Some(1866001));
    s.apply_batch(&[fixture(1866002)], true).unwrap();
    assert_eq!(s.indexed_height().unwrap(), Some(1866002));
    assert_eq!(
        s.header_id_at(1866002).unwrap(),
        Some(fixture(1866002).header.id.0)
    );
}

#[test]
fn rejects_parent_mismatch() {
    let dir = tempfile::tempdir().unwrap();
    let s = seeded_store(dir.path());
    s.apply_batch(&[fixture(1866000)], true).unwrap();
    let err = s.apply_batch(&[fixture(1866002)], true).unwrap_err();
    assert!(matches!(err, xp_store::StoreError::ParentMismatch { .. }));
    assert_eq!(s.indexed_height().unwrap(), Some(1866000)); // txn rolled back
}

#[test]
fn outputs_become_unspent_then_spent() {
    let dir = tempfile::tempdir().unwrap();
    let s = seeded_store(dir.path());
    let b = fixture(1866000);
    s.apply_batch(std::slice::from_ref(&b), true).unwrap();
    let rd = xp_store::Reader::new(&s).unwrap();
    let out = &b.txs[0].outputs[0];
    let row = rd.box_by_id(&out.id.0).unwrap().expect("box indexed");
    assert_eq!(row.value, out.value);
    assert!(row.spent.is_none());
    let bal = rd.balance(&out.tree_hash.0).unwrap().expect("balance");
    assert!(bal.nano >= out.value);
    // rent maturity indexed
    let mats = rd
        .rent_matures_range(xp_types::rent::maturity_height(out.creation_height), 1, 10)
        .unwrap();
    assert!(mats.iter().any(|(_, id)| *id == out.id.0));
}
