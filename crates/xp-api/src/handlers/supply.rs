//! Circulating-supply figures derived from chain state.
//!
//! The ERG in existence at the indexed tip is what the chain started with minus whatever the
//! emission contract still holds. That is exact at every height and needs no emission schedule,
//! so it stays correct across EIP-27 re-emission and the early foundation share — both of which
//! a hardcoded schedule gets wrong.

use crate::dto::SupplyDto;
use crate::{blocking, ApiError, AppState};
use axum::extract::State;
use axum::Json;
use xp_types::{GENESIS_FOUNDATION_NANO, GENESIS_NO_PREMINE_NANO, GENESIS_TOTAL_NANO};

pub async fn supply(State(state): State<AppState>) -> Result<Json<SupplyDto>, ApiError> {
    let dto = blocking(&state, move |rd| {
        let indexed_height = rd.indexed_height();
        // Without the emission tree (a store seeded from a height above genesis) there is no
        // honest figure to report, so report none rather than a plausible-looking guess.
        let Some(tree) = rd.emission_tree_hash()? else {
            return Ok(SupplyDto {
                indexed_height: indexed_height?,
                emission_remaining_nano: None,
                emitted_nano: None,
                genesis_total_nano: GENESIS_TOTAL_NANO.to_string(),
                complete: false,
            });
        };
        let remaining = rd.balance(&tree)?.map(|b| b.nano).unwrap_or(0);
        Ok(SupplyDto {
            indexed_height: indexed_height?,
            emission_remaining_nano: Some(remaining.to_string()),
            emitted_nano: Some(GENESIS_TOTAL_NANO.saturating_sub(remaining).to_string()),
            genesis_total_nano: GENESIS_TOTAL_NANO.to_string(),
            complete: true,
        })
    })
    .await?;
    Ok(Json(dto))
}

/// Non-emission genesis allocation, exposed for callers that want to net it out.
pub const PREALLOCATED_NANO: u64 = GENESIS_FOUNDATION_NANO + GENESIS_NO_PREMINE_NANO;
