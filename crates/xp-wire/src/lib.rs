pub mod boxser;
pub mod tree;
pub use tree::{template_hash_of, tree_hash, tree_info, TreeInfo, TreeKind};

use boxser::{
    box_bytes, hash32_field, registers_of, tokens_of, u64_field, BoxParts, REGISTER_NAMES,
};
use tree::blake2b256;
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

fn hex_field(v: &serde_json::Value, field: &'static str) -> Result<Vec<u8>, WireError> {
    let s = v
        .get(field)
        .and_then(|x| x.as_str())
        .ok_or(WireError::MissingField(field))?;
    hex::decode(s).map_err(|_| WireError::MissingField(field))
}

fn array_field<'a>(
    v: &'a serde_json::Value,
    field: &'static str,
) -> Result<&'a Vec<serde_json::Value>, WireError> {
    v.get(field)
        .and_then(|x| x.as_array())
        .ok_or(WireError::MissingField(field))
}

/// Ids of the boxes referenced by an `inputs`/`dataInputs` array (absent array = empty).
fn referenced_box_ids(
    tx: &serde_json::Value,
    field: &'static str,
    required: bool,
) -> Result<Vec<BoxId>, WireError> {
    let items = match tx.get(field) {
        Some(v) => v.as_array().ok_or(WireError::MissingField(field))?,
        None if !required => return Ok(Vec::new()),
        None => return Err(WireError::MissingField(field)),
    };
    items
        .iter()
        .map(|i| Ok(BoxId(hash32_field(i, "boxId")?)))
        .collect()
}

/// Registers as `{"R4": "<hex>", ...}` — the node's hex verbatim, R4..R9 in order.
fn registers_json_of(v: &serde_json::Value, count: usize) -> String {
    let mut map = serde_json::Map::new();
    if let Some(regs) = v.get("additionalRegisters").and_then(|r| r.as_object()) {
        for name in REGISTER_NAMES.iter().take(count) {
            if let Some(val) = regs.get(*name) {
                map.insert((*name).to_string(), val.clone());
            }
        }
    }
    serde_json::Value::Object(map).to_string()
}

/// Decodes one box from node JSON. `tx_id`/`index` override the JSON fields when the box is
/// read as a transaction output (they are the enclosing transaction's, and authoritative).
///
/// The node's `boxId` is kept even if our serialisation disagrees: the chain accepted those
/// bytes, and halting the whole sync over one box we cannot re-serialise is worse than a
/// possibly-off size for it.
fn decode_box(
    v: &serde_json::Value,
    tx_id: Option<TxId>,
    index: Option<u16>,
) -> Result<DecodedBox, WireError> {
    let id = BoxId(hash32_field(v, "boxId")?);
    let tx_id = match tx_id {
        Some(t) => t,
        None => TxId(hash32_field(v, "transactionId")?),
    };
    let index = match index {
        Some(i) => i,
        None => u64_field(v, "index")? as u16,
    };
    let tree_bytes = hex_field(v, "ergoTree")?;
    let tokens = tokens_of(v)?;
    let registers = registers_of(v)?;
    let value = u64_field(v, "value")?;
    let creation_height = u64_field(v, "creationHeight")? as u32;
    let bytes = box_bytes(&BoxParts {
        value,
        tree_bytes: &tree_bytes,
        creation_height,
        tokens: &tokens,
        registers: &registers,
        tx_id: &tx_id.0,
        index,
    });
    let computed = blake2b256(&bytes);
    if computed != id.0 {
        tracing::warn!(
            node_box_id = %hex::encode(id.0),
            computed_box_id = %hex::encode(computed),
            "box id from our serialisation differs from the node's; keeping the node's id"
        );
    }
    Ok(DecodedBox {
        id,
        value,
        tree_hash: tree_hash(&tree_bytes),
        tree_bytes,
        creation_height,
        tx_id,
        index,
        registers_json: registers_json_of(v, registers.len()),
        tokens,
        size: bytes.len() as u32,
    })
}

fn decode_header(header_json: &serde_json::Value) -> Result<DecodedHeader, WireError> {
    let difficulty: u128 = header_json
        .get("difficulty")
        .and_then(|d| d.as_str())
        .and_then(|s| s.parse().ok())
        .ok_or(WireError::MissingField("header.difficulty"))?;
    let miner_pk: [u8; 33] = header_json
        .get("powSolutions")
        .ok_or(WireError::MissingField("header.powSolutions"))
        .and_then(|s| hex_field(s, "pk"))?
        .try_into()
        .map_err(|_| WireError::MissingField("header.powSolutions.pk"))?;
    let votes: [u8; 3] = hex_field(header_json, "votes")?
        .try_into()
        .map_err(|_| WireError::MissingField("header.votes"))?;
    let version = u64_field(header_json, "version")? as u8;
    Ok(DecodedHeader {
        id: HeaderId(hash32_field(header_json, "id")?),
        parent_id: HeaderId(hash32_field(header_json, "parentId")?),
        height: u64_field(header_json, "height")? as u32,
        timestamp: u64_field(header_json, "timestamp")?,
        difficulty,
        miner_pk,
        votes,
        version,
        raw_json: header_json.to_string(),
    })
}

/// Decodes a node `FullBlock` JSON without going through ergo-lib's block/box model: the
/// node's ids are taken verbatim, so a box the chain accepted but ergo-lib cannot
/// re-serialise canonically no longer makes the whole block undecodable.
pub fn decode_block(json: &str) -> Result<DecodedBlock, WireError> {
    let v: serde_json::Value = serde_json::from_str(json)?;
    let header_json = v.get("header").ok_or(WireError::MissingField("header"))?;
    let header = decode_header(header_json)?;
    let block_txs = v
        .get("blockTransactions")
        .ok_or(WireError::MissingField("blockTransactions"))?;
    let raw_txs = array_field(block_txs, "transactions")?;
    let mut txs = Vec::with_capacity(raw_txs.len());
    for tx in raw_txs {
        let tx_id = TxId(hash32_field(tx, "id")?);
        let raw_outputs = array_field(tx, "outputs")?;
        let mut outputs = Vec::with_capacity(raw_outputs.len());
        for (i, o) in raw_outputs.iter().enumerate() {
            outputs.push(decode_box(o, Some(tx_id), Some(i as u16))?);
        }
        txs.push(DecodedTx {
            id: tx_id,
            inputs: referenced_box_ids(tx, "inputs", true)?,
            data_inputs: referenced_box_ids(tx, "dataInputs", false)?,
            outputs,
            size: u64_field(tx, "size")? as u32,
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
    let v: serde_json::Value = serde_json::from_str(json)?;
    let raw = v
        .as_array()
        .ok_or(WireError::MissingField("genesis boxes"))?;
    raw.iter().map(|b| decode_box(b, None, None)).collect()
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
