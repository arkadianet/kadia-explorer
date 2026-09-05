use xp_store::Store;
use xp_types::rent::maturity_height;
use xp_wire::{decode_genesis_boxes, DecodedBox};

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
