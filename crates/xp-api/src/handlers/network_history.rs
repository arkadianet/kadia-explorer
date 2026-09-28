//! Bounded canonical-header history. Height buckets are never presented as calendar days.
use crate::{history_blocking, ApiError, AppState};
use axum::{
    extract::{rejection::QueryRejection, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use xp_store::{rows::HeaderRow, Reader, StoreError};

const MAX_BLOCKS: u32 = 20_160;
const MAX_BUCKETS: u32 = 120;
const MAX_DECODED: usize = 64 * 1024 * 1024;
const MAX_RESPONSE: usize = 256 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Params {
    from_height: String,
    to_height: String,
    buckets: Option<String>,
    end_block_id: Option<String>,
}

#[derive(Serialize)]
pub struct Anchor {
    height: u32,
    block_id: String,
}
#[derive(Serialize)]
pub struct Bucket {
    from_height: u32,
    to_height: u32,
    block_count: u32,
    transaction_count: String,
    fees: String,
    size_bytes: String,
    difficulty_min: String,
    difficulty_max: String,
    difficulty_end: String,
    first_timestamp: u64,
    last_timestamp: u64,
    earliest_timestamp: u64,
    latest_timestamp: u64,
}
#[derive(Serialize)]
pub struct History {
    scope: &'static str,
    consistency: &'static str,
    complete: bool,
    full_history: bool,
    partial_from: Option<u32>,
    indexed_height: u32,
    anchor: Anchor,
    requested_buckets: u32,
    bucket_width: u32,
    totals: Bucket,
    buckets: Vec<Bucket>,
}

fn problem(status: StatusCode, code: &'static str, detail: &str) -> ApiError {
    ApiError::History {
        status,
        code,
        detail: detail.into(),
    }
}
fn invalid(detail: &str) -> ApiError {
    problem(
        StatusCode::BAD_REQUEST,
        "invalid_network_history_query",
        detail,
    )
}
fn limited(code: &'static str) -> ApiError {
    problem(
        StatusCode::UNPROCESSABLE_ENTITY,
        code,
        "This range exceeds the network-history budget. Request a smaller height range.",
    )
}
fn number(raw: &str) -> Result<u32, ApiError> {
    if raw.is_empty()
        || raw.len() > 10
        || !raw.bytes().all(|c| c.is_ascii_digit())
        || raw.starts_with('0')
    {
        return Err(invalid(
            "Heights and bucket counts must be positive decimal integers.",
        ));
    }
    raw.parse()
        .map_err(|_| invalid("An integer exceeds its supported range."))
}
struct Budget {
    bytes: usize,
    work: u32,
    deadline: Instant,
}
impl Budget {
    fn new() -> Self {
        Self {
            bytes: 0,
            work: 0,
            deadline: Instant::now() + Duration::from_secs(4),
        }
    }
    fn check(&self) -> Result<(), ApiError> {
        if Instant::now() > self.deadline {
            Err(limited("network_history_deadline"))
        } else {
            Ok(())
        }
    }
    fn admit(&mut self, bytes: usize) -> Result<(), StoreError> {
        if Instant::now() > self.deadline {
            return Err(StoreError::ReadLimit("network_history_deadline"));
        }
        self.work += 1;
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .filter(|n| *n <= MAX_DECODED)
            .ok_or(StoreError::ReadLimit("network_history_decode_limit"))?;
        if self.work > MAX_BLOCKS {
            return Err(StoreError::ReadLimit("network_history_work_limit"));
        }
        Ok(())
    }
}
struct Aggregate {
    from: u32,
    to: u32,
    count: u32,
    txs: u64,
    fees: u128,
    bytes: u64,
    min: u128,
    max: u128,
    end: u128,
    first: u64,
    last: u64,
    earliest: u64,
    latest: u64,
}
impl Aggregate {
    fn new(height: u32, row: &HeaderRow) -> Self {
        Self {
            from: height,
            to: height,
            count: 0,
            txs: 0,
            fees: 0,
            bytes: 0,
            min: row.difficulty,
            max: row.difficulty,
            end: row.difficulty,
            first: row.timestamp,
            last: row.timestamp,
            earliest: row.timestamp,
            latest: row.timestamp,
        }
    }
    fn add(&mut self, height: u32, row: &HeaderRow) {
        self.to = height;
        self.count += 1;
        self.txs += u64::from(row.tx_count);
        self.fees += u128::from(row.fees);
        self.bytes += u64::from(row.size);
        self.min = self.min.min(row.difficulty);
        self.max = self.max.max(row.difficulty);
        self.end = row.difficulty;
        self.last = row.timestamp;
        self.earliest = self.earliest.min(row.timestamp);
        self.latest = self.latest.max(row.timestamp);
    }
    fn finish(self) -> Bucket {
        Bucket {
            from_height: self.from,
            to_height: self.to,
            block_count: self.count,
            transaction_count: self.txs.to_string(),
            fees: self.fees.to_string(),
            size_bytes: self.bytes.to_string(),
            difficulty_min: self.min.to_string(),
            difficulty_max: self.max.to_string(),
            difficulty_end: self.end.to_string(),
            first_timestamp: self.first,
            last_timestamp: self.last,
            earliest_timestamp: self.earliest,
            latest_timestamp: self.latest,
        }
    }
}

fn read(
    rd: &Reader,
    from: u32,
    to: u32,
    requested_buckets: u32,
    pin: Option<[u8; 32]>,
) -> Result<History, ApiError> {
    let mut budget = Budget::new();
    let indexed_height = rd.indexed_height()?.ok_or_else(|| {
        problem(
            StatusCode::NOT_FOUND,
            "network_history_unavailable",
            "No indexed block headers are available.",
        )
    })?;
    let partial_from = rd.partial_from()?;
    let full_history = rd.full_history()?;
    if to > indexed_height {
        return Err(problem(
            if pin.is_some() {
                StatusCode::CONFLICT
            } else {
                StatusCode::NOT_FOUND
            },
            if pin.is_some() {
                "network_history_conflict"
            } else {
                "network_history_unavailable"
            },
            "The requested end height is not currently indexed. Reload the range explicitly.",
        ));
    }
    if from < partial_from.unwrap_or(1).max(1) {
        return Err(problem(
            StatusCode::UNPROCESSABLE_ENTITY,
            "network_history_incomplete",
            "The requested range starts before retained block history. Choose a retained range.",
        ));
    }
    let end_id = rd
        .header_id_at(to)?
        .ok_or_else(|| ApiError::Integrity("missing history end header".into()))?;
    if pin.is_some_and(|id| id != end_id) {
        return Err(problem(StatusCode::CONFLICT, "network_history_conflict",
            "The pinned end block changed. Reload the range explicitly to inspect the new canonical history."));
    }
    let width = (to - from + 1).div_ceil(requested_buckets);
    let mut buckets = Vec::new();
    let mut total: Option<Aggregate> = None;
    let mut current: Option<Aggregate> = None;
    let mut previous_id = None;
    for height in from..=to {
        budget.check()?;
        let row = rd
            .header_at_admitted(height, |bytes| budget.admit(bytes))
            .map_err(|error| match error {
                StoreError::ReadLimit(code) => limited(code),
                other => ApiError::from(other),
            })?
            .ok_or_else(|| ApiError::Integrity("missing network history header".into()))?;
        if previous_id.is_some_and(|id| id != row.parent_id) || (height == to && row.id != end_id) {
            return Err(ApiError::Integrity(
                "inconsistent network history header chain".into(),
            ));
        }
        previous_id = Some(row.id);
        total
            .get_or_insert_with(|| Aggregate::new(height, &row))
            .add(height, &row);
        current
            .get_or_insert_with(|| Aggregate::new(height, &row))
            .add(height, &row);
        if (height - from + 1).is_multiple_of(width) || height == to {
            buckets.push(current.take().expect("current aggregate exists").finish());
        }
        budget.check()?;
    }
    let response = History {
        scope: "canonical_block_headers",
        consistency: "single_reader",
        complete: true,
        full_history,
        partial_from,
        indexed_height,
        anchor: Anchor {
            height: to,
            block_id: hex::encode(end_id),
        },
        requested_buckets,
        bucket_width: width,
        totals: total.expect("nonempty validated range").finish(),
        buckets,
    };
    if serde_json::to_vec(&response)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .len()
        > MAX_RESPONSE
    {
        return Err(limited("network_history_response_limit"));
    }
    budget.check()?;
    Ok(response)
}

pub async fn history(
    State(state): State<AppState>,
    query: Result<Query<Params>, QueryRejection>,
) -> Result<Json<History>, ApiError> {
    let Query(params) = query.map_err(|_| {
        invalid("Expected from_height, to_height, optional buckets and end_block_id.")
    })?;
    let from = number(&params.from_height)?;
    let to = number(&params.to_height)?;
    let buckets = params
        .buckets
        .as_deref()
        .map(number)
        .transpose()?
        .unwrap_or(60);
    if to < from || to - from >= MAX_BLOCKS || buckets > MAX_BUCKETS {
        return Err(invalid(
            "Use an inclusive range of at most 20,160 blocks and 1–120 buckets.",
        ));
    }
    let pin = params
        .end_block_id
        .as_deref()
        .map(crate::dto::parse_id)
        .transpose()?;
    Ok(Json(
        history_blocking(&state, move |rd| read(rd, from, to, buckets, pin)).await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_decode_work_and_deadline_are_bounded() {
        let mut budget = Budget::new();
        budget.admit(MAX_DECODED).unwrap();
        assert!(matches!(
            budget.admit(1),
            Err(StoreError::ReadLimit("network_history_decode_limit"))
        ));
        let mut budget = Budget::new();
        budget.work = MAX_BLOCKS;
        assert!(budget.admit(0).is_err());
        let mut budget = Budget::new();
        budget.deadline = Instant::now() - Duration::from_millis(1);
        assert!(budget.admit(0).is_err());
        assert!(budget.check().is_err());
    }
}
