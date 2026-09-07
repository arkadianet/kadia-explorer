use crate::dto::{
    block_dto, enrich_txs, parse_dir, parse_limit, parse_u64_cursor, tx_dto, BlockDto, ListParams,
    PageDto, TxDto,
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

/// Every tx of the block, in in-block order. Blocks are bounded (a few hundred txs at most),
/// so this is deliberately unpaginated.
pub async fn block_txs(
    State(state): State<AppState>,
    Path(raw): Path<String>,
) -> Result<Json<Vec<TxDto>>, ApiError> {
    let out = blocking(&state, move |rd| {
        let emission = rd.emission_tree_hash()?;
        let height = resolve_height(rd, &raw)?;
        if rd.header_at(height)?.is_none() {
            return Err(ApiError::NotFound);
        }
        let tip = rd.indexed_height()?;
        let mut items = rd
            .txs_in_block(height)?
            .iter()
            .map(|(id, row)| tx_dto(rd, id, row, tip, emission.as_ref()))
            .collect::<Result<Vec<_>, _>>()?;
        enrich_txs(rd, items.iter_mut())?;
        Ok(items)
    })
    .await?;
    Ok(Json(out))
}
