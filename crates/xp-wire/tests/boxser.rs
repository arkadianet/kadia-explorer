//! The node's `boxId` is authoritative: it is `blake2b256` of the exact `ErgoBox.bytes`
//! the chain accepted. These tests pin our own serialiser against that, including a box
//! (`e75700ef…`) whose ErgoTree is encoded non-canonically and which ergo-lib's JSON
//! deserialiser rejects.

use xp_wire::boxser::box_bytes_from_json;
use xp_wire::tree::blake2b256;
use xp_wire::{decode_block, decode_genesis_boxes};

const CANONICAL: [u32; 3] = [1866000, 1866001, 1866002];
const NON_CANONICAL: u32 = 1702686;

fn fixture(h: u32) -> String {
    std::fs::read_to_string(format!(
        "{}/../../tests/fixtures/blocks/{h}.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn fixture_json(h: u32) -> serde_json::Value {
    serde_json::from_str(&fixture(h)).unwrap()
}

fn outputs(v: &serde_json::Value) -> Vec<serde_json::Value> {
    v["blockTransactions"]["transactions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|t| t["outputs"].as_array().unwrap().clone())
        .collect()
}

fn assert_id_matches(o: &serde_json::Value) {
    let want = o["boxId"].as_str().unwrap();
    let bytes = box_bytes_from_json(o).unwrap();
    assert_eq!(
        hex::encode(blake2b256(&bytes)),
        want,
        "serialised bytes do not hash to the node's boxId for {want}"
    );
}

#[test]
fn non_canonical_box_fixture_hashes_to_its_node_box_id() {
    let raw = std::fs::read_to_string(format!(
        "{}/../../tests/fixtures/boxes/e75700ef081f64e6c5df7084840078b31d13eeb427fb3e30cef629d7cbb31394.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let o: serde_json::Value = serde_json::from_str(&raw).unwrap();
    // header byte 0x09: ErgoTree version 1 with the size flag — the shape ergo-lib re-encodes
    assert!(o["ergoTree"].as_str().unwrap().starts_with("09"));
    assert_id_matches(&o);
}

#[test]
fn every_fixture_output_hashes_to_its_node_box_id() {
    let mut n = 0usize;
    for h in CANONICAL.iter().copied().chain([NON_CANONICAL]) {
        let v = fixture_json(h);
        let outs = outputs(&v);
        assert!(!outs.is_empty());
        for o in &outs {
            assert_id_matches(o);
            n += 1;
        }
    }
    // guards against the loop silently checking nothing
    assert!(n >= 50, "only {n} outputs checked");
    println!("checked {n} block outputs");
}

#[test]
fn genesis_boxes_hash_to_their_node_box_ids() {
    let raw = std::fs::read_to_string(format!(
        "{}/../../tests/fixtures/genesis.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 3);
    for o in arr {
        assert_id_matches(o);
    }
    let decoded = decode_genesis_boxes(&raw).unwrap();
    let got: Vec<String> = decoded.iter().map(|b| hex::encode(b.id.0)).collect();
    let want: Vec<String> = arr
        .iter()
        .map(|b| b["boxId"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(got, want);
}

#[test]
fn block_with_non_canonical_tree_decodes_and_keeps_node_box_ids() {
    let v = fixture_json(NON_CANONICAL);
    let b = decode_block(&fixture(NON_CANONICAL)).unwrap();
    assert_eq!(b.header.height, NON_CANONICAL);
    let want: Vec<String> = outputs(&v)
        .iter()
        .map(|o| o["boxId"].as_str().unwrap().to_owned())
        .collect();
    let got: Vec<String> = b
        .txs
        .iter()
        .flat_map(|t| t.outputs.iter().map(|o| hex::encode(o.id.0)))
        .collect();
    assert_eq!(got, want);
    // the box that used to halt the sync
    const BAD: &str = "e75700ef081f64e6c5df7084840078b31d13eeb427fb3e30cef629d7cbb31394";
    let bad = b
        .txs
        .iter()
        .flat_map(|t| &t.outputs)
        .find(|o| hex::encode(o.id.0) == BAD)
        .expect("the non-canonical box is present in this block");
    assert_eq!(bad.value, 1_003_000_000);
    assert_eq!(bad.creation_height, 1702685);
    let bad_json = outputs(&v).into_iter().find(|o| o["boxId"] == BAD).unwrap();
    assert_eq!(
        bad.size as usize,
        box_bytes_from_json(&bad_json).unwrap().len()
    );
    assert_eq!(bad.tokens, vec![]);
    let regs: serde_json::Value = serde_json::from_str(&bad.registers_json).unwrap();
    assert_eq!(regs, bad_json["additionalRegisters"]);
}

/// The store format must not change: for canonical boxes our sizes, ids, tokens and
/// registers have to equal exactly what the previous ergo-lib-based decoder produced.
#[test]
fn canonical_fixtures_match_ergo_lib_derived_values() {
    use ergo_lib::chain::block::FullBlock;
    use ergo_lib::ergo_chain_types::Digest32;
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

    let mut n = 0usize;
    for h in CANONICAL {
        let fb: FullBlock = serde_json::from_str(&fixture(h)).unwrap();
        let ours = decode_block(&fixture(h)).unwrap();
        let theirs = &fb.block_transactions.transactions;
        assert_eq!(ours.txs.len(), theirs.len());
        for (tx, want) in ours.txs.iter().zip(theirs.iter()) {
            assert_eq!(tx.id.0, want.id().0 .0);
            assert_eq!(
                tx.size as usize,
                want.sigma_serialize_bytes().unwrap().len()
            );
            assert_eq!(tx.outputs.len(), want.outputs.len());
            for (got, w) in tx.outputs.iter().zip(want.outputs.iter()) {
                assert_eq!(got.id.0, Digest32::from(w.box_id()).0);
                assert_eq!(got.value, *w.value.as_u64());
                assert_eq!(got.creation_height, w.creation_height);
                assert_eq!(got.tree_bytes, w.ergo_tree.sigma_serialize_bytes().unwrap());
                assert_eq!(
                    got.size as usize,
                    w.sigma_serialize_bytes().unwrap().len(),
                    "box size changed for {}",
                    hex::encode(got.id.0)
                );
                let want_tokens: Vec<([u8; 32], u64)> = w
                    .tokens
                    .as_ref()
                    .map(|ts| {
                        ts.iter()
                            .map(|t| (Digest32::from(t.token_id).0, *t.amount.as_u64()))
                            .collect()
                    })
                    .unwrap_or_default();
                assert_eq!(got.tokens, want_tokens);
                let want_regs: serde_json::Value =
                    serde_json::to_value(&w.additional_registers).unwrap();
                let got_regs: serde_json::Value =
                    serde_json::from_str(&got.registers_json).unwrap();
                assert_eq!(got_regs, want_regs);
                n += 1;
            }
        }
    }
    println!("compared {n} boxes against ergo-lib");
}
