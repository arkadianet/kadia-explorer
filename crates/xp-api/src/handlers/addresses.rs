use crate::budget::Budget;
use crate::dto::{
    address_dto, box_dto_from_reader, checked_tx_summary_dto, enrich_balance, enrich_boxes,
    parse_bool_param, parse_dir, parse_limit, parse_u64_cursor, AddrBoxParams, AddressDto,
    AddressRentDto, BoxDto, ListParams, PageDto, RentDto, TxSummaryDto,
};
use crate::paging::{Binding, Filter, Route};
use crate::{blocking, ApiError, AppState};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use xp_store::{Reader, StoreError};
use xp_types::rent::maturity_height;
use xp_types::{hex32, Hash32};

/// Largest number of unspent boxes `/rent` will scan for one address before reporting
/// `truncated: true`. The route sorts the scanned boxes by maturity, so it cannot stream.
pub const RENT_SCAN_CAP: usize = 5_000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RentParams {
    view: Option<String>,
}

#[derive(Serialize)]
struct RentAnchor {
    height: u32,
    block_id: String,
}

/// Compact projection deliberately omits registers, scripts and token metadata.
#[derive(Serialize)]
struct ExposureBox {
    id: String,
    value: String,
    creation_height: u32,
    size: u32,
    token_count: usize,
    rent: RentDto,
}

#[derive(Serialize)]
struct ExposureContext {
    scope: &'static str,
    address: String,
    tree_hash: String,
    indexed_height: Option<u32>,
    anchor: Option<RentAnchor>,
    full_history: bool,
    partial_from: Option<u32>,
    scanned_count: usize,
    scan_limit: usize,
    scan_complete: bool,
}

#[derive(Serialize)]
struct Exposure {
    items: Vec<ExposureBox>,
    truncated: bool,
    context: ExposureContext,
}

fn exposure_error(error: ApiError) -> ApiError {
    let ApiError::Expansion(code) = error else {
        return error;
    };
    let code = match code {
        "expansion_decode_limit" => "rent_exposure_decode_limit",
        "expansion_response_limit" => "rent_exposure_response_limit",
        "expansion_deadline" => "rent_exposure_deadline",
        _ => "rent_exposure_work_limit",
    };
    ApiError::History {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        code,
        detail: "Rent exposure exceeds this request's resource budget. No exposure total was returned; use the address's paged unspent boxes for individual evidence.".into(),
    }
}

fn exposure(rd: &Reader, address: String) -> Result<Response, ApiError> {
    let tree = tree_of(rd, &address)?;
    let indexed_height = rd.indexed_height()?;
    let full_history = rd.full_history()?;
    let anchor = indexed_height
        .map(|height| {
            Ok::<_, ApiError>(rd.header_id_at(height)?.map(|id| RentAnchor {
                height,
                block_id: hex32(&id),
            }))
        })
        .transpose()?
        .flatten();
    if full_history && indexed_height.is_some_and(|h| h > 0) && anchor.is_none() {
        return Err(ApiError::Integrity("missing canonical rent anchor".into()));
    }
    let budget = RefCell::new(Budget::new());
    let scanned = Cell::new(0usize);
    let mut items = Vec::new();
    let result = rd.visit_history_candidates_admitted::<ApiError>(
        &tree,
        None,
        true,
        |bytes| {
            // Looking up one extra key proves truncation without decoding a 5,001st box.
            if bytes > 0 && scanned.get() == RENT_SCAN_CAP {
                return Err(StoreError::ReadLimit("rent_scan_limit"));
            }
            budget.borrow_mut().admit_tx_row(bytes)
        },
        |id, row| {
            budget
                .borrow_mut()
                .work(1 + row.tokens.len() + row.registers_json.len().div_ceil(128))?;
            if row.spent.is_some() {
                return Err(ApiError::Integrity(
                    "spent box in unspent rent index".into(),
                ));
            }
            items.push(ExposureBox {
                id: hex32(&id),
                value: row.value.to_string(),
                creation_height: row.creation_height,
                size: row.size,
                token_count: row.tokens.len(),
                rent: crate::dto::rent_dto(&row, indexed_height),
            });
            scanned.set(scanned.get() + 1);
            Ok(true)
        },
    );
    let truncated = match result {
        Ok(()) => false,
        Err(ApiError::Expansion("rent_scan_limit")) => true,
        Err(error) => return Err(error),
    };
    items.sort_by(|a, b| (a.rent.maturity_height, &a.id).cmp(&(b.rent.maturity_height, &b.id)));
    let response = Exposure {
        context: ExposureContext {
            scope: "indexed_unspent_boxes",
            address,
            tree_hash: hex32(&tree),
            indexed_height,
            anchor,
            full_history,
            partial_from: rd.partial_from()?,
            scanned_count: items.len(),
            scan_limit: RENT_SCAN_CAP,
            scan_complete: !truncated,
        },
        items,
        truncated,
    };
    budget.into_inner().json(&response)
}

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
        let canonical = rd.required_tree(&tree)?.address;
        let bal = rd
            .balance(&tree)?
            .ok_or_else(|| ApiError::Integrity("missing address balance".into()))?;
        let mut dto = address_dto(canonical, &tree, &bal);
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
        let tree = p.paging.resolve(
            rd,
            Route::AddressBoxes,
            dir.into(),
            p.cursor.as_deref(),
            || tree_of(rd, &addr),
        )?;
        p.paging.read(
            rd,
            Binding::new(
                Route::AddressBoxes,
                dir.into(),
                Filter::Boxes {
                    entity: tree,
                    unspent,
                },
            )?,
            p.cursor.as_deref(),
            |ctx| {
                let rd = ctx.reader();
                let tip = rd.indexed_height()?;
                let page = rd.tree_boxes(&tree, unspent, cursor, limit, dir)?;
                let mut items = page
                    .items
                    .iter()
                    .map(|(id, row)| box_dto_from_reader(rd, id, row, tip, emission.as_ref()))
                    .collect::<Result<Vec<_>, _>>()?;
                enrich_boxes(rd, items.iter_mut())?;
                Ok(PageDto {
                    paging: Default::default(),
                    items,
                    next_cursor: page.next_cursor.map(|c| c.to_string()),
                })
            },
        )
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
        let tree = p.paging.resolve(
            rd,
            Route::AddressSummaries,
            dir.into(),
            p.cursor.as_deref(),
            || tree_of(rd, &addr),
        )?;
        p.paging.read(
            rd,
            Binding::new(Route::AddressSummaries, dir.into(), Filter::Entity(tree))?,
            p.cursor.as_deref(),
            |ctx| {
                let rd = ctx.reader();
                let page = rd.tree_txs_bounded(&tree, cursor, limit, dir, ctx.tx_end()?)?;
                Ok(PageDto {
                    paging: Default::default(),
                    items: page
                        .items
                        .iter()
                        .map(|(id, row)| checked_tx_summary_dto(id, row))
                        .collect::<Result<Vec<_>, _>>()?,
                    next_cursor: page.next_cursor.map(|c| c.to_string()),
                })
            },
        )
    })
    .await?;
    Ok(Json(page))
}

/// Scans at most [`RENT_SCAN_CAP`] of the address's unspent boxes in insertion order,
/// then sorts those boxes by rent maturity, soonest first. When `truncated` is true,
/// unscanned boxes may mature sooner than those returned.
pub async fn rent(
    State(state): State<AppState>,
    Path(addr): Path<String>,
    Query(params): Query<RentParams>,
) -> Result<Response, ApiError> {
    if addr.len() > 4096 {
        return Err(ApiError::BadRequest("address exceeds 4096 bytes".into()));
    }
    if let Some(view) = params.view {
        if view != "exposure" {
            return Err(ApiError::BadRequest(
                "rent view must be exposure or omitted".into(),
            ));
        }
        return blocking(&state, move |rd| exposure(rd, addr))
            .await
            .map_err(exposure_error);
    }
    let dto = blocking(&state, move |rd| {
        let emission = rd.emission_tree_hash()?;
        let tree = tree_of(rd, &addr)?;
        let tip = rd.indexed_height()?;
        let mut rows = Vec::new();
        let mut cursor = None;
        let mut truncated = false;
        loop {
            let page = rd.tree_boxes(&tree, true, cursor, 500, xp_store::read::Dir::Asc)?;
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
    Ok(Json(dto).into_response())
}
