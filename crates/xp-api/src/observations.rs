//! Bounded, process-local observations of requested transaction IDs, never a network archive.
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex as AsyncMutex, Semaphore};
use xp_source::{BlockSource, SourceError};
use xp_types::{hex32, Hash32};

pub(crate) const RETENTION_SECONDS: u64 = 3600;
const MAX_ENTRIES: usize = 1024;
pub(crate) const MAX_INPUTS: usize = 512;
const MAX_BODY: usize = 2 * 1024 * 1024;
const CACHE_TIME: Duration = Duration::from_secs(5);
const MAX_DETAIL_IDS: usize = 32;
const MAX_DETAIL_OUTPUTS: usize = 32;
const MAX_DETAIL_TOKENS: usize = 32;
const MAX_DETAIL_SCRIPT_BYTES: usize = 2048;
// Address recreation parses Sigma and Base58-encodes P2S payloads: use a tighter cap.
const MAX_ADDRESS_SCRIPT_BYTES: usize = 512;
const MAX_DETAIL_BYTES: usize = 32 * 1024;
const MAX_PENDING_WORK: usize = 10_000;

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(crate) struct Inclusion {
    pub block_id: String,
    pub height: u32,
    pub confirmations: u32,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct MempoolObservation {
    pub observation: &'static str,
    pub checked_at_ms: Option<u64>,
    pub first_seen_at_ms: Option<u64>,
    pub last_seen_at_ms: Option<u64>,
    pub error: Option<&'static str>,
}
impl Default for MempoolObservation {
    fn default() -> Self {
        Self {
            observation: "not_checked",
            checked_at_ms: None,
            first_seen_at_ms: None,
            last_seen_at_ms: None,
            error: None,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct PendingSummary {
    pub input_count: usize,
    pub output_count: usize,
    pub data_input_count: usize,
    pub size: Option<u32>,
    pub fee: Option<String>,
    pub details: PendingDetails,
}

/// A bounded node projection, not resolved input balances or a confirmed receipt.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct PendingDetails {
    pub inputs: Vec<String>,
    pub data_inputs: Vec<String>,
    pub outputs: Vec<PendingOutput>,
    pub inputs_truncated: bool,
    pub data_inputs_truncated: bool,
    pub outputs_truncated: bool,
    /// Completeness of this projection only; signatures/registers are not included.
    pub complete: bool,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct PendingToken {
    pub id: String,
    pub amount: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct PendingOutput {
    pub index: usize,
    pub id: Option<String>,
    pub value: String,
    pub ergo_tree: Option<String>,
    /// Derived with the ingestion pipeline's Mainnet encoder. None is unavailable,
    /// not an empty address or a claim about ownership of this output.
    pub address: Option<String>,
    pub tokens: Vec<PendingToken>,
    /// Missing node assets are unknown, never an invented empty balance.
    pub token_count: Option<usize>,
    pub tokens_truncated: bool,
    pub ergo_tree_truncated: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct PendingRecord {
    pub inputs: Vec<Hash32>,
    pub summary: PendingSummary,
}

#[derive(Debug, Default)]
pub(crate) struct Entry {
    pub mempool: MempoolObservation,
    pub pending: Option<PendingSummary>,
    pub inputs: Vec<Hash32>,
    pub included: Option<Inclusion>,
    next_check: Option<Instant>,
}
impl Entry {
    pub fn record_inclusion(&mut self, inclusion: Inclusion, inputs: Vec<Hash32>) {
        self.included = Some(inclusion);
        self.inputs = inputs;
        // A later disappearance needs a new node observation, not a cached pre-inclusion one.
        self.next_check = None;
    }
}

#[derive(Debug)]
struct CachedEntry {
    entry: Arc<AsyncMutex<Entry>>,
    accessed: Instant,
}

#[derive(Debug)]
pub(crate) struct Observations {
    entries: Mutex<HashMap<Hash32, CachedEntry>>,
    permits: Arc<Semaphore>,
}
impl Default for Observations {
    fn default() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            permits: Arc::new(Semaphore::new(2)),
        }
    }
}
impl Observations {
    pub fn entry(&self, id: Hash32) -> Option<Arc<AsyncMutex<Entry>>> {
        self.entry_at(id, Instant::now())
    }

    fn entry_at(&self, id: Hash32, now: Instant) -> Option<Arc<AsyncMutex<Entry>>> {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.retain(|_, value| {
            Arc::strong_count(&value.entry) > 1
                || now.duration_since(value.accessed).as_secs() < RETENTION_SECONDS
        });
        if let Some(value) = entries.get_mut(&id) {
            value.accessed = now;
            return Some(value.entry.clone());
        }
        if entries.len() == MAX_ENTRIES {
            let oldest = entries
                .iter()
                .filter(|(_, v)| Arc::strong_count(&v.entry) == 1)
                .min_by_key(|(_, v)| v.accessed)
                .map(|(k, _)| *k)?;
            entries.remove(&oldest);
        }
        let entry = Arc::new(AsyncMutex::new(Entry::default()));
        entries.insert(
            id,
            CachedEntry {
                entry: entry.clone(),
                accessed: now,
            },
        );
        Some(entry)
    }

    /// The caller holds one entry lock: concurrent requests for the same ID coalesce.
    /// Cancellation drops the lock/permit and leaves the previous observation intact.
    pub async fn observe(
        &self,
        id: Hash32,
        entry: &mut Entry,
        source: Option<&Arc<dyn BlockSource>>,
    ) {
        if entry.next_check.is_some_and(|at| at > Instant::now()) {
            return;
        }
        let result = match source {
            None => Err("unsupported"),
            Some(source) => match self.permits.clone().try_acquire_owned() {
                Err(_) => Err("busy"),
                Ok(permit) => {
                    let fetched = tokio::time::timeout(
                        Duration::from_secs(2),
                        source.unconfirmed_transaction_json(&id),
                    )
                    .await;
                    match fetched {
                        Ok(Ok(Some(raw))) if raw.len() <= MAX_BODY => {
                            // Keep admission held if request cancellation outlives the parser.
                            tokio::task::spawn_blocking(move || {
                                let _permit = permit;
                                parse_pending(&raw, &id)
                                    .map(Some)
                                    .map_err(|_| "unavailable")
                            })
                            .await
                            .unwrap_or(Err("unavailable"))
                        }
                        Ok(Ok(None)) => Ok(None),
                        Ok(Err(SourceError::Capability(_))) => Err("unsupported"),
                        _ => Err("unavailable"),
                    }
                }
            },
        };
        let at = now_ms();
        entry.mempool.checked_at_ms = Some(at);
        entry.next_check = Some(
            Instant::now()
                + if result.is_ok() {
                    CACHE_TIME
                } else {
                    Duration::from_secs(2)
                },
        );
        match result {
            Ok(Some(pending)) => {
                entry.mempool.observation = "present";
                entry.mempool.first_seen_at_ms.get_or_insert(at);
                entry.mempool.last_seen_at_ms = Some(at);
                entry.mempool.error = None;
                entry.inputs = pending.inputs;
                entry.pending = Some(pending.summary);
            }
            Ok(None) => {
                entry.mempool.observation = "absent";
                entry.mempool.error = None;
            }
            Err(error) => {
                entry.mempool.observation = "unavailable";
                entry.mempool.error = Some(error);
            }
        }
    }
}

#[derive(Deserialize)]
struct NodeInput {
    #[serde(rename = "boxId")]
    id: String,
}
#[derive(Deserialize)]
struct NodeOutput {
    #[serde(rename = "boxId")]
    id: Option<String>,
    #[serde(rename = "transactionId")]
    transaction_id: Option<String>,
    index: Option<usize>,
    value: u64,
    #[serde(rename = "ergoTree")]
    tree: String,
    assets: Option<Vec<NodeToken>>,
}
#[derive(Deserialize)]
struct NodeToken {
    #[serde(rename = "tokenId")]
    id: String,
    amount: u64,
}
#[derive(Deserialize)]
struct NodeTransaction {
    id: String,
    inputs: Vec<NodeInput>,
    #[serde(rename = "dataInputs")]
    data_inputs: Vec<NodeInput>,
    outputs: Vec<NodeOutput>,
    size: Option<u32>,
}

fn parse_pending(raw: &str, requested: &Hash32) -> Result<PendingRecord, ()> {
    if raw.len() > MAX_BODY {
        return Err(());
    }
    let tx: NodeTransaction = serde_json::from_str(raw).map_err(|_| ())?;
    if tx.id != hex32(requested)
        || tx.inputs.is_empty()
        || tx.inputs.len() > MAX_INPUTS
        || tx.data_inputs.len() > MAX_INPUTS
        || tx.outputs.is_empty()
        || tx.outputs.len() > 4096
        || tx.size == Some(0)
    {
        return Err(());
    }
    let inputs: Vec<_> = tx
        .inputs
        .iter()
        .map(|i| xp_types::parse_hex32(&i.id).map_err(|_| ()))
        .collect::<Result<_, _>>()?;
    if inputs.iter().collect::<HashSet<_>>().len() != inputs.len() {
        return Err(());
    }
    let data_inputs: Vec<_> = tx
        .data_inputs
        .iter()
        .map(|input| xp_types::parse_hex32(&input.id).map_err(|_| ()))
        .collect::<Result<_, _>>()?;
    if data_inputs.iter().collect::<HashSet<_>>().len() != data_inputs.len() {
        return Err(());
    }
    let mut work = inputs.len() + data_inputs.len() + tx.outputs.len();
    let mut output_ids = HashSet::new();
    let mut fee = 0u128;
    for (index, output) in tx.outputs.iter().enumerate() {
        work = work
            .checked_add(
                output.assets.as_ref().map_or(0, Vec::len) + output.tree.len().div_ceil(128),
            )
            .ok_or(())?;
        if work > MAX_PENDING_WORK {
            return Err(());
        }
        if output.tree.is_empty()
            || output.tree.len() % 2 != 0
            || !output.tree.bytes().all(|b| b.is_ascii_hexdigit())
            || output.value == 0
            || output.value > i64::MAX as u64
            || output
                .transaction_id
                .as_ref()
                .is_some_and(|id| id != &tx.id)
            || output.index.is_some_and(|supplied| supplied != index)
        {
            return Err(());
        }
        if let Some(id) = &output.id {
            if !output_ids.insert(xp_types::parse_hex32(id).map_err(|_| ())?) {
                return Err(());
            }
        }
        if let Some(assets) = &output.assets {
            let mut ids = HashSet::new();
            for token in assets {
                if token.amount == 0
                    || token.amount > i64::MAX as u64
                    || !ids.insert(xp_types::parse_hex32(&token.id).map_err(|_| ())?)
                {
                    return Err(());
                }
            }
        }
        if output.tree.eq_ignore_ascii_case(xp_store::FEE_TREE_HEX) {
            fee += u128::from(output.value);
        }
    }
    let mut details = PendingDetails {
        inputs: inputs.iter().take(MAX_DETAIL_IDS).map(hex32).collect(),
        data_inputs: data_inputs.iter().take(MAX_DETAIL_IDS).map(hex32).collect(),
        outputs: Vec::new(),
        inputs_truncated: inputs.len() > MAX_DETAIL_IDS,
        data_inputs_truncated: data_inputs.len() > MAX_DETAIL_IDS,
        outputs_truncated: tx.outputs.len() > MAX_DETAIL_OUTPUTS,
        complete: false,
    };
    for (index, output) in tx.outputs.iter().take(MAX_DETAIL_OUTPUTS).enumerate() {
        let ergo_tree_truncated = output.tree.len() > MAX_DETAIL_SCRIPT_BYTES * 2;
        let address = (output.tree.len() <= MAX_ADDRESS_SCRIPT_BYTES * 2)
            .then(|| {
                let bytes = hex::decode(&output.tree).ok()?;
                let tree = xp_wire::tree_info(&bytes).ok()?;
                // A decoder may accept a valid prefix or normalize a script. Only link
                // an address when its canonical script is exactly this output's script.
                (xp_wire::tree::address_tree_hash(&tree.address).ok()?
                    == xp_wire::tree_hash(&bytes))
                .then_some(tree.address)
            })
            .flatten();
        let tokens: Vec<_> = output
            .assets
            .as_ref()
            .into_iter()
            .flatten()
            .take(MAX_DETAIL_TOKENS)
            .map(|token| {
                Ok::<_, ()>(PendingToken {
                    id: hex32(&xp_types::parse_hex32(&token.id).map_err(|_| ())?),
                    amount: token.amount.to_string(),
                })
            })
            .collect::<Result<_, _>>()?;
        details.outputs.push(PendingOutput {
            index,
            id: output
                .id
                .as_ref()
                .map(|id| {
                    xp_types::parse_hex32(id)
                        .map(|id| hex32(&id))
                        .map_err(|_| ())
                })
                .transpose()?,
            value: output.value.to_string(),
            ergo_tree: (!ergo_tree_truncated).then(|| output.tree.to_ascii_lowercase()),
            address,
            tokens,
            token_count: output.assets.as_ref().map(Vec::len),
            tokens_truncated: output
                .assets
                .as_ref()
                .is_some_and(|assets| assets.len() > MAX_DETAIL_TOKENS),
            ergo_tree_truncated,
        });
        // At most32 small serializations; bound retained/cache and status response size too.
        if serde_json::to_vec(&details).map_err(|_| ())?.len() > MAX_DETAIL_BYTES {
            details.outputs.pop();
            details.outputs_truncated = true;
            break;
        }
    }
    details.complete = !details.inputs_truncated
        && !details.data_inputs_truncated
        && !details.outputs_truncated
        && details.outputs.iter().all(|output| {
            !output.tokens_truncated && !output.ergo_tree_truncated && output.token_count.is_some()
        });
    Ok(PendingRecord {
        summary: PendingSummary {
            input_count: inputs.len(),
            output_count: tx.outputs.len(),
            data_input_count: tx.data_inputs.len(),
            size: tx.size,
            fee: Some(fee.to_string()),
            details,
        },
        inputs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn detail_fixture() -> Value {
        json!({"id":hex32(&[1;32]),"inputs":[{"boxId":hex32(&[2;32])}],
            "dataInputs":[{"boxId":hex32(&[3;32])}],"size":100,
            "outputs":[{"boxId":hex32(&[4;32]),"transactionId":hex32(&[1;32]),"index":0,
                "value":9007199254740993u64,"ergoTree":"0008d3",
                "assets":[{"tokenId":hex32(&[5;32]),"amount":9007199254740993u64}]}]})
    }
    fn numbered(n: usize) -> String {
        let mut id = [0xab; 32];
        id[..8].copy_from_slice(&(n as u64).to_be_bytes());
        hex32(&id)
    }

    #[test]
    fn pending_details_keep_exact_evidence_without_resolving_input_balances() {
        let tx = detail_fixture();
        let parsed = parse_pending(&tx.to_string(), &[1; 32]).unwrap();
        let detail = serde_json::to_value(parsed.summary.details).unwrap();
        assert_eq!(detail["inputs"], json!([hex32(&[2; 32])]));
        assert_eq!(detail["data_inputs"], json!([hex32(&[3; 32])]));
        assert_eq!(detail["outputs"][0]["value"], "9007199254740993");
        assert_eq!(
            detail["outputs"][0]["tokens"][0]["amount"],
            "9007199254740993"
        );
        assert_eq!(detail["outputs"][0]["id"], hex32(&[4; 32]));
        assert_eq!(detail["outputs"][0]["index"], 0);
        assert_eq!(detail["complete"], true);
        assert!(detail["outputs"][0]["address"].is_string());
        let mut minimal = tx;
        let output = minimal["outputs"][0].as_object_mut().unwrap();
        output.remove("boxId");
        output.remove("assets");
        let parsed = parse_pending(&minimal.to_string(), &[1; 32]).unwrap();
        assert_eq!(parsed.summary.output_count, 1);
        assert!(parsed.summary.details.outputs[0].id.is_none());
        assert!(parsed.summary.details.outputs[0].token_count.is_none());
        assert!(parsed.summary.details.outputs[0].tokens.is_empty());
        assert!(!parsed.summary.details.complete);
    }

    #[test]
    fn malformed_detail_fields_fail_instead_of_becoming_partial_zeroes() {
        for path in ["value", "amount"] {
            for invalid in [
                json!(0),
                json!(-1),
                json!(1.5),
                json!(1.0),
                json!("9007199254740993"),
                json!(u64::MAX),
            ] {
                let mut tx = detail_fixture();
                if path == "value" {
                    tx["outputs"][0]["value"] = invalid;
                } else {
                    tx["outputs"][0]["assets"][0]["amount"] = invalid;
                }
                assert!(parse_pending(&tx.to_string(), &[1; 32]).is_err(), "{tx}");
            }
        }
        for (key, value) in [
            ("boxId", json!("bad")),
            ("transactionId", json!(hex32(&[9; 32]))),
            ("index", json!(2)),
            ("ergoTree", json!("xyz")),
            ("assets", json!({})),
        ] {
            let mut tx = detail_fixture();
            tx["outputs"][0][key] = value;
            assert!(parse_pending(&tx.to_string(), &[1; 32]).is_err());
        }
        for field in ["inputs", "dataInputs", "outputs"] {
            let mut tx = detail_fixture();
            let mut duplicate = tx[field][0].clone();
            if field == "outputs" {
                duplicate["index"] = json!(1);
            }
            tx[field].as_array_mut().unwrap().push(duplicate);
            assert!(
                parse_pending(&tx.to_string(), &[1; 32]).is_err(),
                "duplicate {field}"
            );
        }
        let mut tx = detail_fixture();
        let mut duplicate = tx["outputs"][0]["assets"][0].clone();
        duplicate["tokenId"] = json!(duplicate["tokenId"].as_str().unwrap().to_uppercase());
        tx["outputs"][0]["assets"]
            .as_array_mut()
            .unwrap()
            .push(duplicate);
        assert!(parse_pending(&tx.to_string(), &[1; 32]).is_err());
    }

    #[test]
    fn projection_caps_do_not_shorten_conflict_input_memory_or_fabricate_missing_fields() {
        let mut tx = detail_fixture();
        tx["inputs"] = json!((0..33)
            .map(|n| json!({"boxId":numbered(n)}))
            .collect::<Vec<_>>());
        tx["dataInputs"] = tx["inputs"].clone();
        let mut outputs = Vec::new();
        for n in 0..33 {
            outputs.push(json!({"value":1,"ergoTree":"00","assets":[],"index":n}));
        }
        tx["outputs"] = json!(outputs);
        tx["outputs"][0]["assets"] = json!((0..33)
            .map(|n| json!({"tokenId":numbered(n),"amount":1}))
            .collect::<Vec<_>>());
        tx["outputs"][0]["ergoTree"] = json!("00".repeat(MAX_DETAIL_SCRIPT_BYTES + 1));
        let parsed = parse_pending(&tx.to_string(), &[1; 32]).unwrap();
        assert_eq!(parsed.inputs.len(), 33);
        let detail = parsed.summary.details;
        assert_eq!(detail.inputs.len(), 32);
        assert!(detail.inputs_truncated);
        assert_eq!(detail.data_inputs.len(), 32);
        assert!(detail.data_inputs_truncated);
        assert_eq!(detail.outputs.len(), 32);
        assert!(detail.outputs_truncated);
        assert_eq!(detail.outputs[0].tokens.len(), 32);
        assert_eq!(detail.outputs[0].token_count, Some(33));
        assert!(detail.outputs[0].tokens_truncated);
        assert!(detail.outputs[0].ergo_tree.is_none());
        assert!(detail.outputs[0].ergo_tree_truncated);
        assert!(!detail.complete);
        // Malformed fields after a projection cap are still not accepted as node evidence.
        tx["outputs"][32]["value"] = json!(-1);
        assert!(parse_pending(&tx.to_string(), &[1; 32]).is_err());
    }

    #[test]
    fn exact_projection_boundaries_and_cache_payload_are_bounded() {
        let mut tx = detail_fixture();
        tx["dataInputs"] = json!([]);
        tx["inputs"] = json!((0..32)
            .map(|n| json!({"boxId":numbered(n)}))
            .collect::<Vec<_>>());
        tx["outputs"] = json!((0..32)
            .map(|n| json!({"value":1,"ergoTree":"00","assets":[],"index":n}))
            .collect::<Vec<_>>());
        tx["outputs"][0]["assets"] = json!((0..32)
            .map(|n| json!({"tokenId":numbered(n),"amount":1}))
            .collect::<Vec<_>>());
        tx["outputs"][0]["ergoTree"] = json!("00".repeat(MAX_DETAIL_SCRIPT_BYTES));
        let parsed = parse_pending(&tx.to_string(), &[1; 32]).unwrap();
        assert!(parsed.summary.details.complete);
        assert_eq!(
            parsed.summary.details.outputs[0]
                .ergo_tree
                .as_ref()
                .unwrap()
                .len(),
            4096
        );
        for output in tx["outputs"].as_array_mut().unwrap() {
            output["ergoTree"] = json!("00".repeat(MAX_DETAIL_SCRIPT_BYTES));
        }
        let parsed = parse_pending(&tx.to_string(), &[1; 32]).unwrap();
        assert!(serde_json::to_vec(&parsed.summary.details).unwrap().len() <= MAX_DETAIL_BYTES);
        assert!(parsed.summary.details.outputs.len() < 32);
        assert!(parsed.summary.details.outputs_truncated);
        assert!(!parsed.summary.details.complete);
    }

    #[test]
    fn pending_validation_work_and_raw_body_have_explicit_bounds() {
        let mut tx = detail_fixture();
        tx["outputs"][0]["assets"] = json!((0..MAX_PENDING_WORK)
            .map(|n| json!({"tokenId":numbered(n),"amount":1}))
            .collect::<Vec<_>>());
        assert!(tx.to_string().len() < MAX_BODY);
        assert!(parse_pending(&tx.to_string(), &[1; 32]).is_err());
        tx = detail_fixture();
        tx["ignored"] = json!("x".repeat(MAX_BODY));
        assert!(parse_pending(&tx.to_string(), &[1; 32]).is_err());
    }

    #[test]
    fn fee_output_sum_does_not_overflow_or_round_at_u64_boundary() {
        let mut tx = detail_fixture();
        tx["outputs"] = json!((0..3)
            .map(|index| json!({"value":i64::MAX as u64,
            "ergoTree":xp_store::FEE_TREE_HEX,"index":index,"assets":[]}))
            .collect::<Vec<_>>());
        let parsed = parse_pending(&tx.to_string(), &[1; 32]).unwrap();
        assert_eq!(parsed.summary.fee.as_deref(), Some("27670116110564327421"));
        assert_eq!(
            parsed.summary.details.outputs[0].value,
            "9223372036854775807"
        );
    }

    #[test]
    fn pending_addresses_reuse_mainnet_p2pk_and_p2s_encoder_with_bounded_fallback() {
        for script in [
            "0008cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            "0008d3",
        ] {
            let bytes = hex::decode(script).unwrap();
            let mut tx = detail_fixture();
            tx["outputs"][0]["ergoTree"] = json!(script);
            let parsed = parse_pending(&tx.to_string(), &[1; 32]).unwrap();
            let output = &parsed.summary.details.outputs[0];
            let address = output.address.as_ref().expect("supported Mainnet script");
            assert_eq!(address, &xp_wire::tree_info(&bytes).unwrap().address);
            assert_eq!(
                xp_wire::tree::address_tree_hash(address).unwrap(),
                xp_wire::tree_hash(&bytes)
            );
            assert_eq!(output.ergo_tree.as_deref(), Some(script));
        }
        for script in [
            "00".into(),
            "0008d300".into(),
            "00".repeat(MAX_ADDRESS_SCRIPT_BYTES + 1),
        ] {
            let mut tx = detail_fixture();
            tx["outputs"][0]["ergoTree"] = json!(script);
            let parsed = parse_pending(&tx.to_string(), &[1; 32]).unwrap();
            let output = &parsed.summary.details.outputs[0];
            assert!(output.address.is_none());
            assert_eq!(output.ergo_tree.as_deref(), Some(script.as_str()));
            assert!(!output.ergo_tree_truncated);
        }
    }
    #[test]
    fn exact_fee_and_transaction_identity_are_preserved() {
        let block: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/receipts/storage-rent-block.json"
        ))
        .unwrap();
        let tx = &block["blockTransactions"]["transactions"][1];
        let id = xp_types::parse_hex32(tx["id"].as_str().unwrap()).unwrap();
        let parsed = parse_pending(&tx.to_string(), &id).unwrap();
        assert_eq!(parsed.summary.fee.as_deref(), Some("1100000"));
        assert_eq!(parsed.inputs.len(), 2);
        assert!(parse_pending(&tx.to_string(), &[0; 32]).is_err());
        let mut modified = tx.clone();
        modified["outputs"][0]["ergoTree"] = serde_json::json!(xp_store::FEE_TREE_HEX);
        modified["outputs"][0]["value"] = serde_json::json!(9007199254740993u64);
        assert_eq!(
            parse_pending(&modified.to_string(), &id)
                .unwrap()
                .summary
                .fee
                .as_deref(),
            Some("9007199255840993")
        );
        modified["inputs"][1] = modified["inputs"][0].clone();
        assert!(parse_pending(&modified.to_string(), &id).is_err());
    }

    #[test]
    fn cache_is_bounded_and_expires_without_evicting_active_requests() {
        let cache = Observations::default();
        let now = Instant::now();
        let mut held = Vec::new();
        for n in 0..MAX_ENTRIES {
            let mut id = [0; 32];
            id[..8].copy_from_slice(&(n as u64).to_be_bytes());
            held.push(cache.entry_at(id, now).unwrap());
        }
        assert!(cache.entry_at([255; 32], now).is_none());
        held.clear();
        assert!(cache.entry_at([255; 32], now).is_some());
        assert_eq!(cache.entries.lock().unwrap().len(), MAX_ENTRIES);
        assert!(cache
            .entry_at([254; 32], now + Duration::from_secs(RETENTION_SECONDS))
            .is_some());
        assert_eq!(cache.entries.lock().unwrap().len(), 1);
    }
}
