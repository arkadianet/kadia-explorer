use crate::dto::{
    enrich_txs, parse_dir, parse_id, parse_limit, parse_u64_cursor, tx_dto, ListParams, PageDto,
    TxDto,
};
use crate::{blocking, ApiError, AppState};
use axum::extract::{Path, Query, State};
use axum::Json;

/// The global tx range by gidx; `desc` (newest first) by default.
pub async fn list(
    State(state): State<AppState>,
    Query(p): Query<ListParams>,
) -> Result<Json<PageDto<TxDto>>, ApiError> {
    let limit = parse_limit(p.limit.as_deref())?;
    let cursor = parse_u64_cursor(p.cursor.as_deref())?;
    let dir = parse_dir(p.dir.as_deref())?;
    let page = blocking(&state, move |rd| {
        let emission = rd.emission_tree_hash()?;
        let tip = rd.indexed_height()?;
        let page = rd.txs_by_gidx(cursor, limit, dir)?;
        let mut items = page
            .items
            .iter()
            .map(|(id, row)| tx_dto(rd, id, row, tip, emission.as_ref()))
            .collect::<Result<Vec<_>, _>>()?;
        enrich_txs(rd, items.iter_mut())?;
        Ok(PageDto {
            items,
            next_cursor: page.next_cursor.map(|c| c.to_string()),
        })
    })
    .await?;
    Ok(Json(page))
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(raw): Path<String>,
) -> Result<Json<TxDto>, ApiError> {
    let dto = blocking(&state, move |rd| {
        let emission = rd.emission_tree_hash()?;
        let id = parse_id(&raw)?;
        let row = rd.tx_by_id(&id)?.ok_or(ApiError::NotFound)?;
        let tip = rd.indexed_height()?;
        let mut dto = tx_dto(rd, &id, &row, tip, emission.as_ref())?;
        enrich_txs(rd, std::iter::once(&mut dto))?;
        Ok(dto)
    })
    .await?;
    Ok(Json(dto))
}
