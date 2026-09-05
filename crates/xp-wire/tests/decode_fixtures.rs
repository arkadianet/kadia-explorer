use xp_wire::{decode_block, tree_info, TreeKind};

fn fixture(h: u32) -> String {
    std::fs::read_to_string(format!(
        "{}/../../tests/fixtures/blocks/{h}.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[test]
fn decodes_real_block() {
    let b = decode_block(&fixture(1866000)).unwrap();
    assert_eq!(b.header.height, 1866000);
    assert!(!b.txs.is_empty());
    let coinbase = &b.txs[0];
    assert!(!coinbase.inputs.is_empty());
    // every output's tx_id and index are consistent with its parent tx
    for tx in &b.txs {
        for (i, o) in tx.outputs.iter().enumerate() {
            assert_eq!(o.tx_id, tx.id);
            assert_eq!(o.index as usize, i);
            assert!(o.size > 0 && o.size <= 4096);
        }
    }
}

#[test]
fn chain_links() {
    let a = decode_block(&fixture(1866000)).unwrap();
    let b = decode_block(&fixture(1866001)).unwrap();
    assert_eq!(b.header.parent_id, a.header.id);
}

#[test]
fn p2pk_tree_gives_9_address() {
    // ergoTree of a real P2PK output taken from tests/fixtures/blocks/1866000.json
    let tree =
        hex::decode("0008cd033e299a9add2321db9220fd34d41b75ce6a2dd0564fd6032205c39b32ef59da98")
            .unwrap();
    let info = tree_info(&tree).unwrap();
    assert!(matches!(info.kind, TreeKind::P2pk));
    assert!(info.address.starts_with('9'));
    assert_eq!(
        info.template_hash,
        xp_wire::template_hash_of(&tree).unwrap()
    );
}
