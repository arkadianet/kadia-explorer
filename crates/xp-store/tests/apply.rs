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

#[test]
fn missing_input_on_a_non_partial_store_is_corruption() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("x.redb")).unwrap();
    let b0 = fixture(1866000);
    // Seed only the tip header — unlike `seeded_store`, this does NOT mark the store partial,
    // so it behaves like a store that fully synced from genesis and simply lost a box: any
    // missing input past height 1 must be corruption, not silently tolerated.
    s.seed_header_only_for_tests(1865999, b0.header.parent_id.0)
        .unwrap();
    let err = s.apply_batch(&[b0], true).unwrap_err();
    assert!(matches!(
        err,
        xp_store::StoreError::Corrupt("input box missing")
    ));
    assert_eq!(s.indexed_height().unwrap(), Some(1865999)); // txn rolled back
}

#[test]
fn spends_a_box_across_blocks_and_updates_all_indexes() {
    let dir = tempfile::tempdir().unwrap();
    let s = seeded_store(dir.path());
    let b0 = fixture(1866000);
    let b1 = fixture(1866001);

    // Box created by block 1866000's tx 0 output 0 is spent by block 1866001's tx 0 — the
    // emission-box chain, which guarantees a cross-block spend exists in every pair of
    // consecutive blocks.
    let spent_box = b0.txs[0].outputs[0].clone();
    let spending_tx = b1.txs[0].clone();
    assert_eq!(spending_tx.inputs[0], spent_box.id);

    s.apply_batch(std::slice::from_ref(&b0), true).unwrap();
    let bal_before = xp_store::Reader::new(&s)
        .unwrap()
        .balance(&spent_box.tree_hash.0)
        .unwrap()
        .expect("tree balance after block 1866000");

    s.apply_batch(std::slice::from_ref(&b1), true).unwrap();

    let rd = xp_store::Reader::new(&s).unwrap();
    let row = rd
        .box_by_id(&spent_box.id.0)
        .unwrap()
        .expect("spent box still indexed");
    assert_eq!(row.spent, Some((spending_tx.id.0, 1866001)));

    let txn = s.begin_read().unwrap();
    let tree_unspent = txn.open_table(xp_store::tables::TREE_UNSPENT).unwrap();
    let tree_boxes = txn.open_table(xp_store::tables::TREE_BOXES).unwrap();
    let rent_matures = txn.open_table(xp_store::tables::RENT_MATURES).unwrap();
    let ku = xp_store::keys::k_hash_gidx(&spent_box.tree_hash.0, row.gidx);
    assert!(
        tree_unspent.get(ku.as_slice()).unwrap().is_none(),
        "TREE_UNSPENT key must be removed once the box is spent"
    );
    assert!(
        tree_boxes.get(ku.as_slice()).unwrap().is_some(),
        "TREE_BOXES key must remain (it indexes all boxes, not just unspent ones)"
    );
    let kr = xp_store::keys::k_rent(
        xp_types::rent::maturity_height(spent_box.creation_height),
        row.gidx,
    );
    assert!(
        rent_matures.get(kr.as_slice()).unwrap().is_none(),
        "RENT_MATURES key must be removed once the box is spent"
    );

    // Net effect of the spending tx on the tree's balance: it loses `spent_box.value` and
    // regains whatever of that same tx's own outputs land back on the same tree (the emission
    // chain re-credits itself a fresh box, minus the amount routed elsewhere).
    let credited: u64 = spending_tx
        .outputs
        .iter()
        .filter(|o| o.tree_hash == spent_box.tree_hash)
        .map(|o| o.value)
        .sum();
    let bal_after = rd
        .balance(&spent_box.tree_hash.0)
        .unwrap()
        .expect("tree balance after block 1866001");
    assert_eq!(bal_after.nano, bal_before.nano - spent_box.value + credited);

    // UNDO recorded for both applied heights.
    let undo = txn.open_table(xp_store::tables::UNDO).unwrap();
    assert!(undo
        .get(xp_store::keys::k_u32(1866000).as_slice())
        .unwrap()
        .is_some());
    assert!(undo
        .get(xp_store::keys::k_u32(1866001).as_slice())
        .unwrap()
        .is_some());

    // META next_box_gidx equals the total output count of both blocks (seeded store starts
    // its gidx counters at 0).
    let total_outputs: u64 = b0.txs.iter().map(|t| t.outputs.len() as u64).sum::<u64>()
        + b1.txs.iter().map(|t| t.outputs.len() as u64).sum::<u64>();
    let meta = txn.open_table(xp_store::tables::META).unwrap();
    let next_box_bytes = meta
        .get(xp_store::tables::META_NEXT_BOX_GIDX)
        .unwrap()
        .unwrap();
    let next_box = u64::from_be_bytes(next_box_bytes.value().try_into().unwrap());
    assert_eq!(next_box, total_outputs);
}
