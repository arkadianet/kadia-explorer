use crate::dto::{
    box_dto_from_reader, format_rent_cursor, parse_limit, parse_rent_cursor, parse_u32_param,
    ListParams, PageDto, RentItemDto, RentUpcomingParams,
};
use crate::{blocking, ApiError, AppState};
use axum::extract::{Query, State};
use axum::Json;
use xp_store::Reader;
use xp_types::Hash32;

/// Default look-ahead window for `/v1/rent/upcoming`, in blocks (~24 h at 2 min/block).
pub const DEFAULT_UPCOMING_BLOCKS: u32 = 720;

fn items_of(
    rd: &Reader,
    rows: Vec<(u32, Hash32)>,
    tip: Option<u32>,
) -> Result<Vec<RentItemDto>, ApiError> {
    let mut out = Vec::with_capacity(rows.len());
    for (maturity_height, box_id) in rows {
        // A `RENT_MATURES` entry always points at a live box row; a missing one is store
        // corruption, not a client-visible condition.
        let row = rd
            .box_by_id(&box_id)?
            .ok_or_else(|| ApiError::Internal("rent index points at a missing box".into()))?;
        out.push(RentItemDto {
            maturity_height,
            box_: box_dto_from_reader(rd, &box_id, &row, tip)?,
        });
    }
    Ok(out)
}

/// Boxes maturing in `[indexed + 1, indexed + 1 + blocks)`, oldest first. Not cursor-paged:
/// the window itself bounds the answer.
pub async fn upcoming(
    State(state): State<AppState>,
    Query(p): Query<RentUpcomingParams>,
) -> Result<Json<PageDto<RentItemDto>>, ApiError> {
    let limit = parse_limit(p.limit.as_deref())?;
    let blocks = parse_u32_param(p.blocks.as_deref(), "blocks", DEFAULT_UPCOMING_BLOCKS)?;
    let page = blocking(&state, move |rd| {
        let tip = rd.indexed_height()?;
        let from = tip.unwrap_or(0).saturating_add(1);
        let rows = rd.rent_matures_range(from, blocks, limit)?;
        Ok(PageDto {
            items: items_of(rd, rows, tip)?,
            next_cursor: None,
        })
    })
    .await?;
    Ok(Json(page))
}

/// Boxes already claimable at the indexed tip, ascending by `(maturity height, gidx)`. The
/// cursor is `"<maturity height>:<gidx>"`.
pub async fn eligible(
    State(state): State<AppState>,
    Query(p): Query<ListParams>,
) -> Result<Json<PageDto<RentItemDto>>, ApiError> {
    let limit = parse_limit(p.limit.as_deref())?;
    let cursor = parse_rent_cursor(p.cursor.as_deref())?;
    let page = blocking(&state, move |rd| {
        let tip = rd.indexed_height()?;
        let (rows, next) = rd.rent_eligible(tip.unwrap_or(0), cursor, limit)?;
        Ok(PageDto {
            items: items_of(rd, rows, tip)?,
            next_cursor: next.map(|(h, g)| format_rent_cursor(h, g)),
        })
    })
    .await?;
    Ok(Json(page))
}
