//! Gross allocation outside the original emission reserve, explicitly not circulation.
use crate::dto::SupplyDto;
use crate::{blocking, ApiError, AppState};
use axum::extract::State;
use axum::Json;
use xp_types::GENESIS_TOTAL_NANO;

pub async fn supply(State(state): State<AppState>) -> Result<Json<SupplyDto>, ApiError> {
    Ok(Json(
        blocking(&state, move |rd| {
            let mut dto = SupplyDto {
                indexed_height: rd.indexed_height()?,
                emission_remaining_nano: None,
                emitted_nano: None,
                outside_emission_nano: None,
                circulating_nano: None,
                definition: "genesis_allocation_minus_original_emission_reserve",
                genesis_total_nano: GENESIS_TOTAL_NANO.to_string(),
                complete: false,
            };
            if !rd.mainnet_genesis()? {
                return Ok(dto);
            }
            let tree = rd
                .emission_tree_hash()?
                .ok_or_else(|| ApiError::Internal("missing emission tree".into()))?;
            let genesis_id: [u8; 32] = hex::decode(xp_store::read::MAINNET_GENESIS[0].0)
                .expect("static genesis id")
                .try_into()
                .expect("static width");
            let genesis_tree = rd
                .box_by_id(&genesis_id)?
                .ok_or_else(|| ApiError::Internal("missing genesis emission box".into()))?
                .tree_hash;
            if tree != genesis_tree {
                return Err(ApiError::Internal(
                    "incorrect emission tree metadata".into(),
                ));
            }
            // Balances are retained even at zero; a missing row is not exhaustion.
            let remaining = rd
                .balance(&tree)?
                .ok_or_else(|| ApiError::Internal("missing emission balance".into()))?
                .nano;
            let outside = GENESIS_TOTAL_NANO.checked_sub(remaining).ok_or_else(|| {
                ApiError::Internal("emission balance exceeds genesis allocation".into())
            })?;
            dto.emission_remaining_nano = Some(remaining.to_string());
            dto.outside_emission_nano = Some(outside.to_string());
            dto.emitted_nano = dto.outside_emission_nano.clone();
            dto.complete = true;
            Ok(dto)
        })
        .await?,
    ))
}
