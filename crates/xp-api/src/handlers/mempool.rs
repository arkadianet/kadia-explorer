//! A bounded primary-node observation, independent of confirmed index state.
use crate::{observations::now_ms, ApiError, AppState};
use axum::{
    extract::{rejection::QueryRejection, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::Semaphore;
use xp_source::{BlockSource, SourceError};

const LIMIT: usize = 100;
const MAX_BODY: usize = 2 * 1024 * 1024;
const MAX_WORK: usize = 10_000;
const MAX_EDGES: usize = 256;
const MAX_SHARED_GROUPS: usize = 16;
const MAX_SHARED_MEMBERS: usize = 256;
const TTL: Duration = Duration::from_secs(5);
const ERROR_TTL: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, Serialize)]
pub struct Item {
    id: String,
    input_count: usize,
    data_input_count: usize,
    output_count: usize,
    size: Option<u32>,
    fee: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    scope: &'static str,
    source: &'static str,
    checked_at_ms: u64,
    expires_at_ms: u64,
    cached: bool,
    limit: usize,
    limit_reached: bool,
    observed_count: usize,
    items: Vec<Item>,
    connections: Connections,
}
#[derive(Clone, Debug, Serialize)]
struct Edge {
    producer_id: String,
    consumer_id: String,
    box_id: String,
    kind: &'static str,
}
#[derive(Clone, Debug, Serialize)]
struct SharedInput {
    box_id: String,
    transaction_count: usize,
    transaction_ids: Vec<String>,
    truncated: bool,
}
#[derive(Clone, Debug, Serialize)]
struct Connections {
    scope: &'static str,
    output_count: usize,
    identified_output_count: usize,
    edge_count: usize,
    edges_truncated: bool,
    edges: Vec<Edge>,
    shared_input_count: usize,
    shared_inputs_truncated: bool,
    shared_inputs: Vec<SharedInput>,
}
#[derive(Debug)]
struct Cached {
    until: Instant,
    value: Result<Snapshot, &'static str>,
}
#[derive(Debug)]
pub(crate) struct Cache {
    cached: Mutex<Option<Cached>>,
    permits: Arc<Semaphore>,
}
impl Default for Cache {
    fn default() -> Self {
        Self {
            cached: Mutex::new(None),
            permits: Arc::new(Semaphore::new(1)),
        }
    }
}
impl Cache {
    fn cached(&self) -> Option<Result<Snapshot, &'static str>> {
        let cache = self
            .cached
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        cache
            .as_ref()
            .filter(|entry| entry.until > Instant::now())
            .map(|entry| {
                entry.value.clone().map(|mut snapshot| {
                    snapshot.cached = true;
                    snapshot
                })
            })
    }
    async fn observe(&self, source: &Arc<dyn BlockSource>) -> Result<Snapshot, &'static str> {
        if let Some(value) = self.cached() {
            return value;
        }
        let permit = Arc::new(
            self.permits
                .clone()
                .try_acquire_owned()
                .map_err(|_| "mempool_busy")?,
        );
        if let Some(value) = self.cached() {
            return value;
        }
        let parser_permit = permit.clone();
        let value = match tokio::time::timeout(
            Duration::from_secs(2),
            source.unconfirmed_transactions_json(LIMIT as u16, MAX_BODY),
        )
        .await
        {
            Ok(Ok(body)) if body.len() <= MAX_BODY => tokio::task::spawn_blocking(move || {
                // Cancellation cannot release admission before CPU parsing finishes.
                let _permit = parser_permit;
                parse(&body)
            })
            .await
            .unwrap_or(Err("mempool_invalid_response")),
            Ok(Ok(_)) | Ok(Err(SourceError::Decode(_))) => Err("mempool_invalid_response"),
            Ok(Err(SourceError::Capability(_))) => Err("mempool_unsupported"),
            _ => Err("mempool_unavailable"),
        };
        let ttl = if value.is_ok() { TTL } else { ERROR_TTL };
        *self
            .cached
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(Cached {
            until: Instant::now() + ttl,
            value: value.clone(),
        });
        value
    }
}

#[derive(Deserialize)]
struct Input {
    #[serde(rename = "boxId")]
    id: String,
}
#[derive(Deserialize)]
struct Output {
    #[serde(rename = "boxId")]
    id: Option<String>,
    #[serde(rename = "transactionId")]
    transaction_id: Option<String>,
    index: Option<usize>,
    value: u64,
    #[serde(rename = "ergoTree")]
    tree: String,
}
#[derive(Deserialize)]
struct Transaction {
    id: String,
    inputs: Vec<Input>,
    #[serde(rename = "dataInputs")]
    data_inputs: Vec<Input>,
    outputs: Vec<Output>,
    size: Option<u32>,
}
fn valid_id(raw: &str) -> bool {
    raw.len() == 64
        && raw
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn parse(raw: &str) -> Result<Snapshot, &'static str> {
    let invalid = "mempool_invalid_response";
    if raw.len() > MAX_BODY {
        return Err(invalid);
    }
    let transactions: Vec<Transaction> = serde_json::from_str(raw).map_err(|_| invalid)?;
    if transactions.len() > LIMIT {
        return Err(invalid);
    }
    let limit_reached = transactions.len() == LIMIT;
    let mut ids = HashSet::new();
    let mut work = 0usize;
    let mut items = Vec::new();
    let mut producers = BTreeMap::new();
    let mut spenders: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut output_count = 0;
    for tx in &transactions {
        work = work
            .checked_add(1 + tx.inputs.len() + tx.data_inputs.len() + tx.outputs.len())
            .filter(|n| *n <= MAX_WORK)
            .ok_or(invalid)?;
        if !valid_id(&tx.id)
            || !ids.insert(tx.id.clone())
            || tx.inputs.is_empty()
            || tx.inputs.len() > 512
            || tx.data_inputs.len() > 512
            || tx.outputs.is_empty()
            || tx.outputs.len() > 4096
            || tx.size == Some(0)
        {
            return Err(invalid);
        }
        let mut inputs = HashSet::new();
        for input in &tx.inputs {
            if !valid_id(&input.id) || !inputs.insert(&input.id) {
                return Err(invalid);
            }
            spenders.entry(&input.id).or_default().insert(&tx.id);
        }
        let mut data_inputs = HashSet::new();
        for input in &tx.data_inputs {
            if !valid_id(&input.id) || !data_inputs.insert(&input.id) {
                return Err(invalid);
            }
        }
        let mut fee = 0u128;
        output_count += tx.outputs.len();
        for (index, output) in tx.outputs.iter().enumerate() {
            if output.value > i64::MAX as u64
                || output.tree.is_empty()
                || output.tree.len() > 16_384
                || output.tree.len() % 2 != 0
                || !output.tree.bytes().all(|b| b.is_ascii_hexdigit())
                || output
                    .transaction_id
                    .as_ref()
                    .is_some_and(|id| id != &tx.id)
                || output.index.is_some_and(|supplied| supplied != index)
            {
                return Err(invalid);
            }
            if let Some(id) = &output.id {
                if !valid_id(id) || producers.insert(id.as_str(), tx.id.as_str()).is_some() {
                    return Err(invalid);
                }
            }
            if output.tree.eq_ignore_ascii_case(xp_store::FEE_TREE_HEX) {
                fee += u128::from(output.value);
            }
        }
        if items.len() < LIMIT {
            items.push(Item {
                id: tx.id.clone(),
                input_count: tx.inputs.len(),
                data_input_count: tx.data_inputs.len(),
                output_count: tx.outputs.len(),
                size: tx.size,
                fee: Some(fee.to_string()),
            });
        }
    }
    // Build references only after every producer is known: node order is not
    // topological. The same snapshot is the entire scope; no extra lookups occur.
    let mut references = BTreeSet::new();
    for tx in &transactions {
        for (inputs, kind) in [(&tx.inputs, "spend"), (&tx.data_inputs, "read")] {
            for input in inputs {
                if let Some(producer) = producers.get(input.id.as_str()) {
                    if *producer == tx.id {
                        return Err(invalid);
                    }
                    references.insert((*producer, tx.id.as_str(), input.id.as_str(), kind));
                }
            }
        }
    }
    let edge_count = references.len();
    let edges: Vec<_> = references
        .into_iter()
        .take(MAX_EDGES)
        .map(|(producer, consumer, box_id, kind)| Edge {
            producer_id: producer.into(),
            consumer_id: consumer.into(),
            box_id: box_id.into(),
            kind,
        })
        .collect();
    let shared: Vec<_> = spenders
        .into_iter()
        .filter(|(_, ids)| ids.len() > 1)
        .collect();
    let shared_input_count = shared.len();
    let mut members_left = MAX_SHARED_MEMBERS;
    let mut shared_inputs = Vec::new();
    for (box_id, ids) in shared.into_iter().take(MAX_SHARED_GROUPS) {
        if members_left < 2 {
            break;
        }
        let transaction_count = ids.len();
        let transaction_ids: Vec<String> = ids
            .into_iter()
            .take(members_left)
            .map(str::to_owned)
            .collect();
        members_left -= transaction_ids.len();
        shared_inputs.push(SharedInput {
            box_id: box_id.into(),
            truncated: transaction_ids.len() < transaction_count,
            transaction_count,
            transaction_ids,
        });
    }
    let connections = Connections {
        scope: "returned_snapshot_only",
        output_count,
        identified_output_count: producers.len(),
        edge_count,
        edges_truncated: edges.len() < edge_count,
        edges,
        shared_input_count,
        shared_inputs_truncated: shared_inputs.len() < shared_input_count
            || shared_inputs.iter().any(|group| group.truncated),
        shared_inputs,
    };
    let checked_at_ms = now_ms();
    Ok(Snapshot {
        scope: "configured_node_mempool",
        source: "configured_primary_node",
        checked_at_ms,
        expires_at_ms: checked_at_ms + TTL.as_millis() as u64,
        cached: false,
        limit: LIMIT,
        limit_reached,
        observed_count: items.len(),
        items,
        connections,
    })
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Params {}
fn unavailable(code: &'static str) -> ApiError {
    let detail = match code {
        "mempool_not_configured" => "No primary node connection is configured for mempool observations.",
        "mempool_unsupported" => "The configured primary node does not support bounded mempool listing.",
        "mempool_busy" => "A mempool observation is already in progress. Retry shortly.",
        "mempool_invalid_response" => "The node response could not be interpreted within the mempool limits. No pending list is available.",
        _ => "The configured node did not provide a fresh mempool observation. This does not mean its mempool is empty.",
    };
    ApiError::History {
        status: StatusCode::SERVICE_UNAVAILABLE,
        code,
        detail: detail.into(),
    }
}
pub async fn snapshot(
    State(state): State<AppState>,
    query: Result<Query<Params>, QueryRejection>,
    source: Option<Extension<Arc<dyn BlockSource>>>,
) -> Response {
    let result = match query {
        Err(_) => Err(ApiError::BadRequest(
            "The mempool snapshot accepts no query parameters.".into(),
        )),
        Ok(_) => match source {
            None => Err(unavailable("mempool_not_configured")),
            Some(source) => state
                .counters
                .mempool
                .observe(&source.0)
                .await
                .map_err(unavailable),
        },
    };
    let response = match result {
        Ok(snapshot) => Json(snapshot).into_response(),
        Err(error) => error.into_response(),
    };
    ([(header::CACHE_CONTROL, "no-store")], response).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn id(n: u32) -> String {
        format!("{n:064x}")
    }
    fn tx(n: u32, inputs: &[u32], reads: &[u32], outputs: &[u32]) -> Value {
        json!({
            "id": id(n),
            "inputs": inputs.iter().map(|n| json!({"boxId":id(*n)})).collect::<Vec<_>>(),
            "dataInputs": reads.iter().map(|n| json!({"boxId":id(*n)})).collect::<Vec<_>>(),
            "outputs": outputs.iter().enumerate().map(|(index, output)| json!({
                "boxId":id(*output), "transactionId":id(n), "index":index,
                "value":1000000, "ergoTree":"00"
            })).collect::<Vec<_>>()
        })
    }
    fn snapshot(rows: Vec<Value>) -> Snapshot {
        parse(&serde_json::to_string(&rows).unwrap()).unwrap()
    }
    #[test]
    fn references_are_snapshot_local_and_independent_of_node_order() {
        let child = tx(2, &[101], &[102], &[103]);
        let parent = tx(1, &[900], &[], &[101, 102]);
        let sibling = tx(3, &[900], &[102], &[104]);
        let first = snapshot(vec![child.clone(), parent.clone(), sibling.clone()]);
        let reordered = snapshot(vec![sibling, parent, child]);
        let connections = serde_json::to_value(&first.connections).unwrap();
        assert_eq!(
            connections,
            serde_json::to_value(&reordered.connections).unwrap()
        );
        assert_eq!(connections["edge_count"], 3);
        assert_eq!(connections["output_count"], 4);
        assert_eq!(connections["identified_output_count"], 4);
        assert_eq!(connections["edges"][0]["producer_id"], id(1));
        assert_eq!(connections["edges"][0]["consumer_id"], id(2));
        assert_eq!(connections["edges"][0]["kind"], "spend");
        assert_eq!(connections["edges"][1]["kind"], "read");
        assert_eq!(connections["shared_input_count"], 1);
        assert_eq!(connections["shared_inputs"][0]["box_id"], id(900));
        assert_eq!(
            connections["shared_inputs"][0]["transaction_ids"],
            json!([id(1), id(3)])
        );
        assert_eq!(
            first.items[0].id,
            id(2),
            "summary order remains the node's order"
        );
    }
    #[test]
    fn missing_output_ids_reduce_coverage_and_unmatched_inputs_are_not_classified() {
        let mut parent = tx(1, &[900], &[], &[101, 102]);
        parent["outputs"][0]
            .as_object_mut()
            .unwrap()
            .remove("boxId");
        let result = snapshot(vec![tx(2, &[101, 999], &[], &[103]), parent]);
        assert_eq!(result.connections.output_count, 3);
        assert_eq!(result.connections.identified_output_count, 2);
        assert_eq!(result.connections.edge_count, 0);
        assert_eq!(result.connections.shared_input_count, 0);
    }
    #[test]
    fn ambiguous_producers_and_contradictory_identity_are_rejected() {
        let original = tx(1, &[900], &[], &[101, 102]);
        for (field, value) in [
            ("boxId", json!("invalid")),
            ("transactionId", json!(id(2))),
            ("index", json!(1)),
        ] {
            let mut bad = original.clone();
            bad["outputs"][0][field] = value;
            assert!(parse(&json!([bad]).to_string()).is_err(), "{field}");
        }
        let duplicate = tx(2, &[901], &[], &[101]);
        assert!(parse(&json!([original.clone(), duplicate]).to_string()).is_err());
        let mut duplicate_within = original;
        duplicate_within["outputs"][1]["boxId"] = json!(id(101));
        assert!(parse(&json!([duplicate_within]).to_string()).is_err());
        assert!(parse(&json!([tx(1, &[101], &[], &[101])]).to_string()).is_err());
    }
    #[test]
    fn connection_lists_have_independent_caps_without_losing_exact_scope_counts() {
        let mut rows = vec![tx(1, &[999], &[], &[101, 102, 103, 104])];
        let inputs: Vec<_> = [101, 102, 103, 104].into_iter().chain(500..520).collect();
        for n in 2..100 {
            rows.push(tx(n, &inputs, &[], &[1000 + n]));
        }
        let result = snapshot(rows);
        assert_eq!(result.connections.edge_count, 392);
        assert_eq!(result.connections.edges.len(), MAX_EDGES);
        assert!(result.connections.edges_truncated);
        assert_eq!(result.connections.shared_input_count, 24);
        assert!(result.connections.shared_inputs.len() <= MAX_SHARED_GROUPS);
        assert_eq!(
            result
                .connections
                .shared_inputs
                .iter()
                .map(|g| g.transaction_ids.len())
                .sum::<usize>(),
            MAX_SHARED_MEMBERS
        );
        assert!(result.connections.shared_inputs.last().unwrap().truncated);
        assert!(result.connections.shared_inputs_truncated);
        assert!(serde_json::to_vec(&result).unwrap().len() < 256 * 1024);
    }
    #[test]
    fn recorded_block_transaction_identities_can_be_read_without_inclusion_claims() {
        let block: Value = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/receipts/storage-rent-block.json"
        ))
        .unwrap();
        let result = parse(&block["blockTransactions"]["transactions"].to_string()).unwrap();
        assert_eq!(
            result.connections.output_count,
            result.connections.identified_output_count
        );
        assert_eq!(result.scope, "configured_node_mempool");
        assert_eq!(result.items[1].fee.as_deref(), Some("1100000"));
    }
    #[test]
    fn shared_work_limit_rejects_many_individually_valid_transactions() {
        let inputs = (0u32..100)
            .map(|n| {
                let mut id = [1; 32];
                id[..4].copy_from_slice(&n.to_be_bytes());
                serde_json::json!({"boxId":hex::encode(id)})
            })
            .collect::<Vec<_>>();
        let transactions = (0u32..100).map(|n| {let mut id=[2;32];id[..4].copy_from_slice(&n.to_be_bytes());serde_json::json!({"id":hex::encode(id),"inputs":inputs,"dataInputs":[],"outputs":[{"value":1000000,"ergoTree":"0008d3"}]})}).collect::<Vec<_>>();
        let raw = serde_json::to_string(&transactions).unwrap();
        assert!(raw.len() < MAX_BODY);
        assert_eq!(parse(&raw).unwrap_err(), "mempool_invalid_response");
    }
}
