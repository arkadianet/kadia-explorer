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
    collections::HashSet,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::Semaphore;
use xp_source::{BlockSource, SourceError};

const LIMIT: usize = 100;
const MAX_BODY: usize = 2 * 1024 * 1024;
const MAX_WORK: usize = 10_000;
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
    for tx in transactions {
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
        }
        let mut data_inputs = HashSet::new();
        for input in &tx.data_inputs {
            if !valid_id(&input.id) || !data_inputs.insert(&input.id) {
                return Err(invalid);
            }
        }
        let mut fee = 0u128;
        for output in &tx.outputs {
            if output.value > i64::MAX as u64
                || output.tree.is_empty()
                || output.tree.len() > 16_384
                || output.tree.len() % 2 != 0
                || !output.tree.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err(invalid);
            }
            if output.tree.eq_ignore_ascii_case(xp_store::FEE_TREE_HEX) {
                fee += u128::from(output.value);
            }
        }
        if items.len() < LIMIT {
            items.push(Item {
                id: tx.id,
                input_count: tx.inputs.len(),
                data_input_count: tx.data_inputs.len(),
                output_count: tx.outputs.len(),
                size: tx.size,
                fee: Some(fee.to_string()),
            });
        }
    }
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
