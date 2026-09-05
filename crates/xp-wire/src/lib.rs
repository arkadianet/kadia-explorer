pub mod tree;
pub use tree::{template_hash_of, tree_hash, tree_info, TreeInfo, TreeKind};

use ergo_lib::chain::block::FullBlock;
use ergo_lib::ergo_chain_types::Digest32;
use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
use xp_types::{BoxId, Hash32, HeaderId, TreeHash, TxId};

#[derive(Debug, thiserror::Error)]
pub enum WireError {
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("tree: {0}")]
    Tree(String),
    #[error("serialize: {0}")]
    Ser(String),
    #[error("missing or invalid field: {0}")]
    MissingField(&'static str),
}

#[derive(Debug, Clone)]
pub struct DecodedHeader {
    pub id: HeaderId,
    pub parent_id: HeaderId,
    pub height: u32,
    pub timestamp: u64,
    pub difficulty: u128,
    pub miner_pk: [u8; 33],
    pub votes: [u8; 3],
    pub version: u8,
    pub raw_json: String,
}
#[derive(Debug, Clone)]
pub struct DecodedBox {
    pub id: BoxId,
    pub value: u64,
    pub tree_bytes: Vec<u8>,
    pub tree_hash: TreeHash,
    pub creation_height: u32,
    pub tx_id: TxId,
    pub index: u16,
    pub tokens: Vec<(Hash32, u64)>,
    pub registers_json: String,
    /// Consensus box size as used for storage rent (`ErgoBox.bytes`: candidate body with
    /// full token ids + tx id + index), NOT the compacted in-block footprint.
    pub size: u32,
}
#[derive(Debug, Clone)]
pub struct DecodedTx {
    pub id: TxId,
    pub inputs: Vec<BoxId>,
    pub data_inputs: Vec<BoxId>,
    pub outputs: Vec<DecodedBox>,
    pub size: u32,
}
#[derive(Debug, Clone)]
pub struct DecodedBlock {
    pub header: DecodedHeader,
    pub txs: Vec<DecodedTx>,
    pub size: u32,
}

fn box_id_to_hash32(id: ergo_lib::ergotree_ir::chain::ergo_box::BoxId) -> Hash32 {
    Digest32::from(id).0
}

fn token_id_to_hash32(id: ergo_lib::ergotree_ir::chain::token::TokenId) -> Hash32 {
    Digest32::from(id).0
}

pub fn decode_block(json: &str) -> Result<DecodedBlock, WireError> {
    let v: serde_json::Value = serde_json::from_str(json)?;
    let fb: FullBlock = serde_json::from_value(v.clone())?;
    let h = &fb.header;
    // `header` cannot be absent once `fb` has parsed successfully above, but we still need
    // its raw JSON to read `difficulty` (not modeled on `ergo_chain_types::Header`), so fetch
    // it with a real error instead of silently defaulting to an empty object.
    let header_json = v
        .get("header")
        .cloned()
        .ok_or(WireError::MissingField("header"))?;
    let difficulty: u128 = header_json
        .get("difficulty")
        .and_then(|d| d.as_str())
        .and_then(|s| s.parse().ok())
        .ok_or(WireError::MissingField("header.difficulty"))?;
    let miner_pk_hex = h.autolykos_solution.miner_pk.to_string();
    let miner_pk_bytes = hex::decode(&miner_pk_hex).map_err(|e| WireError::Ser(e.to_string()))?;
    let mut miner_pk = [0u8; 33];
    if miner_pk_bytes.len() != miner_pk.len() {
        return Err(WireError::Ser(format!(
            "unexpected miner_pk length: {}",
            miner_pk_bytes.len()
        )));
    }
    miner_pk.copy_from_slice(&miner_pk_bytes);
    let header = DecodedHeader {
        id: HeaderId(h.id.0 .0),
        parent_id: HeaderId(h.parent_id.0 .0),
        height: h.height,
        timestamp: h.timestamp,
        difficulty,
        miner_pk,
        votes: h.votes.0,
        version: h.version,
        raw_json: header_json.to_string(),
    };
    let mut txs = Vec::with_capacity(fb.block_transactions.transactions.len());
    for tx in fb.block_transactions.transactions.iter() {
        let tx_id = TxId(tx.id().0 .0);
        let mut outputs = Vec::with_capacity(tx.outputs.len());
        for (i, o) in tx.outputs.iter().enumerate() {
            // Consensus box size (storage rent basis): `ErgoBox.bytes` = candidate body
            // serialized with full 32-byte token ids + tx id + index, per sigmastate's
            // `ErgoBox.sigmaSerializer`. This is exactly `ErgoBox::sigma_serialize_bytes`,
            // not the compacted in-block footprint.
            let bytes = o
                .sigma_serialize_bytes()
                .map_err(|e| WireError::Ser(e.to_string()))?;
            let tree_bytes = o
                .ergo_tree
                .sigma_serialize_bytes()
                .map_err(|e| WireError::Ser(e.to_string()))?;
            let tokens = o
                .tokens
                .as_ref()
                .map(|ts| {
                    ts.iter()
                        .map(|t| (token_id_to_hash32(t.token_id), *t.amount.as_u64()))
                        .collect()
                })
                .unwrap_or_default();
            outputs.push(DecodedBox {
                id: BoxId(box_id_to_hash32(o.box_id())),
                value: *o.value.as_u64(),
                tree_hash: tree_hash(&tree_bytes),
                tree_bytes,
                creation_height: o.creation_height,
                tx_id,
                index: i as u16,
                tokens,
                registers_json: serde_json::to_string(&o.additional_registers)?,
                size: bytes.len() as u32,
            });
        }
        let size = tx
            .sigma_serialize_bytes()
            .map_err(|e| WireError::Ser(e.to_string()))?
            .len() as u32;
        txs.push(DecodedTx {
            id: tx_id,
            inputs: tx
                .inputs
                .iter()
                .map(|i| BoxId(box_id_to_hash32(i.box_id)))
                .collect(),
            data_inputs: tx
                .data_inputs
                .as_ref()
                .map(|d| {
                    d.iter()
                        .map(|i| BoxId(box_id_to_hash32(i.box_id)))
                        .collect()
                })
                .unwrap_or_default(),
            outputs,
            size,
        });
    }
    let size = v
        .get("size")
        .and_then(|s| s.as_u64())
        .ok_or(WireError::MissingField("size"))? as u32;
    Ok(DecodedBlock { header, txs, size })
}

/// Decodes the chain-spec genesis boxes as served by a node's `GET /utxo/genesis`: a JSON
/// array of `ErgoBox` objects. These boxes are created by the chain spec rather than by any
/// block, so they never appear in block data and must be seeded into a store separately —
/// otherwise the first block that spends one looks like a missing input.
///
/// The resulting [`DecodedBox`]es follow exactly the same rules `decode_block` applies to a
/// transaction's outputs, except that `tx_id` and `index` come from the JSON (all-zero tx id,
/// index 0 for each) instead of from an enclosing transaction.
pub fn decode_genesis_boxes(json: &str) -> Result<Vec<DecodedBox>, WireError> {
    let raw: Vec<ErgoBox> = serde_json::from_str(json)?;
    let mut out = Vec::with_capacity(raw.len());
    for b in &raw {
        let bytes = b
            .sigma_serialize_bytes()
            .map_err(|e| WireError::Ser(e.to_string()))?;
        let tree_bytes = b
            .ergo_tree
            .sigma_serialize_bytes()
            .map_err(|e| WireError::Ser(e.to_string()))?;
        let tokens = b
            .tokens
            .as_ref()
            .map(|ts| {
                ts.iter()
                    .map(|t| (token_id_to_hash32(t.token_id), *t.amount.as_u64()))
                    .collect()
            })
            .unwrap_or_default();
        out.push(DecodedBox {
            id: BoxId(box_id_to_hash32(b.box_id())),
            value: *b.value.as_u64(),
            tree_hash: tree_hash(&tree_bytes),
            tree_bytes,
            creation_height: b.creation_height,
            tx_id: TxId(b.transaction_id.0 .0),
            index: b.index,
            tokens,
            registers_json: serde_json::to_string(&b.additional_registers)?,
            size: bytes.len() as u32,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_value() -> serde_json::Value {
        let raw = std::fs::read_to_string(format!(
            "{}/../../tests/fixtures/blocks/1866000.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        serde_json::from_str(&raw).unwrap()
    }

    #[test]
    fn missing_top_level_size_is_an_error() {
        let mut v = fixture_value();
        v.as_object_mut().unwrap().remove("size");
        let err = decode_block(&v.to_string()).unwrap_err();
        assert!(matches!(err, WireError::MissingField("size")));
    }

    #[test]
    fn non_numeric_difficulty_is_an_error() {
        let mut v = fixture_value();
        v["header"]["difficulty"] = serde_json::Value::String("not-a-number".to_string());
        let err = decode_block(&v.to_string()).unwrap_err();
        assert!(matches!(err, WireError::MissingField("header.difficulty")));
    }
}
