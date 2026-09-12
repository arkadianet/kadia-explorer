use crate::dto::{
    block_dto, checked_tx_summary_dto, parse_dir, parse_limit, parse_u64_cursor, BlockDto,
    ListParams, PageDto, TxSummaryDto,
};
use crate::{blocking, ApiError, AppState};
use axum::extract::{Path, Query, State};
use axum::Json;
use xp_store::read::Dir;
use xp_store::Reader;

/// Resolves a `{height_or_id}` path segment: all-digits is a height, 64 hex chars is a
/// header id, anything else is a 400.
fn resolve_height(rd: &Reader, raw: &str) -> Result<u32, ApiError> {
    if !raw.is_empty() && raw.bytes().all(|b| b.is_ascii_digit()) {
        return raw
            .parse::<u32>()
            .map_err(|_| ApiError::BadRequest(format!("height out of range: {raw:?}")));
    }
    if raw.len() == 64 {
        let id = crate::dto::parse_id(raw)?;
        return rd.height_of_header(&id)?.ok_or(ApiError::NotFound);
    }
    Err(ApiError::BadRequest(format!(
        "expected a block height or a 64-hex header id, got {raw:?}"
    )))
}

/// Newest first. The cursor is the height of the last item returned; the next page starts
/// strictly below it.
///
/// This route is descending-only: the store's header index is walked backwards from the tip,
/// and an ascending walk would need a different cursor meaning. `dir=asc` is therefore
/// rejected outright rather than silently ignored.
pub async fn list(
    State(state): State<AppState>,
    Query(p): Query<ListParams>,
) -> Result<Json<PageDto<BlockDto>>, ApiError> {
    if let Dir::Asc = parse_dir(p.dir.as_deref())? {
        return Err(ApiError::BadRequest(
            "dir must be desc for /v1/blocks".into(),
        ));
    }
    let limit = parse_limit(p.limit.as_deref())?;
    let cursor = parse_u64_cursor(p.cursor.as_deref())?;
    let before = cursor
        .map(|c| u32::try_from(c).map_err(|_| ApiError::BadRequest("cursor out of range".into())))
        .transpose()?;
    let page = blocking(&state, move |rd| Ok(rd.headers_desc(before, limit)?)).await?;
    let last = page.last().map(|(h, _)| *h);
    let items: Vec<BlockDto> = page.iter().map(|(h, row)| block_dto(*h, row)).collect();
    // Height 0 has no page below it, so it also terminates the walk.
    let next_cursor = match last {
        Some(h) if items.len() == limit && h > 0 => Some(h.to_string()),
        _ => None,
    };
    Ok(Json(PageDto { items, next_cursor }))
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(raw): Path<String>,
) -> Result<Json<BlockDto>, ApiError> {
    let dto = blocking(&state, move |rd| {
        let height = resolve_height(rd, &raw)?;
        let header = rd.header_at(height)?.ok_or(ApiError::NotFound)?;
        Ok(block_dto(height, &header))
    })
    .await?;
    Ok(Json(dto))
}

pub async fn summaries(
    State(state): State<AppState>,
    Path(raw): Path<String>,
    Query(p): Query<ListParams>,
) -> Result<Json<PageDto<TxSummaryDto>>, ApiError> {
    let limit = parse_limit(p.limit.as_deref())?;
    let cursor = parse_u64_cursor(p.cursor.as_deref())?;
    let dir = parse_dir(p.dir.as_deref())?;
    Ok(Json(
        blocking(&state, move |rd| {
            let height = resolve_height(rd, &raw)?;
            rd.header_at(height)?.ok_or(ApiError::NotFound)?;
            let page = rd.txs_in_block_page(height, cursor, limit, dir)?;
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

/// Every tx in block order: either the complete legacy array or an explicit problem.
pub async fn block_txs(
    State(state): State<AppState>,
    Path(raw): Path<String>,
) -> Result<axum::response::Response, ApiError> {
    blocking(&state, move |rd| {
        let mut budget = crate::budget::Budget::new();
        let emission = rd.emission_tree_hash()?;
        let height = resolve_height(rd, &raw)?;
        rd.header_at(height)?.ok_or(ApiError::NotFound)?;
        let tip = rd.indexed_height()?;
        let mut items = Vec::new();
        let mut cursor = None;
        loop {
            budget.check()?;
            let page = rd.txs_in_block_page_admitted(height, cursor, 1, Dir::Asc, |bytes| {
                budget.admit_tx_row(bytes)
            })?;
            for (id, row) in &page.items {
                items.push(budget.tx(rd, id, row, tip, emission.as_ref())?);
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        budget.json(&items)
    })
    .await
}
