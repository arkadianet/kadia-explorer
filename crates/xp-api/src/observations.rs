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
    value: u64,
    #[serde(rename = "ergoTree")]
    tree: String,
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
    for input in &tx.data_inputs {
        xp_types::parse_hex32(&input.id).map_err(|_| ())?;
    }
    let mut fee = 0u64;
    for output in &tx.outputs {
        if output.tree.is_empty()
            || output.tree.len() % 2 != 0
            || !output.tree.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(());
        }
        if output.tree.eq_ignore_ascii_case(xp_store::FEE_TREE_HEX) {
            fee = fee.checked_add(output.value).ok_or(())?;
        }
    }
    Ok(PendingRecord {
        summary: PendingSummary {
            input_count: inputs.len(),
            output_count: tx.outputs.len(),
            data_input_count: tx.data_inputs.len(),
            size: tx.size,
            fee: Some(fee.to_string()),
        },
        inputs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
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
