use crate::dto::{stalled_dto, StatusDto};
use crate::{ApiError, AppState};
use axum::extract::State;
use axum::Json;
use std::sync::atomic::Ordering;
use xp_ingest::Mode;

pub async fn status(State(state): State<AppState>) -> Result<Json<StatusDto>, ApiError> {
    let s = state.status.borrow().clone();
    // `best` can trail `indexed` briefly right after a batch commits, so saturate rather
    // than underflow.
    let lag_blocks = s.best.saturating_sub(s.indexed.unwrap_or(0));
    Ok(Json(StatusDto {
        indexed: s.indexed,
        best: s.best,
        mode: match s.mode {
            Mode::Bulk => "bulk",
            Mode::Tip => "tip",
        },
        source: s.source,
        halted: s.halted,
        lag_blocks,
        stalled: s.stalled.as_ref().map(stalled_dto),
        inflight_reads: state.counters.inflight_reads.load(Ordering::Relaxed),
        rate_limited_total: state.counters.rate_limited_total.load(Ordering::Relaxed),
    }))
}
