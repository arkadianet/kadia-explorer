//! Observed header signals, never miner identities, earnings or activated governance rules.
use crate::{history_blocking, ApiError, AppState};
use axum::{
    extract::{rejection::QueryRejection, Query, State},
    http::StatusCode,
    Json,
};
use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer, Serialize,
};
use std::{
    collections::BTreeMap,
    fmt,
    time::{Duration, Instant},
};
use xp_store::{Reader, StoreError};

const MAX_BLOCKS: u32 = 20_160;
const MAX_DECODED: usize = 64 * 1024 * 1024;
const MAX_RESPONSE: usize = 128 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Params {
    from_height: String,
    to_height: String,
    top: Option<String>,
    end_block_id: Option<String>,
}
#[derive(Serialize)]
pub struct Anchor {
    height: u32,
    block_id: String,
}
#[derive(Serialize)]
pub struct Totals {
    fees: String,
    transaction_count: String,
}
#[derive(Serialize)]
pub struct MinerKey {
    public_key: String,
    block_count: u32,
    first_height: u32,
    last_height: u32,
    first_block_id: String,
    last_block_id: String,
    fees: String,
}
#[derive(Serialize)]
pub struct MinerKeys {
    distinct_count: u32,
    items: Vec<MinerKey>,
    other_key_count: u32,
    other_block_count: u32,
    other_fees: String,
}
#[derive(Serialize)]
pub struct Version {
    version: u8,
    block_count: u32,
}
#[derive(Serialize)]
pub struct VoteTuple {
    votes: String,
    block_count: u32,
}
#[derive(Serialize)]
pub struct Votes {
    known_blocks: u32,
    unknown_blocks: u32,
    zero_vote_blocks: u32,
    distinct_tuples: u32,
    items: Vec<VoteTuple>,
    other_tuple_count: u32,
    other_block_count: u32,
}
#[derive(Serialize)]
pub struct Overview {
    scope: &'static str,
    consistency: &'static str,
    complete: bool,
    from_height: u32,
    to_height: u32,
    block_count: u32,
    indexed_height: u32,
    full_history: bool,
    partial_from: Option<u32>,
    anchor: Anchor,
    top: u32,
    totals: Totals,
    miner_keys: MinerKeys,
    versions: Vec<Version>,
    votes: Votes,
}
fn problem(status: StatusCode, code: &'static str, detail: &str) -> ApiError {
    ApiError::History {
        status,
        code,
        detail: detail.into(),
    }
}
fn invalid(detail: &str) -> ApiError {
    problem(StatusCode::BAD_REQUEST, "invalid_mining_query", detail)
}
fn limited(code: &'static str) -> ApiError {
    problem(
        StatusCode::UNPROCESSABLE_ENTITY,
        code,
        "This range exceeds the mining-header budget. Request a smaller height range.",
    )
}
fn number(raw: &str) -> Result<u32, ApiError> {
    if raw.is_empty()
        || raw.len() > 10
        || raw.starts_with('0')
        || !raw.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(invalid(
            "Use positive decimal heights and a top count from 1 to 50.",
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
            Err(limited("mining_deadline"))
        } else {
            Ok(())
        }
    }
    fn admit(&mut self, bytes: usize) -> Result<(), StoreError> {
        if Instant::now() > self.deadline {
            return Err(StoreError::ReadLimit("mining_deadline"));
        }
        self.work += 1;
        if self.work > MAX_BLOCKS {
            return Err(StoreError::ReadLimit("mining_work_limit"));
        }
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .filter(|n| *n <= MAX_DECODED)
            .ok_or(StoreError::ReadLimit("mining_decode_limit"))?;
        Ok(())
    }
}

// Deserialize only the tiny vote field. Ignored fields are skipped without allocating a JSON
// Value tree; malformed/missing/duplicate votes remain unknown rather than becoming zeroes.
#[derive(Default)]
struct ParsedVotes(Option<[u8; 3]>);
impl<'de> Deserialize<'de> for ParsedVotes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct VoteVisitor;
        impl Visitor<'_> for VoteVisitor {
            type Value = ParsedVotes;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("three hex-encoded vote bytes")
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                let mut bytes = [0; 3];
                Ok(ParsedVotes(
                    (value.len() == 6 && hex::decode_to_slice(value, &mut bytes).is_ok())
                        .then_some(bytes),
                ))
            }
        }
        deserializer.deserialize_str(VoteVisitor)
    }
}
#[derive(Deserialize)]
struct VoteHeader {
    #[serde(default)]
    votes: ParsedVotes,
}
fn votes(raw: &str) -> Option<[u8; 3]> {
    if !raw.trim_start().starts_with('{') {
        return None;
    }
    serde_json::from_str::<VoteHeader>(raw).ok()?.votes.0
}
struct KeyAggregate {
    count: u32,
    first: u32,
    last: u32,
    first_id: [u8; 32],
    last_id: [u8; 32],
    fees: u128,
}

fn read(
    rd: &Reader,
    from: u32,
    to: u32,
    top: u32,
    pin: Option<[u8; 32]>,
) -> Result<Overview, ApiError> {
    let mut budget = Budget::new();
    let indexed_height = rd.indexed_height()?.ok_or_else(|| {
        problem(
            StatusCode::NOT_FOUND,
            "mining_unavailable",
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
                "mining_conflict"
            } else {
                "mining_unavailable"
            },
            "The requested end height is not indexed. Reload the range explicitly.",
        ));
    }
    if from < partial_from.unwrap_or(1).max(1) {
        return Err(problem(
            StatusCode::UNPROCESSABLE_ENTITY,
            "mining_incomplete",
            "The requested range starts before retained block history. Choose a retained range.",
        ));
    }
    let end_id = rd
        .header_id_at(to)?
        .ok_or_else(|| ApiError::Integrity("missing mining end header".into()))?;
    if pin.is_some_and(|id| id != end_id) {
        return Err(problem(StatusCode::CONFLICT, "mining_conflict", "The pinned end block changed. Clear the pin explicitly to inspect the new canonical history."));
    }
    let mut keys: BTreeMap<[u8; 33], KeyAggregate> = BTreeMap::new();
    let mut versions: BTreeMap<u8, u32> = BTreeMap::new();
    let mut tuples: BTreeMap<[u8; 3], u32> = BTreeMap::new();
    let mut unknown = 0;
    let mut total_fees = 0u128;
    let mut transactions = 0u64;
    let mut previous_id = None;
    for height in from..=to {
        budget.check()?;
        let row = rd
            .header_at_admitted(height, |bytes| budget.admit(bytes))
            .map_err(|e| match e {
                StoreError::ReadLimit(code) => limited(code),
                other => ApiError::from(other),
            })?
            .ok_or_else(|| ApiError::Integrity("missing mining header".into()))?;
        if previous_id.is_some_and(|id| id != row.parent_id) || (height == to && row.id != end_id) {
            return Err(ApiError::Integrity(
                "inconsistent mining header chain".into(),
            ));
        }
        previous_id = Some(row.id);
        let key = keys.entry(row.miner_pk).or_insert(KeyAggregate {
            count: 0,
            first: height,
            last: height,
            first_id: row.id,
            last_id: row.id,
            fees: 0,
        });
        key.count += 1;
        key.last = height;
        key.last_id = row.id;
        key.fees += u128::from(row.fees);
        total_fees += u128::from(row.fees);
        transactions += u64::from(row.tx_count);
        *versions.entry(row.version).or_default() += 1;
        match votes(&row.raw_json) {
            Some(tuple) => *tuples.entry(tuple).or_default() += 1,
            None => unknown += 1,
        }
        budget.check()?;
    }
    let distinct_count = keys.len() as u32;
    let mut keys = keys.into_iter().collect::<Vec<_>>();
    keys.sort_by(|(ak, a), (bk, b)| b.count.cmp(&a.count).then(ak.cmp(bk)));
    let mut other_blocks = 0;
    let mut other_fees = 0u128;
    let key_items = keys
        .into_iter()
        .enumerate()
        .filter_map(|(i, (pk, key))| {
            if i >= top as usize {
                other_blocks += key.count;
                other_fees += key.fees;
                None
            } else {
                Some(MinerKey {
                    public_key: hex::encode(pk),
                    block_count: key.count,
                    first_height: key.first,
                    last_height: key.last,
                    first_block_id: hex::encode(key.first_id),
                    last_block_id: hex::encode(key.last_id),
                    fees: key.fees.to_string(),
                })
            }
        })
        .collect::<Vec<_>>();
    let distinct_tuples = tuples.len() as u32;
    let zero_vote_blocks = tuples.get(&[0, 0, 0]).copied().unwrap_or(0);
    let mut tuples = tuples.into_iter().collect::<Vec<_>>();
    tuples.sort_by(|(a, ac), (b, bc)| bc.cmp(ac).then(a.cmp(b)));
    let mut other_vote_blocks = 0;
    let vote_items = tuples
        .into_iter()
        .enumerate()
        .filter_map(|(i, (tuple, count))| {
            if i >= top as usize {
                other_vote_blocks += count;
                None
            } else {
                Some(VoteTuple {
                    votes: hex::encode(tuple),
                    block_count: count,
                })
            }
        })
        .collect::<Vec<_>>();
    let count = to - from + 1;
    let response = Overview {
        scope: "canonical_block_headers",
        consistency: "single_reader",
        complete: true,
        from_height: from,
        to_height: to,
        block_count: count,
        indexed_height,
        full_history,
        partial_from,
        anchor: Anchor {
            height: to,
            block_id: hex::encode(end_id),
        },
        top,
        totals: Totals {
            fees: total_fees.to_string(),
            transaction_count: transactions.to_string(),
        },
        miner_keys: MinerKeys {
            distinct_count,
            other_key_count: distinct_count - key_items.len() as u32,
            items: key_items,
            other_block_count: other_blocks,
            other_fees: other_fees.to_string(),
        },
        versions: versions
            .into_iter()
            .map(|(version, block_count)| Version {
                version,
                block_count,
            })
            .collect(),
        votes: Votes {
            known_blocks: count - unknown,
            unknown_blocks: unknown,
            zero_vote_blocks,
            distinct_tuples,
            other_tuple_count: distinct_tuples - vote_items.len() as u32,
            items: vote_items,
            other_block_count: other_vote_blocks,
        },
    };
    if serde_json::to_vec(&response)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .len()
        > MAX_RESPONSE
    {
        return Err(limited("mining_response_limit"));
    }
    budget.check()?;
    Ok(response)
}

pub async fn overview(
    State(state): State<AppState>,
    query: Result<Query<Params>, QueryRejection>,
) -> Result<Json<Overview>, ApiError> {
    let Query(params) = query
        .map_err(|_| invalid("Expected from_height, to_height, optional top and end_block_id."))?;
    let from = number(&params.from_height)?;
    let to = number(&params.to_height)?;
    let top = params.top.as_deref().map(number).transpose()?.unwrap_or(20);
    if to < from || to - from >= MAX_BLOCKS || top > 50 {
        return Err(invalid("Use 1–20,160 blocks and a top count from 1 to 50."));
    }
    let pin = params
        .end_block_id
        .as_deref()
        .map(crate::dto::parse_id)
        .transpose()?;
    Ok(Json(
        history_blocking(&state, move |rd| read(rd, from, to, top, pin)).await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_votes_are_not_zeroes() {
        assert_eq!(votes(r#"{"votes":"000000"}"#), Some([0; 3]));
        assert_eq!(
            votes(r#"{"votes":"01ff00","ignored":[1,2]}"#),
            Some([1, 255, 0])
        );
        for raw in [
            "{}",
            "[]",
            r#"["000000"]"#,
            r#"{"votes":null}"#,
            r#"{"votes":"0000"}"#,
            r#"{"votes":"00gg00"}"#,
            r#"{"votes":"000000","votes":"010000"}"#,
            "{",
        ] {
            assert_eq!(votes(raw), None, "{raw}");
        }
    }
    #[test]
    fn shared_decode_work_and_deadline_are_bounded() {
        let mut budget = Budget::new();
        budget.admit(MAX_DECODED).unwrap();
        assert!(matches!(
            budget.admit(1),
            Err(StoreError::ReadLimit("mining_decode_limit"))
        ));
        let mut budget = Budget::new();
        budget.work = MAX_BLOCKS;
        assert!(matches!(
            budget.admit(0),
            Err(StoreError::ReadLimit("mining_work_limit"))
        ));
        let mut budget = Budget::new();
        budget.deadline = Instant::now() - Duration::from_millis(1);
        assert!(budget.admit(0).is_err());
        assert!(budget.check().is_err());
    }
}
