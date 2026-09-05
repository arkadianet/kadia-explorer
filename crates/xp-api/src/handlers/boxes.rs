use crate::dto::{box_dto_from_reader, parse_id, rent_dto, BoxDto, BoxRentDto};
use crate::{blocking, ApiError, AppState};
use axum::extract::{Path, State};
use axum::Json;
use xp_types::hex32;

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
        Ok(BoxRentDto {
            // Echo the canonical lowercase-hex id, not the caller's path spelling.
            box_id: hex32(&id),
            rent: rent_dto(&row, tip),
        })
    })
    .await?;
    Ok(Json(dto))
}
