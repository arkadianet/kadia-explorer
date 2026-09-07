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

#[test]
fn missing_input_at_height_one_is_corruption_on_a_non_partial_store() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("x.redb")).unwrap();
    let boxes = xp_wire::decode_genesis_boxes(
        &std::fs::read_to_string(format!(
            "{}/../../tests/fixtures/genesis.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap(),
    )
    .unwrap();
    s.seed_genesis(&boxes).unwrap();

    // A synthetic height-1 block whose only tx spends a box nobody ever created. Once the
    // real genesis boxes are seeded there is no reason left for height 1 to tolerate an
    // unknown input, so this must be corruption rather than a silently skipped input.
    let mut b = fixture(1866000);
    b.header.height = 1;
    b.header.parent_id = xp_types::HeaderId([0u8; 32]);
    let mut tx = b.txs[0].clone();
    tx.inputs = vec![xp_types::BoxId([0x5Au8; 32])];
    tx.outputs = vec![];
    b.txs = vec![tx];

    let err = s.apply_batch(&[b], true).unwrap_err();
    assert!(
        matches!(err, xp_store::StoreError::Corrupt("input box missing")),
        "unexpected error: {err}"
    );
    assert_eq!(s.indexed_height().unwrap(), None); // txn rolled back
}

/// Block 1866000's tx 0 is the emission tx: it spends the emission box and outputs
/// `[re-created emission box (1_412_124 ERG), miner reward (12 ERG)]`. The header's `reward`
/// must be the miner's 12 ERG, not the emission remainder.
#[test]
fn header_reward_is_the_miner_share_not_the_emission_remainder() {
    let dir = tempfile::tempdir().unwrap();
    let s = seeded_store(dir.path());
    let b = fixture(1866000);
    s.apply_batch(std::slice::from_ref(&b), true).unwrap();
    let rd = xp_store::Reader::new(&s).unwrap();
    let h = rd.header_at(1866000).unwrap().expect("header indexed");
    assert_eq!(h.reward, 12_000_000_000);
    // Sanity: the naive "sum every tx 0 output" would have been three orders of magnitude
    // larger, so the assertion above is really testing the split.
    let all_outputs: u64 = b.txs[0].outputs.iter().map(|o| o.value).sum();
    assert!(all_outputs > 1_000_000 * 1_000_000_000);

    // 1866000 exercised the fallback (its tx 0 input predates this partial store's seed
    // point). 1866001's tx 0 spends the emission box block 1866000 created, so it exercises
    // the primary path: the input box resolves and its tree identifies the emission output.
    s.apply_batch(&[fixture(1866001)], true).unwrap();
    let rd = xp_store::Reader::new(&s).unwrap();
    let h1 = rd.header_at(1866001).unwrap().expect("header indexed");
    assert_eq!(h1.reward, 12_000_000_000);
}

/// The stored [`xp_store::FEE_TREE_HASH`] must really be the blake2b256 of the mainnet
/// miner-fee contract's serialized ergo tree — the whole fee computation hangs off it, and a
/// wrong constant would silently make every fee 0 again.
#[test]
fn fee_tree_hash_is_the_hash_of_the_miner_fee_tree() {
    let bytes = hex::decode(xp_store::FEE_TREE_HEX).unwrap();
    assert_eq!(xp_wire::tree_hash(&bytes).0, xp_store::FEE_TREE_HASH);
    assert!(xp_store::is_fee_tree(&xp_store::FEE_TREE_HASH));
    assert!(!xp_store::is_fee_tree(&[0u8; 32]));
    // Every fee-paying tx in the fixture block creates an output on exactly this tree.
    let b = fixture(1866000);
    assert!(b.txs[1]
        .outputs
        .iter()
        .any(|o| o.tree_hash.0 == xp_store::FEE_TREE_HASH));
}

/// On Ergo a fee is an *output* to the miner-fee contract, not `value_in - value_out` (which
/// is identically zero, every tx being value-balanced). These are the exact nanoERG amounts
/// carried by block 1866000's fee outputs.
#[test]
fn tx_fee_is_the_sum_of_its_miner_fee_outputs() {
    let dir = tempfile::tempdir().unwrap();
    let s = seeded_store(dir.path());
    s.apply_batch(&[fixture(1866000)], true).unwrap();
    let rd = xp_store::Reader::new(&s).unwrap();
    let txs = rd.txs_in_block(1866000).unwrap();
    assert_eq!(txs.len(), 16);

    let fee = |i: usize| txs.iter().find(|(_, r)| r.index == i as u16).unwrap().1.fee;
    // tx 0 is the emission tx: no fee output, so no fee.
    assert_eq!(fee(0), 0);
    assert_eq!(fee(1), 1_500_000);
    assert_eq!(fee(11), 1_000_000);
    assert_eq!(fee(13), 1_100_000);
    assert_eq!(fee(14), 8_000_000);
    // The last tx of a block is the fee-collection tx: it *spends* every fee box into the
    // miner's box and creates no fee output of its own, so its own fee is 0.
    assert_eq!(fee(15), 0);
}

/// `HeaderRow::fees` is the sum of its txs' fees, and — the fee-collection invariant — that
/// sum is exactly what the block's last tx pays out to the miner.
#[test]
fn block_fees_sum_tx_fees_and_match_the_fee_collection_tx() {
    let dir = tempfile::tempdir().unwrap();
    let s = seeded_store(dir.path());
    let blocks = [fixture(1866000), fixture(1866001), fixture(1866002)];
    s.apply_batch(&blocks, true).unwrap();
    let rd = xp_store::Reader::new(&s).unwrap();

    let expected = [26_600_000u64, 13_600_000, 2_500_000];
    for (b, want) in blocks.iter().zip(expected) {
        let h = b.header.height;
        let hdr = rd.header_at(h).unwrap().expect("header indexed");
        let txs = rd.txs_in_block(h).unwrap();
        let sum: u64 = txs.iter().map(|(_, r)| r.fee).sum();
        assert_eq!(hdr.fees, want, "block {h} fees");
        assert_eq!(sum, want, "block {h} tx fee sum");
        // The fee-collection tx's single output holds precisely the collected fees.
        let last = b.txs.last().unwrap();
        let collected: u64 = last.outputs.iter().map(|o| o.value).sum();
        assert_eq!(collected, want, "block {h} fee-collection output");
        assert!(hdr.fees > 0);
    }
}

/// Reads the raw hex of R4 out of a box's stored `registers_json` (this crate's own
/// canonical `{"R4":"<hex>",…}` rendering — hex values never contain a quote).
fn r4_hex(registers_json: &str) -> Option<&str> {
    let (_, after) = registers_json.split_once("\"R4\":\"")?;
    let end = after.find('"')?;
    Some(&after[..end])
}

fn all_outputs(blocks: &[xp_wire::DecodedBlock]) -> Vec<&xp_wire::DecodedBox> {
    blocks
        .iter()
        .flat_map(|b| b.txs.iter())
        .flat_map(|t| t.outputs.iter())
        .collect()
}

/// `TEMPLATES` must count every box carrying a template, and `TEMPLATE_UNSPENT` must hold
/// exactly the ones still unspent — cross-checked against the fixture blocks themselves and
/// against the key counts in `TEMPLATE_BOXES`/`TEMPLATE_UNSPENT`.
#[test]
fn template_index_counts_boxes_and_unspent() {
    let dir = tempfile::tempdir().unwrap();
    let s = seeded_store(dir.path());
    let blocks = [fixture(1866000), fixture(1866001), fixture(1866002)];
    s.apply_batch(&blocks, true).unwrap();

    // The template carried by the most boxes in the fixtures. (A plain P2PK tree is NOT
    // constant-segregated, so its "template" still contains the public key and is unique per
    // address — the shared templates are the segregated contract scripts.)
    let mut counts: std::collections::HashMap<[u8; 32], usize> = Default::default();
    for o in all_outputs(&blocks) {
        if let Ok(h) = xp_wire::template_hash_of(&o.tree_bytes) {
            *counts.entry(h).or_default() += 1;
        }
    }
    let tmpl = *counts
        .iter()
        .max_by_key(|(h, n)| (**n, **h))
        .expect("some template")
        .0;

    let outs: Vec<_> = all_outputs(&blocks)
        .into_iter()
        .filter(|o| xp_wire::template_hash_of(&o.tree_bytes).ok() == Some(tmpl))
        .collect();
    let first_tree = outs[0].tree_hash.0;
    assert!(outs.len() > 1, "expected several boxes on this template");
    let spent_within = outs
        .iter()
        .filter(|o| {
            blocks
                .iter()
                .flat_map(|b| b.txs.iter())
                .any(|t| t.inputs.contains(&o.id))
        })
        .count();
    assert!(spent_within > 0, "expected at least one same-run spend");

    let txn = s.begin_read().unwrap();
    let templates = txn.open_table(xp_store::tables::TEMPLATES).unwrap();
    let row = xp_store::rows::TemplateRow::decode(
        templates
            .get(tmpl.as_slice())
            .unwrap()
            .expect("template row indexed")
            .value(),
    )
    .unwrap();
    assert_eq!(row.box_count, outs.len() as u64);
    assert_eq!(row.unspent_count, (outs.len() - spent_within) as u64);
    assert_eq!(row.first_seen, 1866000);
    assert_eq!(row.example_tree, first_tree);

    // The counters must agree with the composite-key indexes they summarise.
    let (lo, hi) = xp_store::keys::prefix_range(tmpl.as_slice());
    let count = |t: xp_store::tables::Tbl| {
        txn.open_table(t)
            .unwrap()
            .range(lo.as_slice()..hi.as_slice())
            .unwrap()
            .count() as u64
    };
    assert_eq!(count(xp_store::tables::TEMPLATE_BOXES), row.box_count);
    assert_eq!(count(xp_store::tables::TEMPLATE_UNSPENT), row.unspent_count);
}

/// Every register present on an output is indexed under `(reg, blake2b256(raw), gidx)`.
#[test]
fn register_index_holds_the_r4_value_key() {
    let dir = tempfile::tempdir().unwrap();
    let s = seeded_store(dir.path());
    let blocks = [fixture(1866000), fixture(1866001), fixture(1866002)];
    s.apply_batch(&blocks, true).unwrap();

    let rd = xp_store::Reader::new(&s).unwrap();
    let mut checked = 0;
    let txn = s.begin_read().unwrap();
    let reg_idx = txn.open_table(xp_store::tables::REGISTER_IDX).unwrap();
    for o in all_outputs(&blocks) {
        let Some(hex_str) = r4_hex(&o.registers_json) else {
            continue;
        };
        let raw = hex::decode(hex_str).unwrap();
        let gidx = rd.box_by_id(&o.id.0).unwrap().expect("box indexed").gidx;
        let k = xp_store::keys::k_register(4, &xp_wire::tree::blake2b256(&raw), gidx);
        assert!(
            reg_idx.get(k.as_slice()).unwrap().is_some(),
            "REGISTER_IDX missing R4 key for box {}",
            hex::encode(o.id.0)
        );
        checked += 1;
    }
    assert!(checked > 0, "no fixture output carries an R4");

    // A value that no box holds must not be indexed.
    let absent = xp_store::keys::k_register(4, &xp_wire::tree::blake2b256(b"nope"), 0);
    assert!(reg_idx.get(absent.as_slice()).unwrap().is_none());
}

/// `BalanceRow::tx_count` counts the transactions that touch a tree — through their inputs
/// or their outputs — exactly once each, not once per box.
#[test]
fn tx_count_counts_each_touching_tx_once() {
    let dir = tempfile::tempdir().unwrap();
    let s = seeded_store(dir.path());
    let blocks = [fixture(1866000), fixture(1866001), fixture(1866002)];
    s.apply_batch(&blocks, true).unwrap();
    let rd = xp_store::Reader::new(&s).unwrap();

    // The miner's reward tree: block 1866000's emission tx pays it, and so does every
    // block's fee-collection tx, so it is touched by more than one tx per block.
    let tree = blocks[0].txs[0].outputs[1].tree_hash.0;
    let mut expected = 0u64;
    for tx in blocks.iter().flat_map(|b| b.txs.iter()) {
        let by_output = tx.outputs.iter().any(|o| o.tree_hash.0 == tree);
        let by_input = tx.inputs.iter().any(|i| {
            rd.box_by_id(&i.0)
                .unwrap()
                .map(|r| r.tree_hash == tree)
                .unwrap_or(false)
        });
        if by_output || by_input {
            expected += 1;
        }
    }
    assert!(expected > 1);
    let bal = rd.balance(&tree).unwrap().expect("balance row");
    assert_eq!(bal.tx_count, expected);
}

/// Spending a box drops its `TEMPLATE_UNSPENT` key and decrements the template's
/// `unspent_count`, while `TEMPLATE_BOXES` and `box_count` are untouched.
#[test]
fn spending_a_box_decrements_its_template_unspent_count() {
    let dir = tempfile::tempdir().unwrap();
    let s = seeded_store(dir.path());
    let b0 = fixture(1866000);
    let b1 = fixture(1866001);
    let spent = b0.txs[0].outputs[0].clone();
    assert_eq!(b1.txs[0].inputs[0], spent.id);
    let tmpl = xp_wire::template_hash_of(&spent.tree_bytes).unwrap();

    s.apply_batch(std::slice::from_ref(&b0), true).unwrap();
    let gidx = xp_store::Reader::new(&s)
        .unwrap()
        .box_by_id(&spent.id.0)
        .unwrap()
        .expect("box indexed")
        .gidx;
    let read_row = |s: &Store| {
        let txn = s.begin_read().unwrap();
        let t = txn.open_table(xp_store::tables::TEMPLATES).unwrap();
        xp_store::rows::TemplateRow::decode(t.get(tmpl.as_slice()).unwrap().unwrap().value())
            .unwrap()
    };
    let before = read_row(&s);
    {
        let txn = s.begin_read().unwrap();
        let tu = txn.open_table(xp_store::tables::TEMPLATE_UNSPENT).unwrap();
        assert!(tu
            .get(xp_store::keys::k_hash_gidx(&tmpl, gidx).as_slice())
            .unwrap()
            .is_some());
    }

    s.apply_batch(std::slice::from_ref(&b1), true).unwrap();

    let created = b1
        .txs
        .iter()
        .flat_map(|t| t.outputs.iter())
        .filter(|o| xp_wire::template_hash_of(&o.tree_bytes).ok() == Some(tmpl))
        .count() as u64;
    // Inputs of b1 that resolve to a box on this same template (at minimum `spent`).
    let rd = xp_store::Reader::new(&s).unwrap();
    let spent_here = b1
        .txs
        .iter()
        .flat_map(|t| t.inputs.iter())
        .filter(|i| {
            rd.box_by_id(&i.0)
                .unwrap()
                .and_then(|r| rd.tree_row(&r.tree_hash).unwrap())
                .map(|t| t.template_hash == tmpl)
                .unwrap_or(false)
        })
        .count() as u64;
    assert!(spent_here >= 1);
    let after = read_row(&s);
    assert_eq!(after.box_count, before.box_count + created);
    assert_eq!(
        after.unspent_count,
        before.unspent_count + created - spent_here
    );

    let txn = s.begin_read().unwrap();
    let k = xp_store::keys::k_hash_gidx(&tmpl, gidx);
    assert!(txn
        .open_table(xp_store::tables::TEMPLATE_UNSPENT)
        .unwrap()
        .get(k.as_slice())
        .unwrap()
        .is_none());
    assert!(txn
        .open_table(xp_store::tables::TEMPLATE_BOXES)
        .unwrap()
        .get(k.as_slice())
        .unwrap()
        .is_some());
}
