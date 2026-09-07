use crate::dto::{
    address_dto, box_dto_from_reader, enrich_balance, enrich_boxes, parse_bool_param, parse_dir,
    parse_limit, parse_u64_cursor, tx_summary_dto, AddrBoxParams, AddressDto, AddressRentDto,
    BoxDto, ListParams, PageDto, TxSummaryDto,
};
use crate::{blocking, ApiError, AppState};
use axum::extract::{Path, Query, State};
use axum::Json;
use xp_store::read::Dir;
use xp_store::Reader;
use xp_types::rent::maturity_height;
use xp_types::Hash32;

/// Largest number of unspent boxes `/rent` will scan for one address before reporting
/// `truncated: true`. The route has to sort the whole set by maturity, so it cannot stream.
pub const RENT_SCAN_CAP: usize = 5_000;

/// `tree_by_address` returns `None` both for an unparseable address and for one that simply
/// has not been seen on-chain; neither is a client error worth a 400 here, so both are 404.
fn tree_of(rd: &Reader, addr: &str) -> Result<Hash32, ApiError> {
    rd.tree_by_address(addr)?.ok_or(ApiError::NotFound)
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(addr): Path<String>,
) -> Result<Json<AddressDto>, ApiError> {
    let dto = blocking(&state, move |rd| {
        let tree = tree_of(rd, &addr)?;
        // Echo the canonical address the store derived from the ergo tree, not the string
        // from the request path: the two agree for a well-formed request, but a client that
        // reaches the same tree by any other encoding gets back the one canonical form.
        let canonical = rd
            .tree_row(&tree)?
            .map(|row| row.address)
            .unwrap_or_else(|| addr.clone());
        let bal = rd.balance(&tree)?;
        let mut dto = address_dto(canonical, &tree, bal.as_ref());
        enrich_balance(rd, &mut dto.balance)?;
        Ok(dto)
    })
    .await?;
    Ok(Json(dto))
}

pub async fn boxes(
    State(state): State<AppState>,
    Path(addr): Path<String>,
    Query(p): Query<AddrBoxParams>,
) -> Result<Json<PageDto<BoxDto>>, ApiError> {
    let limit = parse_limit(p.limit.as_deref())?;
    let cursor = parse_u64_cursor(p.cursor.as_deref())?;
    let dir = parse_dir(p.dir.as_deref())?;
    let unspent = parse_bool_param(p.unspent.as_deref(), "unspent")?;
    let page = blocking(&state, move |rd| {
        let emission = rd.emission_tree_hash()?;
        let tree = tree_of(rd, &addr)?;
        let tip = rd.indexed_height()?;
        let page = rd.tree_boxes(&tree, unspent, cursor, limit, dir)?;
        let mut items = page
            .items
            .iter()
            .map(|(id, row)| box_dto_from_reader(rd, id, row, tip, emission.as_ref()))
            .collect::<Result<Vec<_>, _>>()?;
        enrich_boxes(rd, items.iter_mut())?;
        Ok(PageDto {
            items,
            next_cursor: page.next_cursor.map(|c| c.to_string()),
        })
    })
    .await?;
    Ok(Json(page))
}

pub async fn txs(
    State(state): State<AppState>,
    Path(addr): Path<String>,
    Query(p): Query<ListParams>,
) -> Result<Json<PageDto<TxSummaryDto>>, ApiError> {
    let limit = parse_limit(p.limit.as_deref())?;
    let cursor = parse_u64_cursor(p.cursor.as_deref())?;
    let dir = parse_dir(p.dir.as_deref())?;
    let page = blocking(&state, move |rd| {
        let tree = tree_of(rd, &addr)?;
        let page = rd.tree_txs(&tree, cursor, limit, dir)?;
        Ok(PageDto {
            items: page
                .items
                .iter()
                .map(|(id, row)| tx_summary_dto(id, row))
                .collect(),
            next_cursor: page.next_cursor.map(|c| c.to_string()),
        })
    })
    .await?;
    Ok(Json(page))
}

/// The address's unspent boxes sorted by rent maturity, soonest first. Scans at most
/// [`RENT_SCAN_CAP`] boxes; beyond that the answer is a prefix of the tree's unspent set and
/// `truncated` is true.
pub async fn rent(
    State(state): State<AppState>,
    Path(addr): Path<String>,
) -> Result<Json<AddressRentDto>, ApiError> {
    let dto = blocking(&state, move |rd| {
        let emission = rd.emission_tree_hash()?;
        let tree = tree_of(rd, &addr)?;
        let tip = rd.indexed_height()?;
        let mut rows = Vec::new();
        let mut cursor = None;
        let mut truncated = false;
        loop {
            let page = rd.tree_boxes(&tree, true, cursor, 500, Dir::Asc)?;
            rows.extend(page.items);
            match page.next_cursor {
                Some(c) if rows.len() < RENT_SCAN_CAP => cursor = Some(c),
                Some(_) => {
                    truncated = true;
                    break;
                }
                None => break,
            }
        }
        rows.sort_by_key(|(_, row)| maturity_height(row.creation_height));
        let mut items = rows
            .iter()
            .map(|(id, row)| box_dto_from_reader(rd, id, row, tip, emission.as_ref()))
            .collect::<Result<Vec<_>, _>>()?;
        enrich_boxes(rd, items.iter_mut())?;
        Ok(AddressRentDto { items, truncated })
    })
    .await?;
    Ok(Json(dto))
}
