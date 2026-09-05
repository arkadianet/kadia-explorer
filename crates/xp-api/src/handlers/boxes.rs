use crate::dto::{box_dto_from_reader, parse_id, BoxDto, BoxRentDto};
use crate::{blocking, ApiError, AppState};
use axum::extract::{Path, State};
use axum::Json;
use xp_types::rent::{maturity_height, rent_due};

pub async fn get_one(
    State(state): State<AppState>,
    Path(raw): Path<String>,
) -> Result<Json<BoxDto>, ApiError> {
    let dto = blocking(&state, move |rd| {
        let id = parse_id(&raw)?;
        let row = rd.box_by_id(&id)?.ok_or(ApiError::NotFound)?;
        let tip = rd.indexed_height()?;
        box_dto_from_reader(rd, &id, &row, tip)
    })
    .await?;
    Ok(Json(dto))
}

pub async fn rent(
    State(state): State<AppState>,
    Path(raw): Path<String>,
) -> Result<Json<BoxRentDto>, ApiError> {
    let dto = blocking(&state, move |rd| {
        let id = parse_id(&raw)?;
        let row = rd.box_by_id(&id)?.ok_or(ApiError::NotFound)?;
        let tip = rd.indexed_height()?;
        let maturity = maturity_height(row.creation_height);
        Ok(BoxRentDto {
            box_id: raw,
            maturity_height: maturity,
            due_nano: rent_due(row.size, row.value).to_string(),
            claimable_at_tip: row.spent.is_none() && tip.is_some_and(|t| t >= maturity),
        })
    })
    .await?;
    Ok(Json(dto))
}
