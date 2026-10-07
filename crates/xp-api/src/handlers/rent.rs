use crate::budget::Budget;
use crate::dto::{
    box_dto_from_reader, enrich_boxes, format_rent_cursor, parse_id, parse_limit,
    parse_rent_cursor, parse_u32_param, ListParams, PageDto, RentItemDto, RentUpcomingParams,
    TokenDto,
};
use crate::paging::{Binding, Filter, Order, Request, Route};
use crate::{blocking, ApiError, AppState};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};
use xp_store::read::ExpansionRow;
use xp_store::rows::BoxRow;
use xp_store::{Reader, StoreError};
use xp_types::rent::{consensus_storage_fee, maturity_height};
use xp_types::{hex32, Hash32};

/// Default look-ahead window for `/v1/rent/upcoming`, in blocks (~24 h at 2 min/block).
pub const DEFAULT_UPCOMING_BLOCKS: u32 = 720;

fn items_of(
    rd: &Reader,
    rows: Vec<(u32, Hash32)>,
    tip: Option<u32>,
    emission: Option<&Hash32>,
) -> Result<Vec<RentItemDto>, ApiError> {
    let mut out: Vec<RentItemDto> = Vec::with_capacity(rows.len());
    for (maturity_height, box_id) in rows {
        // A `RENT_MATURES` entry always points at a live box row; a missing one is store
        // corruption, not a client-visible condition.
        let row = rd
            .box_by_id(&box_id)?
            .ok_or_else(|| ApiError::Integrity("rent index points at a missing box".into()))?;
        out.push(RentItemDto {
            maturity_height,
            box_: box_dto_from_reader(rd, &box_id, &row, tip, emission)?,
        });
    }
    enrich_boxes(rd, out.iter_mut().map(|i| &mut i.box_))?;
    Ok(out)
}

#[derive(serde::Serialize)]
pub struct UpcomingDto {
    items: Vec<RentItemDto>,
    next_cursor: Option<String>,
    complete: bool,
    indexed_height: Option<u32>,
}

/// Bounded prefix of boxes maturing in the requested window. `complete` distinguishes
/// exact answers from lower bounds; null cursor does not imply completeness here.
pub async fn upcoming(
    State(state): State<AppState>,
    Query(p): Query<RentUpcomingParams>,
) -> Result<Json<UpcomingDto>, ApiError> {
    let limit = parse_limit(p.limit.as_deref())?;
    let blocks = parse_u32_param(p.blocks.as_deref(), "blocks", DEFAULT_UPCOMING_BLOCKS)?;
    let page = blocking(&state, move |rd| {
        let emission = rd.emission_tree_hash()?;
        let tip = rd.indexed_height()?;
        let from = tip.unwrap_or(0).saturating_add(1);
        let mut rows = rd.rent_matures_range(from, blocks, limit + 1)?;
        let complete = rows.len() <= limit;
        rows.truncate(limit);
        Ok(UpcomingDto {
            complete,
            indexed_height: tip,
            items: items_of(rd, rows, tip, emission.as_ref())?,
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
        p.paging.read(
            rd,
            Binding::new(Route::RentEligible, Order::Asc, Filter::None)?,
            p.cursor.as_deref(),
            |ctx| {
                let rd = ctx.reader();
                let emission = rd.emission_tree_hash()?;
                let tip = rd.indexed_height()?;
                let (rows, next) = rd.rent_eligible(tip.unwrap_or(0), cursor, limit)?;
                Ok(PageDto {
                    paging: Default::default(),
                    items: items_of(rd, rows, tip, emission.as_ref())?,
                    next_cursor: next.map(|(h, g)| format_rent_cursor(h, g)),
                })
            },
        )
    })
    .await?;
    Ok(Json(page))
}

/// Maximum candidate boxes examined by one schedule request, including filter misses.
pub const SCHEDULE_SCAN_LIMIT: usize = 2_000;
const TARGET_BLOCK_SECONDS: u64 = 120;
const MAX_DATE_MS: u64 = 8_640_000_000_000_000;
const BATCH_SCAN_LIMIT: usize = 50_000;
const BATCH_DECODE_LIMIT: usize = 16 * 1024 * 1024;
const BATCH_WORK_LIMIT: usize = 250_000;
// Reserve one second of the shared four-second request deadline for serialization.
const BATCH_DEADLINE: Duration = Duration::from_secs(3);
const REEMISSION_TOKEN: &str = "d9a2cc8a09abfaed87afacfbb7daee79a6b26f10c6613fc13d3f3953e5521d1a";

#[derive(Default, Deserialize)]
pub struct ScheduleParams {
    #[serde(flatten)]
    paging: Request,
    cursor: Option<String>,
    limit: Option<String>,
    window: Option<String>,
    mode: Option<String>,
    token_id: Option<String>,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum ScheduleMode {
    All,
    Collectible,
    FullClaim,
}

#[derive(Serialize)]
struct ScheduleFilter {
    window: &'static str,
    mode: ScheduleMode,
    token_id: Option<Hash32>,
}

#[derive(Serialize)]
struct ScheduleItem {
    box_id: String,
    tx_id: String,
    creation_height: u32,
    maturity_height: u32,
    estimated_maturity_ms: u64,
    value: String,
    consensus_fee_nano: String,
    collectible_due_nano: String,
    collectible: bool,
    full_claim: bool,
    protocol_constrained: bool,
    tokens: Vec<TokenDto>,
}

#[derive(Serialize)]
struct ScheduleContext {
    scope: &'static str,
    anchor_timestamp_ms: u64,
    from_height: u32,
    to_height: u32,
    target_block_seconds: u64,
    window: &'static str,
    mode: ScheduleMode,
    token_id: Option<String>,
    full_history: bool,
    partial_from: Option<u32>,
    scanned: usize,
    scan_limit: usize,
    scan_limit_reached: bool,
    complete: bool,
    batch_scanned: Option<usize>,
    batch_scan_limit: usize,
    batch_complete: Option<bool>,
    batch_stop_reason: Option<&'static str>,
    rent_factor: u64,
    fee_arithmetic: &'static str,
}

#[derive(Serialize)]
struct RentBatch {
    estimated_hour_start_ms: u64,
    box_count: usize,
    collectible_due_nano: String,
    full_claim_count: usize,
    full_claim_value_nano: String,
    selected_token_full_claim_amount: Option<String>,
}

#[derive(Default)]
struct BatchTotals {
    boxes: usize,
    due: u128,
    full_claims: usize,
    full_claim_value: u128,
    selected_token: u128,
}

struct BatchOverview {
    batches: Vec<RentBatch>,
    scanned: usize,
    stop_reason: Option<&'static str>,
}

#[derive(Serialize)]
struct SchedulePage {
    #[serde(flatten)]
    page: PageDto<ScheduleItem>,
    schedule_context: ScheduleContext,
    batches: Option<Vec<RentBatch>>,
}

struct ScheduleFacts {
    fee: i32,
    collectible: bool,
    full_claim: bool,
    protocol_constrained: bool,
    due: u64,
}

fn schedule_facts(row: &BoxRow, reemission: &Hash32) -> ScheduleFacts {
    let fee = consensus_storage_fee(row.size);
    // Mainnet config and verifyReemissionSpending are pinned at Ergo commit
    // 3a6b00d37e3bda2b36447a922606b4ca5a09568f. Re-emission spending obligations
    // conflict with the ordinary rent path; a positive arithmetic fee is insufficient.
    let protocol_constrained = row.tokens.iter().any(|(id, _)| id == reemission);
    let collectible = fee > 0 && !protocol_constrained;
    ScheduleFacts {
        fee,
        collectible,
        full_claim: collectible && (fee as u64) >= row.value && !protocol_constrained,
        protocol_constrained,
        due: if collectible {
            (fee as u64).min(row.value)
        } else {
            0
        },
    }
}

fn schedule_matches(row: &BoxRow, facts: &ScheduleFacts, filter: &ScheduleFilter) -> bool {
    let mode_matches = match filter.mode {
        ScheduleMode::All => true,
        ScheduleMode::Collectible => facts.collectible,
        ScheduleMode::FullClaim => facts.full_claim,
    };
    mode_matches
        && filter
            .token_id
            .is_none_or(|wanted| row.tokens.iter().any(|(id, _)| *id == wanted))
}

fn validate_candidate(
    row: &BoxRow,
    candidate: &xp_store::read::RentCandidate,
) -> Result<(), ApiError> {
    if row.spent.is_some()
        || row.gidx != candidate.gidx
        || maturity_height(row.creation_height) != candidate.maturity_height
    {
        return Err(ApiError::Integrity("rent index membership mismatch".into()));
    }
    Ok(())
}

/// Scan overview independently of the item result limit. Every subtotal represents a
/// complete decoded box; budget exhaustion stops before admitting the next contribution.
#[allow(clippy::too_many_arguments)]
fn schedule_batches(
    rd: &Reader,
    filter: &ScheduleFilter,
    from_height: u32,
    to_height: u32,
    timestamp: u64,
    reemission: &Hash32,
    request_start: Instant,
) -> Result<BatchOverview, ApiError> {
    let mut totals = BTreeMap::<u64, BatchTotals>::new();
    let mut scanned = 0usize;
    let mut decoded = 0usize;
    let mut work = 0usize;
    let mut after = None;
    let mut stop_reason = None;
    'scan: loop {
        if request_start.elapsed() >= BATCH_DEADLINE {
            stop_reason = Some("deadline");
            break;
        }
        let page = rd.rent_candidates(from_height, to_height, after, SCHEDULE_SCAN_LIMIT)?;
        let more = page.more;
        let count = page.items.len();
        for (position, candidate) in page.items.into_iter().enumerate() {
            if request_start.elapsed() >= BATCH_DEADLINE {
                stop_reason = Some("deadline");
                break 'scan;
            }
            let bytes = rd.expansion_row_len(ExpansionRow::Box, &candidate.box_id)?;
            if bytes > BATCH_DECODE_LIMIT - decoded {
                stop_reason = Some("decode_limit");
                break 'scan;
            }
            decoded += bytes;
            let row = match rd.box_by_id_checked(&candidate.box_id, || {
                if request_start.elapsed() >= BATCH_DEADLINE {
                    Err(StoreError::ReadLimit("rent_batch_deadline"))
                } else {
                    Ok(())
                }
            }) {
                Err(StoreError::ReadLimit("rent_batch_deadline")) => {
                    stop_reason = Some("deadline");
                    break 'scan;
                }
                result => result?,
            }
            .ok_or_else(|| ApiError::Integrity("rent index points at a missing box".into()))?;
            let units = 1 + row.tokens.len() + row.registers_json.len().div_ceil(128);
            if units > BATCH_WORK_LIMIT - work {
                stop_reason = Some("work_limit");
                break 'scan;
            }
            work += units;
            validate_candidate(&row, &candidate)?;
            scanned += 1;
            after = Some((candidate.maturity_height, candidate.gidx));
            let facts = schedule_facts(&row, reemission);
            if schedule_matches(&row, &facts, filter) {
                let estimated = timestamp
                    + u64::from(candidate.maturity_height - (from_height - 1))
                        * TARGET_BLOCK_SECONDS
                        * 1_000;
                let total = totals.entry(estimated / 3_600_000 * 3_600_000).or_default();
                total.boxes += 1;
                total.due += u128::from(facts.due);
                if facts.full_claim {
                    total.full_claims += 1;
                    total.full_claim_value += u128::from(row.value);
                    if let Some(wanted) = filter.token_id {
                        for (_, amount) in row.tokens.iter().filter(|(id, _)| *id == wanted) {
                            total.selected_token += u128::from(*amount);
                        }
                    }
                }
            }
            if scanned == BATCH_SCAN_LIMIT {
                if position + 1 < count || more {
                    stop_reason = Some("candidate_limit");
                }
                break 'scan;
            }
        }
        if !more {
            break;
        }
    }
    Ok(BatchOverview {
        scanned,
        stop_reason,
        batches: totals
            .into_iter()
            .map(|(hour, total)| RentBatch {
                estimated_hour_start_ms: hour,
                box_count: total.boxes,
                collectible_due_nano: total.due.to_string(),
                full_claim_count: total.full_claims,
                full_claim_value_nano: total.full_claim_value.to_string(),
                selected_token_full_claim_amount: filter
                    .token_id
                    .map(|_| total.selected_token.to_string()),
            })
            .collect(),
    })
}

fn schedule_error(error: ApiError) -> ApiError {
    let ApiError::Expansion(code) = error else {
        return error;
    };
    let code = match code {
        "expansion_decode_limit" => "rent_schedule_decode_limit",
        "expansion_response_limit" => "rent_schedule_response_limit",
        "expansion_deadline" => "rent_schedule_deadline",
        _ => "rent_schedule_work_limit",
    };
    ApiError::History {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        code,
        detail: "Rent schedule exceeds this request's resource budget. No partial page or totals were returned; reduce the result limit or narrow the window.".into(),
    }
}

fn schedule_unavailable() -> ApiError {
    ApiError::History {
        status: StatusCode::SERVICE_UNAVAILABLE,
        code: "rent_schedule_unavailable",
        detail: "A canonical indexed header is required to estimate this rent window.".into(),
    }
}

fn schedule_check(budget: &Budget) -> Result<(), StoreError> {
    budget
        .check()
        .map_err(|_| StoreError::ReadLimit("expansion_deadline"))
}

/// Upcoming, currently unspent boxes in maturity order. Calendar times are estimates from
/// the indexed header timestamp and 120-second target, never predicted claim transactions.
/// Token and eligibility filters run before the result cap; a scan-limited empty page still
/// carries an exclusive candidate cursor. Strict current-state paging rejects changed tips.
pub async fn schedule(
    State(state): State<AppState>,
    Query(mut p): Query<ScheduleParams>,
) -> Result<Response, ApiError> {
    let (window, blocks) = match p.window.as_deref().unwrap_or("24h") {
        "24h" => ("24h", 720u32),
        "7d" => ("7d", 5_040),
        "30d" => ("30d", 21_600),
        "90d" => ("90d", 64_800),
        _ => {
            return Err(ApiError::BadRequest(
                "window must be 24h, 7d, 30d or 90d".into(),
            ))
        }
    };
    let mode = match p.mode.as_deref().unwrap_or("all") {
        "all" => ScheduleMode::All,
        "collectible" => ScheduleMode::Collectible,
        "full_claim" => ScheduleMode::FullClaim,
        _ => {
            return Err(ApiError::BadRequest(
                "mode must be all, collectible or full_claim".into(),
            ))
        }
    };
    let filter = ScheduleFilter {
        window,
        mode,
        token_id: p.token_id.as_deref().map(parse_id).transpose()?,
    };
    let limit = p
        .limit
        .as_deref()
        .unwrap_or("100")
        .parse::<usize>()
        .ok()
        .filter(|n| (1..=100).contains(n))
        .ok_or_else(|| ApiError::BadRequest("schedule limit must be 1 through 100".into()))?;
    let cursor = parse_rent_cursor(p.cursor.as_deref())?;
    if p.paging
        .consistency
        .as_deref()
        .is_some_and(|value| value != "strict")
    {
        return Err(ApiError::BadRequest(
            "rent schedule requires strict consistency".into(),
        ));
    }
    p.paging.consistency = Some("strict".into());
    let query_hash = xp_wire::tree::blake2b256(
        &serde_json::to_vec(&filter).map_err(|e| ApiError::Internal(e.to_string()))?,
    );
    blocking(&state, move |rd| {
        let request_start = Instant::now();
        let mut budget = Budget::new();
        let reemission = parse_id(REEMISSION_TOKEN)?;
        let height = rd.indexed_height()?.ok_or_else(schedule_unavailable)?;
        // Admit before shared paging also decodes this same current-state header.
        let header = rd
            .header_at_admitted(height, |bytes| budget.admit_tx_row(bytes))?
            .ok_or_else(schedule_unavailable)?;
        let from_height = height.checked_add(1).ok_or_else(schedule_unavailable)?;
        let to_height = height.saturating_add(blocks);
        let projected_end = header
            .timestamp
            .checked_add(u64::from(to_height - height) * TARGET_BLOCK_SECONDS * 1_000)
            .filter(|time| *time <= MAX_DATE_MS);
        if projected_end.is_none() {
            return Err(ApiError::Integrity(
                "rent schedule timestamp is outside the supported range".into(),
            ));
        }
        let mut scanned = 0;
        let mut scan_limit_reached = false;
        let page = p.paging.read(
            rd,
            Binding::new(
                Route::RentSchedule,
                Order::Asc,
                Filter::RentSchedule { query_hash },
            )?,
            p.cursor.as_deref(),
            |ctx| {
                let anchor = ctx.anchor().ok_or_else(schedule_unavailable)?;
                if anchor.height != height || anchor.block_id != header.id {
                    return Err(ApiError::Integrity("rent schedule anchor mismatch".into()));
                }
                if cursor.is_some_and(|(h, _)| h < from_height || h > to_height) {
                    return Err(ApiError::BadRequest(
                        "rent cursor is outside this window".into(),
                    ));
                }
                let candidates =
                    rd.rent_candidates(from_height, to_height, cursor, SCHEDULE_SCAN_LIMIT)?;
                let candidate_count = candidates.items.len();
                let mut items = Vec::new();
                let mut last = None;
                for candidate in candidates.items {
                    budget.admit_row(rd, ExpansionRow::Box, &candidate.box_id)?;
                    let row = rd
                        .box_by_id_checked(&candidate.box_id, || schedule_check(&budget))?
                        .ok_or_else(|| {
                            ApiError::Integrity("rent index points at a missing box".into())
                        })?;
                    budget.work(row.tokens.len() + row.registers_json.len().div_ceil(128))?;
                    validate_candidate(&row, &candidate)?;
                    scanned += 1;
                    last = Some((candidate.maturity_height, candidate.gidx));
                    let facts = schedule_facts(&row, &reemission);
                    if !schedule_matches(&row, &facts, &filter) {
                        continue;
                    }
                    let mut tokens = Vec::with_capacity(row.tokens.len());
                    for (id, amount) in &row.tokens {
                        budget.admit_row(rd, ExpansionRow::Token, id)?;
                        let metadata = rd
                            .token_names_checked(&[*id], || schedule_check(&budget))?
                            .pop();
                        let (name, decimals) = match metadata {
                            Some((_, name, decimals)) => {
                                budget.work(name.len().div_ceil(128))?;
                                (Some(name), decimals)
                            }
                            None => (None, None),
                        };
                        tokens.push(TokenDto {
                            id: hex32(id),
                            amount: amount.to_string(),
                            name,
                            decimals,
                        });
                    }
                    items.push(ScheduleItem {
                        box_id: hex32(&candidate.box_id),
                        tx_id: hex32(&row.tx_id),
                        creation_height: row.creation_height,
                        maturity_height: candidate.maturity_height,
                        estimated_maturity_ms: header.timestamp
                            + u64::from(candidate.maturity_height - height)
                                * TARGET_BLOCK_SECONDS
                                * 1_000,
                        value: row.value.to_string(),
                        consensus_fee_nano: facts.fee.to_string(),
                        collectible_due_nano: facts.due.to_string(),
                        collectible: facts.collectible,
                        full_claim: facts.full_claim,
                        protocol_constrained: facts.protocol_constrained,
                        tokens,
                    });
                    if items.len() == limit {
                        break;
                    }
                }
                let more = scanned < candidate_count || candidates.more;
                scan_limit_reached = more && scanned == SCHEDULE_SCAN_LIMIT;
                Ok(PageDto {
                    items,
                    next_cursor: more
                        .then_some(last)
                        .flatten()
                        .map(|(h, g)| format_rent_cursor(h, g)),
                    paging: Default::default(),
                })
            },
        )?;
        let complete = page.next_cursor.is_none();
        let overview = if cursor.is_none() {
            Some(schedule_batches(
                rd,
                &filter,
                from_height,
                to_height,
                header.timestamp,
                &reemission,
                request_start,
            )?)
        } else {
            None
        };
        budget.json(&SchedulePage {
            page,
            schedule_context: ScheduleContext {
                scope: "indexed_unspent_rent_window",
                anchor_timestamp_ms: header.timestamp,
                from_height,
                to_height,
                target_block_seconds: TARGET_BLOCK_SECONDS,
                window,
                mode,
                token_id: filter.token_id.as_ref().map(hex32),
                full_history: rd.full_history()?,
                partial_from: rd.partial_from()?,
                scanned,
                scan_limit: SCHEDULE_SCAN_LIMIT,
                scan_limit_reached,
                complete,
                batch_scanned: overview.as_ref().map(|v| v.scanned),
                batch_scan_limit: BATCH_SCAN_LIMIT,
                batch_complete: overview.as_ref().map(|v| v.stop_reason.is_none()),
                batch_stop_reason: overview.as_ref().and_then(|v| v.stop_reason),
                rent_factor: xp_types::rent::RENT_PER_BYTE,
                fee_arithmetic: "wrapping_i32",
            },
            batches: overview.map(|v| v.batches),
        })
    })
    .await
    .map_err(schedule_error)
}
