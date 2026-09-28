//! Confirmed index facts and independent, explicitly scoped node observations.
use crate::budget::Budget;
use crate::dto::parse_id;
use crate::observations::{
    now_ms, Entry, Inclusion, MempoolObservation, PendingSummary, MAX_INPUTS, RETENTION_SECONDS,
};
use crate::{blocking, ApiError, AppState};
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde::Serialize;
use std::sync::Arc;
use xp_source::BlockSource;
use xp_store::read::ExpansionRow;
use xp_types::{hex32, Hash32};

#[derive(Debug, Serialize)]
struct Conflict {
    input_id: String,
    tx_id: String,
    height: u32,
}
#[derive(Debug)]
struct Indexed {
    checked_at_ms: u64,
    height: Option<u32>,
    inclusion: Option<Inclusion>,
    conflicts: Vec<Conflict>,
    confirmed_inputs: Vec<Hash32>,
}
#[derive(Debug, Serialize)]
struct Status {
    id: String,
    state: &'static str,
    checked_at_ms: u64,
    indexed_height: Option<u32>,
    inclusion: Option<Inclusion>,
    previous_inclusion: Option<Inclusion>,
    mempool: MempoolObservation,
    pending: Option<PendingSummary>,
    conflicts: Vec<Conflict>,
    history_scope: &'static str,
    retention_seconds: u64,
}

async fn indexed(state: &AppState, id: Hash32, inputs: Vec<Hash32>) -> Result<Indexed, ApiError> {
    blocking(state, move |rd| {
        let mut budget = Budget::new();
        let height = rd.indexed_height()?;
        let row = rd.tx_by_id_admitted(&id, |bytes| budget.admit_tx_row(bytes))?;
        // Retain the complete input set only when it fits the observation cache bound.
        // Conflicts are positive findings, never a claim of exhaustive network coverage.
        let confirmed_inputs = row
            .as_ref()
            .filter(|tx| tx.inputs.len() <= MAX_INPUTS)
            .map(|tx| tx.inputs.clone())
            .unwrap_or_default();
        let inclusion = row
            .map(|tx| {
                let block = rd
                    .header_at(tx.height)?
                    .ok_or_else(|| ApiError::Integrity("missing transaction header".into()))?;
                let confirmations = height
                    .and_then(|h| h.checked_sub(tx.height))
                    .and_then(|n| n.checked_add(1))
                    .ok_or_else(|| ApiError::Integrity("transaction beyond indexed tip".into()))?;
                Ok::<_, ApiError>(Inclusion {
                    block_id: hex32(&block.id),
                    height: tx.height,
                    confirmations,
                })
            })
            .transpose()?;
        let mut conflicts = Vec::new();
        if inclusion.is_none() {
            for input in inputs {
                budget.admit_row(rd, ExpansionRow::Box, &input)?;
                if let Some((spender, at)) = rd.box_by_id(&input)?.and_then(|b| b.spent) {
                    if spender != id {
                        conflicts.push(Conflict {
                            input_id: hex32(&input),
                            tx_id: hex32(&spender),
                            height: at,
                        });
                    }
                }
            }
        }
        Ok(Indexed {
            checked_at_ms: now_ms(),
            height,
            inclusion,
            conflicts,
            confirmed_inputs,
        })
    })
    .await
}

fn response(id: Hash32, indexed: Indexed, entry: &Entry) -> Response {
    let state = if indexed.inclusion.is_some() {
        "confirmed"
    } else if !indexed.conflicts.is_empty() {
        "conflicted"
    } else {
        match entry.mempool.observation {
            "present" => "pending",
            "absent" if entry.mempool.first_seen_at_ms.is_some() => "no_longer_observed",
            "absent" => "not_observed",
            _ => "unavailable",
        }
    };
    let previous_inclusion = entry
        .included
        .as_ref()
        .filter(|previous| {
            indexed.inclusion.as_ref().is_none_or(|current| {
                current.block_id != previous.block_id || current.height != previous.height
            })
        })
        .cloned();
    let mut mempool = entry.mempool.clone();
    if indexed.inclusion.is_some() {
        // A past node observation is history, never a simultaneous pending verdict.
        mempool.observation = "not_checked";
        mempool.error = None;
    }
    let status = Status {
        id: hex32(&id),
        state,
        checked_at_ms: indexed.checked_at_ms,
        indexed_height: indexed.height,
        inclusion: indexed.inclusion,
        previous_inclusion,
        mempool,
        pending: entry.pending.clone(),
        conflicts: indexed.conflicts,
        history_scope: "process_local_requested_transactions",
        retention_seconds: RETENTION_SECONDS,
    };
    ([(header::CACHE_CONTROL, "no-store")], Json(status)).into_response()
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(raw): Path<String>,
    source: Option<Extension<Arc<dyn BlockSource>>>,
) -> Result<Response, ApiError> {
    let id = parse_id(&raw)?;
    let Some(cached) = state.counters.observations.entry(id) else {
        let first = indexed(&state, id, vec![]).await?;
        let mut entry = Entry::default();
        entry.mempool = MempoolObservation {
            observation: "unavailable",
            error: Some("busy"),
            ..Default::default()
        };
        return Ok(response(id, first, &entry));
    };
    let mut entry = cached.lock().await;
    let first = indexed(&state, id, vec![]).await?;
    if let Some(inclusion) = first.inclusion.clone() {
        let inputs = first.confirmed_inputs.clone();
        let result = response(id, first, &entry);
        entry.record_inclusion(inclusion, inputs);
        return Ok(result);
    }
    state
        .counters
        .observations
        .observe(id, &mut entry, source.as_ref().map(|s| &s.0))
        .await;
    // Do not hold a Reader across node I/O. A newly indexed transaction always wins.
    let inputs = entry.inputs.clone();
    let current = indexed(&state, id, inputs).await?;
    let inclusion = current.inclusion.clone();
    let inputs = current.confirmed_inputs.clone();
    let result = response(id, current, &entry);
    if let Some(inclusion) = inclusion {
        entry.record_inclusion(inclusion, inputs);
    }
    Ok(result)
}
