//! Optional node evidence, bound to the indexed block before and after I/O.
//! The node is trusted to report accepted proofs; this is not consensus replay.
use crate::budget::Budget;
use crate::dto::{parse_id, TxDto};
use crate::{blocking, ApiError, AppState};
use axum::extract::{Path, State};
use axum::{Extension, Json};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use xp_source::BlockSource;
use xp_types::hex32;

#[derive(Debug, Serialize)]
pub struct InputEvidence {
    id: String,
    proof: &'static str,
    extension_127: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Evidence {
    tx_id: String,
    block_id: String,
    height: u32,
    assurance: &'static str,
    inputs: Vec<InputEvidence>,
}

#[derive(Deserialize)]
struct Block {
    header: Header,
    #[serde(rename = "blockTransactions")]
    transactions: Transactions,
}
#[derive(Deserialize)]
struct Header {
    id: String,
    height: u32,
}
#[derive(Deserialize)]
struct Transactions {
    transactions: Vec<Transaction>,
}
#[derive(Deserialize)]
struct Transaction {
    id: String,
    inputs: Vec<Input>,
    outputs: Vec<Output>,
}
#[derive(Deserialize)]
struct Output {
    #[serde(rename = "boxId")]
    id: String,
}
#[derive(Deserialize)]
struct Input {
    #[serde(rename = "boxId")]
    id: String,
    #[serde(rename = "spendingProof")]
    spending: Proof,
}
#[derive(Deserialize)]
struct Proof {
    #[serde(rename = "proofBytes")]
    bytes: String,
    extension: HashMap<String, String>,
}

fn extract(raw: &str, block_id: &str, tx: &TxDto) -> Result<Evidence, ApiError> {
    let block: Block = serde_json::from_str(raw)
        .map_err(|_| ApiError::Integrity("invalid node proof evidence".into()))?;
    if block.header.id != block_id || block.header.height != tx.height {
        return Err(ApiError::Integrity("node evidence block mismatch".into()));
    }
    let found = block
        .transactions
        .transactions
        .get(usize::from(tx.index))
        .ok_or_else(|| ApiError::Integrity("missing node transaction".into()))?;
    if found.id != tx.id
        || !found
            .inputs
            .iter()
            .map(|i| &i.id)
            .eq(tx.inputs.iter().map(|i| &i.id))
        || !found
            .outputs
            .iter()
            .map(|o| &o.id)
            .eq(tx.outputs.iter().map(|o| &o.id))
    {
        return Err(ApiError::Integrity(
            "node evidence transaction mismatch".into(),
        ));
    }
    let inputs = found
        .inputs
        .iter()
        .map(|input| {
            if input.spending.bytes.len() % 2 != 0
                || !input.spending.bytes.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err(ApiError::Integrity("invalid node spending proof".into()));
            }
            let selector = input
                .spending
                .extension
                .get("127")
                .filter(|s| {
                    s.len() <= 16 && s.len() % 2 == 0 && s.bytes().all(|b| b.is_ascii_hexdigit())
                })
                .cloned();
            Ok(InputEvidence {
                id: input.id.clone(),
                proof: if input.spending.bytes.is_empty() {
                    "empty"
                } else {
                    "nonempty"
                },
                extension_127: selector,
            })
        })
        .collect::<Result<_, ApiError>>()?;
    Ok(Evidence {
        tx_id: tx.id.clone(),
        block_id: block_id.into(),
        height: tx.height,
        assurance: "trusted_node_response",
        inputs,
    })
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(raw_id): Path<String>,
    source: Option<Extension<Arc<dyn BlockSource>>>,
) -> Result<Json<Evidence>, ApiError> {
    let id = parse_id(&raw_id)?;
    let (tx, anchor) = blocking(&state, move |rd| {
        let mut budget = Budget::new();
        let row = rd
            .tx_by_id_admitted(&id, |n| budget.admit_tx_row(n))?
            .ok_or(ApiError::NotFound)?;
        let header = rd
            .header_at(row.height)?
            .ok_or_else(|| ApiError::Integrity("missing transaction header".into()))?;
        let tx = budget.tx(
            rd,
            &id,
            &row,
            rd.indexed_height()?,
            rd.emission_tree_hash()?.as_ref(),
        )?;
        Ok((tx, header.id))
    })
    .await?;
    let source = source.ok_or(ApiError::Overloaded)?.0;
    let permit = state
        .counters
        .evidence_permits
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::Overloaded)?;
    let height = tx.height;
    let raw = tokio::time::timeout(Duration::from_secs(5), async {
        if source.header_id_at(height).await.ok().flatten() != Some(anchor) {
            return Err(ApiError::Overloaded);
        }
        let raw = source
            .full_block_json(&anchor)
            .await
            .map_err(|_| ApiError::Overloaded)?
            .ok_or(ApiError::Overloaded)?;
        if raw.len() > 16 * 1024 * 1024 {
            return Err(ApiError::Overloaded);
        }
        if source.header_id_at(height).await.ok().flatten() != Some(anchor) {
            return Err(ApiError::Overloaded);
        }
        Ok(raw)
    })
    .await
    .map_err(|_| ApiError::Overloaded)??;
    let evidence = blocking(&state, move |rd| {
        // Occupy the evidence slot until parsing finishes, including cancellation.
        let _permit = permit;
        if rd.header_at(height)?.map(|h| h.id) != Some(anchor) {
            return Err(ApiError::Overloaded);
        }
        extract(&raw, &hex32(&anchor), &tx)
    })
    .await?;
    Ok(Json(evidence))
}

#[cfg(test)]
mod tests {
    use super::*;
    const RAW: &str = include_str!("../../../../tests/fixtures/receipts/storage-rent-block.json");

    fn fixture_tx() -> (tempfile::TempDir, TxDto, String) {
        let dir = tempfile::tempdir().unwrap();
        let store = xp_store::Store::open(&dir.path().join("receipt.redb")).unwrap();
        let block = xp_wire::decode_block(RAW).unwrap();
        let anchor = hex32(&block.header.id.0);
        store
            .seed_for_tests(block.header.height - 1, block.header.parent_id.0)
            .unwrap();
        let id = block.txs[1].id.0;
        store.apply_batch(&[block], true).unwrap();
        let rd = xp_store::Reader::new(&store).unwrap();
        let row = rd.tx_by_id(&id).unwrap().unwrap();
        let tx = Budget::new()
            .tx(&rd, &id, &row, rd.indexed_height().unwrap(), None)
            .unwrap();
        (dir, tx, anchor)
    }

    #[test]
    fn real_rent_proofs_and_signed_funding_input() {
        let (_dir, tx, anchor) = fixture_tx();
        let evidence = extract(RAW, &anchor, &tx).unwrap();
        assert_eq!(evidence.inputs[0].proof, "empty");
        assert_eq!(evidence.inputs[0].extension_127.as_deref(), Some("0300"));
        assert_eq!(evidence.inputs[1].proof, "nonempty");
        assert_eq!(evidence.inputs[1].extension_127, None);
    }

    #[test]
    fn mismatches_and_absent_proofs_never_become_evidence() {
        let (_dir, mut tx, anchor) = fixture_tx();
        assert!(extract(RAW, "wrong-block", &tx).is_err());
        let mut altered: serde_json::Value = serde_json::from_str(RAW).unwrap();
        altered["blockTransactions"]["transactions"][1]["inputs"][0]["spendingProof"]
            .as_object_mut()
            .unwrap()
            .remove("proofBytes");
        assert!(extract(&altered.to_string(), &anchor, &tx).is_err());
        tx.outputs[0].id = "wrong-output".into();
        assert!(extract(RAW, &anchor, &tx).is_err());
    }
}
