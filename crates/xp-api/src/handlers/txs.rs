use crate::budget::Budget;
use crate::dto::{
    checked_tx_summary_dto, parse_dir, parse_id, parse_limit, parse_u64_cursor, ListParams,
    PageDto, TxSummaryDto,
};
use crate::{blocking, ApiError, AppState};
use axum::extract::{Path, Query, State};
use axum::response::Response;
use axum::Json;

pub async fn summaries(
    State(state): State<AppState>,
    Query(p): Query<ListParams>,
) -> Result<Json<PageDto<TxSummaryDto>>, ApiError> {
    let limit = parse_limit(p.limit.as_deref())?;
    let cursor = parse_u64_cursor(p.cursor.as_deref())?;
    let dir = parse_dir(p.dir.as_deref())?;
    Ok(Json(
        blocking(&state, move |rd| {
            let page = rd.txs_by_gidx(cursor, limit, dir)?;
            Ok(PageDto {
                items: page
                    .items
                    .iter()
                    .map(|(id, row)| checked_tx_summary_dto(id, row))
                    .collect::<Result<_, _>>()?,
                next_cursor: page.next_cursor.map(|c| c.to_string()),
            })
        })
        .await?,
    ))
}

/// The global tx range by gidx; `desc` (newest first) by default.
pub async fn list(
    State(state): State<AppState>,
    Query(p): Query<ListParams>,
) -> Result<Response, ApiError> {
    let limit = parse_limit(p.limit.as_deref())?;
    let mut cursor = parse_u64_cursor(p.cursor.as_deref())?;
    let dir = parse_dir(p.dir.as_deref())?;
    blocking(&state, move |rd| {
        let mut budget = Budget::new();
        let emission = rd.emission_tree_hash()?;
        let tip = rd.indexed_height()?;
        let mut items = Vec::new();
        let mut next_cursor = None;
        for _ in 0..limit {
            budget.check()?;
            let page =
                rd.txs_by_gidx_admitted(cursor, 1, dir, |bytes| budget.admit_tx_row(bytes))?;
            let Some((id, row)) = page.items.first() else {
                next_cursor = None;
                break;
            };
            items.push(budget.tx(rd, id, row, tip, emission.as_ref())?);
            cursor = page.next_cursor;
            next_cursor = cursor.map(|c| c.to_string());
            if cursor.is_none() {
                break;
            }
        }
        budget.json(&PageDto { items, next_cursor })
    })
    .await
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(raw): Path<String>,
) -> Result<Response, ApiError> {
    blocking(&state, move |rd| {
        let mut budget = Budget::new();
        let emission = rd.emission_tree_hash()?;
        let id = parse_id(&raw)?;
        let row = rd
            .tx_by_id_admitted(&id, |bytes| budget.admit_tx_row(bytes))?
            .ok_or(ApiError::NotFound)?;
        let tip = rd.indexed_height()?;
        let dto = budget.tx(rd, &id, &row, tip, emission.as_ref())?;
        budget.json(&dto)
    })
    .await
}
