use std::collections::HashSet;
use xp_store::read::Dir;
use xp_store::{Reader, Store};
use xp_types::Hash32;

fn fixture(h: u32) -> xp_wire::DecodedBlock {
    xp_wire::decode_block(
        &std::fs::read_to_string(format!(
            "{}/../../tests/fixtures/blocks/{h}.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap(),
    )
    .unwrap()
}

/// Seeds a store and applies all three fixture blocks (1866000..=1866002), matching the
/// convention used by `tests/apply.rs`.
fn seeded_and_applied() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("x.redb")).unwrap();
    let b0 = fixture(1866000);
    s.seed_for_tests(1865999, b0.header.parent_id.0).unwrap();
    s.apply_batch(
        &[fixture(1866000), fixture(1866001), fixture(1866002)],
        true,
    )
    .unwrap();
    (dir, s)
}

#[test]
fn headers_desc_orders_newest_first_and_respects_before_height() {
    let (_dir, s) = seeded_and_applied();
    let rd = Reader::new(&s).unwrap();

    // Limit to exactly the 3 fixture blocks; the store also carries a synthetic seed header
    // at 1865999 (see `Store::seed_for_tests`), which is not part of this fixture set.
    let all = rd.headers_desc(None, 3).unwrap();
    let heights: Vec<u32> = all.iter().map(|(h, _)| *h).collect();
    assert_eq!(heights, vec![1866002, 1866001, 1866000]);

    let before = rd.headers_desc(Some(1866002), 2).unwrap();
    let heights: Vec<u32> = before.iter().map(|(h, _)| *h).collect();
    assert_eq!(heights, vec![1866001, 1866000]);

    let limited = rd.headers_desc(None, 1).unwrap();
    assert_eq!(limited.len(), 1);
    assert_eq!(limited[0].0, 1866002);
}

#[test]
fn tree_boxes_asc_and_desc_walk_all_boxes_of_a_tree_with_limit_one() {
    let (_dir, s) = seeded_and_applied();
    let rd = Reader::new(&s).unwrap();

    let coinbase_tree = fixture(1866000).txs[0].outputs[0].tree_hash.0;

    // Expected set: every output across the three fixtures on this tree.
    let mut expected: HashSet<Hash32> = HashSet::new();
    for h in [1866000, 1866001, 1866002] {
        for tx in &fixture(h).txs {
            for o in &tx.outputs {
                if o.tree_hash.0 == coinbase_tree {
                    expected.insert(o.id.0);
                }
            }
        }
    }
    assert!(!expected.is_empty());

    // Ascending, one page at a time.
    let mut asc_ids = HashSet::new();
    let mut cursor = None;
    loop {
        let page = rd
            .tree_boxes(&coinbase_tree, false, cursor, 1, Dir::Asc)
            .unwrap();
        if page.items.is_empty() {
            break;
        }
        for (id, _) in &page.items {
            asc_ids.insert(*id);
        }
        match page.next_cursor {
            Some(c) => cursor = Some(c),
            None => break,
        }
    }
    assert_eq!(asc_ids, expected);

    // Descending, one page at a time.
    let mut desc_ids = HashSet::new();
    let mut cursor = None;
    loop {
        let page = rd
            .tree_boxes(&coinbase_tree, false, cursor, 1, Dir::Desc)
            .unwrap();
        if page.items.is_empty() {
            break;
        }
        for (id, _) in &page.items {
            desc_ids.insert(*id);
        }
        match page.next_cursor {
            Some(c) => cursor = Some(c),
            None => break,
        }
    }
    assert_eq!(desc_ids, expected);
}

#[test]
fn boxes_of_tx_returns_output_count_rows_in_index_order() {
    let (_dir, s) = seeded_and_applied();
    let rd = Reader::new(&s).unwrap();

    let b0 = fixture(1866000);
    let tx = &b0.txs[0];
    let row = rd.tx_by_id(&tx.id.0).unwrap().expect("tx indexed");
    assert_eq!(row.output_count as usize, tx.outputs.len());

    let boxes = rd.boxes_of_tx(&row, &tx.id.0).unwrap();
    assert_eq!(boxes.len(), tx.outputs.len());
    for (i, (id, box_row)) in boxes.iter().enumerate() {
        assert_eq!(*id, tx.outputs[i].id.0);
        assert_eq!(box_row.index, i as u16);
        assert_eq!(box_row.value, tx.outputs[i].value);
    }
}

#[test]
fn richlist_first_entry_is_the_max_balance() {
    let (_dir, s) = seeded_and_applied();
    let rd = Reader::new(&s).unwrap();

    // Compute the max directly from TREE_BALANCE.
    let txn = s.begin_read().unwrap();
    let table = txn.open_table(xp_store::tables::TREE_BALANCE).unwrap();
    let mut max_nano = 0u64;
    for item in table.range::<&[u8]>(..).unwrap() {
        let (_, v) = item.unwrap();
        let bal = xp_store::rows::BalanceRow::decode(v.value()).unwrap();
        max_nano = max_nano.max(bal.nano);
    }
    assert!(max_nano > 0);

    let (items, _next) = rd.richlist(None, 1).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].1, max_nano);
}

#[test]
fn rent_eligible_includes_unspent_and_excludes_spent() {
    let (_dir, s) = seeded_and_applied();
    let rd = Reader::new(&s).unwrap();

    let mut created: HashSet<Hash32> = HashSet::new();
    let mut spent: HashSet<Hash32> = HashSet::new();
    for h in [1866000, 1866001, 1866002] {
        let b = fixture(h);
        for tx in &b.txs {
            for o in &tx.outputs {
                created.insert(o.id.0);
            }
            for i in &tx.inputs {
                spent.insert(i.0);
            }
        }
    }
    let expected: HashSet<Hash32> = created.difference(&spent).copied().collect();
    assert!(!expected.is_empty());

    let at_height = 1866002 + xp_types::rent::RENT_PERIOD;
    let (items, _next) = rd.rent_eligible(at_height, None, 10_000).unwrap();
    let got: HashSet<Hash32> = items.iter().map(|(_, id)| *id).collect();
    assert_eq!(got, expected);

    // Spent boxes must not appear.
    for id in &spent {
        assert!(!got.contains(id));
    }
}

#[test]
fn tree_by_address_round_trips_through_tree_row() {
    let (_dir, s) = seeded_and_applied();
    let rd = Reader::new(&s).unwrap();

    let coinbase_tree = fixture(1866000).txs[0].outputs[0].tree_hash.0;
    let row = rd.tree_row(&coinbase_tree).unwrap().expect("tree indexed");

    let found = rd.tree_by_address(&row.address).unwrap();
    assert_eq!(found, Some(coinbase_tree));
}

#[test]
fn txs_in_block_returns_exactly_that_blocks_txs_in_order() {
    let (_dir, s) = seeded_and_applied();
    let rd = Reader::new(&s).unwrap();

    let b1 = fixture(1866001);
    let got = rd.txs_in_block(1866001).unwrap();
    let got_ids: Vec<Hash32> = got.iter().map(|(id, _)| *id).collect();
    let want_ids: Vec<Hash32> = b1.txs.iter().map(|t| t.id.0).collect();
    assert_eq!(got_ids, want_ids);
}
