//! A compact, fixed-work overview. Its totals cover blocks, not an inferred wall-clock day.
use crate::budget::Budget;
use crate::dto::{block_dto, BlockDto};
use crate::{blocking, ApiError, AppState};
use axum::{extract::State, Json};
use serde::Serialize;
use xp_store::rows::HeaderRow;

const BLOCKS: usize = 720;
const HOUR: u64 = 3_600_000;

#[derive(Serialize)]
pub struct NetworkSummary {
    pub scope: &'static str,
    pub requested_blocks: usize,
    pub block_count: usize,
    pub transaction_count: u64,
    pub fees: String,
    pub from_height: Option<u32>,
    pub to_height: Option<u32>,
    pub anchor_id: Option<String>,
    pub earliest_timestamp: Option<u64>,
    pub latest_timestamp: Option<u64>,
    pub partial_from: Option<u32>,
    pub recent_blocks: Vec<BlockDto>,
    pub blocks_per_hour: Vec<u64>,
    pub transactions_per_hour: Vec<u64>,
    pub fees_per_hour: Vec<String>,
    pub blocks_per_ten_minutes: Vec<u64>,
}

fn bucket(timestamp: u64, anchor: u64, step: u64, count: usize) -> Option<usize> {
    let age = anchor.checked_sub(timestamp)?;
    (age < step * count as u64).then(|| count - 1 - (age / step) as usize)
}

fn aggregate(rows: &[(u32, HeaderRow)], partial_from: Option<u32>) -> NetworkSummary {
    let anchor = rows.first().map(|(_, h)| h.timestamp).unwrap_or(0);
    let mut blocks = vec![0; 24];
    let mut txs = vec![0; 24];
    let mut fees = vec![0u128; 24];
    let mut ten = vec![0; 36];
    for (_, h) in rows {
        if let Some(i) = bucket(h.timestamp, anchor, HOUR, 24) {
            blocks[i] += 1;
            txs[i] += u64::from(h.tx_count);
            fees[i] += u128::from(h.fees);
        }
        if let Some(i) = bucket(h.timestamp, anchor, 600_000, 36) {
            ten[i] += 1;
        }
    }
    NetworkSummary {
        scope: "latest_indexed_blocks",
        requested_blocks: BLOCKS,
        block_count: rows.len(),
        transaction_count: rows.iter().map(|(_, h)| u64::from(h.tx_count)).sum(),
        fees: rows
            .iter()
            .map(|(_, h)| u128::from(h.fees))
            .sum::<u128>()
            .to_string(),
        from_height: rows.last().map(|(height, _)| *height),
        to_height: rows.first().map(|(height, _)| *height),
        anchor_id: rows.first().map(|(_, h)| hex::encode(h.id)),
        earliest_timestamp: rows.iter().map(|(_, h)| h.timestamp).min(),
        latest_timestamp: rows.iter().map(|(_, h)| h.timestamp).max(),
        partial_from,
        recent_blocks: rows
            .iter()
            .take(6)
            .map(|(height, h)| block_dto(*height, h))
            .collect(),
        blocks_per_hour: blocks,
        transactions_per_hour: txs,
        fees_per_hour: fees.into_iter().map(|f| f.to_string()).collect(),
        blocks_per_ten_minutes: ten,
    }
}

pub async fn summary(State(state): State<AppState>) -> Result<Json<NetworkSummary>, ApiError> {
    Ok(Json(
        blocking(&state, |rd| {
            let mut budget = Budget::new();
            let partial_from = rd.partial_from()?;
            let mut rows = Vec::new();
            if let Some(tip) = rd.indexed_height()? {
                for height in (partial_from.unwrap_or(1)..=tip).rev().take(BLOCKS) {
                    budget.work(1)?;
                    let header = rd
                        .header_at_admitted(height, |n| budget.admit_tx_row(n))?
                        .ok_or_else(|| ApiError::Integrity("missing summary header".into()))?;
                    rows.push((height, header));
                }
            }
            Ok(aggregate(&rows, partial_from))
        })
        .await
        .map_err(|error| match error {
            ApiError::Expansion(_) => ApiError::History {
                status: axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                code: "network_summary_limit",
                detail: "The bounded network summary is unavailable for this snapshot.".into(),
            },
            other => other,
        })?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn buckets_are_anchor_relative_and_exclude_future_and_open_lower_edge() {
        assert_eq!(bucket(100 * HOUR, 100 * HOUR, HOUR, 24), Some(23));
        assert_eq!(bucket(99 * HOUR, 100 * HOUR, HOUR, 24), Some(22));
        assert_eq!(bucket(76 * HOUR, 100 * HOUR, HOUR, 24), None);
        assert_eq!(bucket(101 * HOUR, 100 * HOUR, HOUR, 24), None);
    }
    #[test]
    fn empty_store_has_no_invented_anchor() {
        let data = aggregate(&[], Some(100));
        assert_eq!(data.block_count, 0);
        assert_eq!(data.anchor_id, None);
        assert_eq!(data.fees, "0");
        assert_eq!(data.partial_from, Some(100));
    }
    #[test]
    fn totals_remain_exact_and_include_clock_regressions_outside_chart_window() {
        let row = HeaderRow {
            id: [1; 32],
            parent_id: [0; 32],
            timestamp: 100 * HOUR,
            difficulty: 1,
            miner_pk: [0; 33],
            tx_count: 7,
            first_tx_gidx: 1,
            size: 1,
            fees: u64::MAX,
            reward: 0,
            version: 1,
            raw_json: String::new(),
        };
        let older = HeaderRow {
            timestamp: 101 * HOUR,
            tx_count: 3,
            ..row.clone()
        };
        let result = aggregate(&[(10, row), (9, older)], None);
        assert_eq!(result.transaction_count, 10);
        assert_eq!(result.fees, (u128::from(u64::MAX) * 2).to_string());
        assert_eq!(result.transactions_per_hour.iter().sum::<u64>(), 7);
        assert_eq!(result.fees_per_hour[23], u64::MAX.to_string());
        assert_eq!(result.latest_timestamp, Some(101 * HOUR));
    }
}
