use xp_store::Store;
use xp_types::rent::maturity_height;
use xp_wire::{decode_genesis_boxes, DecodedBox};

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

fn genesis_boxes() -> Vec<DecodedBox> {
    decode_genesis_boxes(
        &std::fs::read_to_string(format!(
            "{}/../../tests/fixtures/genesis.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn seeds_genesis_boxes_into_an_empty_store() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("x.redb")).unwrap();
    assert!(!s.genesis_seeded().unwrap());

    let boxes = genesis_boxes();
    s.seed_genesis(&boxes).unwrap();
    assert!(s.genesis_seeded().unwrap());
    // Genesis is not a block: it must not make the store look "indexed".
    assert_eq!(s.indexed_height().unwrap(), None);

    let rd = xp_store::Reader::new(&s).unwrap();
    for b in &boxes {
        let row = rd.box_by_id(&b.id.0).unwrap().expect("genesis box indexed");
        assert!(row.spent.is_none());
        assert_eq!(row.value, b.value);
        assert_eq!(row.creation_height, 0);
        assert!(rd.tree_row(&b.tree_hash.0).unwrap().is_some());
    }

    // The emission box is the largest of the three and sits alone on its tree.
    let emission = boxes.iter().max_by_key(|b| b.value).unwrap();
    let bal = rd.balance(&emission.tree_hash.0).unwrap().expect("balance");
    assert_eq!(bal.nano, emission.value);
    assert_eq!(bal.box_count, 1);
    assert_eq!(bal.first_seen, 0);
    assert_eq!(bal.last_seen, 0);

    // Genesis boxes belong to no transaction: all-zero tx_id, index 0, and no TXS row for
    // that id (so a lookup of it finds nothing rather than a phantom transaction).
    for b in &boxes {
        assert_eq!(b.tx_id.0, [0u8; 32]);
    }
    assert!(rd.tx_by_id(&[0u8; 32]).unwrap().is_none());

    let matures = rd.rent_matures_range(maturity_height(0), 1, 10).unwrap();
    assert_eq!(matures.len(), 3);
    for b in &boxes {
        assert!(matures
            .iter()
            .any(|(h, id)| *h == maturity_height(0) && *id == b.id.0));
    }
}

#[test]
fn second_seed_is_a_no_op() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("x.redb")).unwrap();
    let boxes = genesis_boxes();
    s.seed_genesis(&boxes).unwrap();
    let fp = s.fingerprint().unwrap();
    s.seed_genesis(&boxes).unwrap();
    assert_eq!(s.fingerprint().unwrap(), fp);
}

#[test]
fn seeding_a_non_empty_store_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("x.redb")).unwrap();
    s.seed_for_tests(1_865_999, [7u8; 32]).unwrap();
    let err = s.seed_genesis(&genesis_boxes()).unwrap_err();
    assert!(
        matches!(
            err,
            xp_store::StoreError::Corrupt("genesis seeding on a non-empty store")
        ),
        "unexpected error: {err}"
    );
}

/// A store that indexed blocks while being neither genesis-seeded nor explicitly partial
/// predates genesis seeding: its genesis-tree balances were built on the old "tolerate a
/// missing input at height 1" rule and are wrong. The schema version cannot distinguish it
/// (the table layout never changed), so `Store::open` detects it from the meta keys.
#[test]
fn a_store_predating_genesis_seeding_is_refused_at_open() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.redb");
    {
        let s = Store::open(&path).unwrap();
        // Writes META_INDEXED_HEIGHT and nothing else — exactly the old code's footprint.
        s.seed_header_only_for_tests(1_000, [9u8; 32]).unwrap();
    }
    let err = match Store::open(&path) {
        Ok(_) => panic!("expected Store::open to refuse a pre-genesis-seeding store"),
        Err(e) => e,
    };
    assert!(
        matches!(
            err,
            xp_store::StoreError::Corrupt(
                "store predates genesis seeding; delete explorer.redb and resync"
            )
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn fresh_seeded_and_partial_stores_still_open() {
    // Fresh: no indexed height at all.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fresh.redb");
    drop(Store::open(&path).unwrap());
    Store::open(&path).unwrap();

    // Genesis-seeded.
    let path = dir.path().join("seeded.redb");
    {
        let s = Store::open(&path).unwrap();
        s.seed_genesis(&genesis_boxes()).unwrap();
    }
    Store::open(&path).unwrap();

    // Partial (seeded from a later height): missing genesis boxes are expected there.
    let path = dir.path().join("partial.redb");
    {
        let s = Store::open(&path).unwrap();
        s.seed_for_tests(1_000, [9u8; 32]).unwrap();
    }
    Store::open(&path).unwrap();
}

/// `first_seen == 0` means "genesis", not "unset". A later block that credits one of the
/// chain-spec trees must leave its `first_seen` at 0 rather than overwrite it with that
/// block's height.
#[test]
fn a_later_credit_does_not_overwrite_a_genesis_trees_first_seen() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("x.redb")).unwrap();
    let boxes = genesis_boxes();
    s.seed_genesis(&boxes).unwrap();
    let tree = boxes[0].tree_hash.0;

    // A synthetic height-1 block with a single input-less tx paying a fresh box to the same
    // tree as genesis box 0. (Box/tx ids need not be consensus-valid: the store never
    // recomputes them.)
    let mut b = fixture(1866000);
    b.header.height = 1;
    b.header.parent_id = xp_types::HeaderId([0u8; 32]);
    let mut out = boxes[0].clone();
    out.id = xp_types::BoxId([0xA1u8; 32]);
    out.value = 1;
    let mut tx = b.txs[0].clone();
    tx.id = xp_types::TxId([0xA2u8; 32]);
    tx.inputs = vec![];
    tx.outputs = vec![out];
    b.txs = vec![tx];

    s.apply_batch(&[b], true).unwrap();

    let rd = xp_store::Reader::new(&s).unwrap();
    let bal = rd.balance(&tree).unwrap().expect("genesis tree balance");
    assert_eq!(
        bal.first_seen, 0,
        "genesis first_seen must survive a credit"
    );
    assert_eq!(bal.last_seen, 1);
}
