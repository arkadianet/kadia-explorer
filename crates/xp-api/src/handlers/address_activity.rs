//! Exact address deltas from retained boxes, with bounded, filter-bound continuation.
use crate::budget::Budget;
use crate::dto::{parse_dir, parse_id, parse_u64_cursor, PageDto};
use crate::paging::{Binding, Filter, Request, Route};
use crate::{blocking, ApiError, AppState};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use xp_store::read::ExpansionRow;
use xp_store::rows::{BoxRow, TxRow};
use xp_store::{Reader, StoreError};
use xp_types::{hex32, Hash32};

pub const SCAN_LIMIT: usize = 200;
const MAX_LIMIT: usize = 100;

#[derive(Default, Deserialize)]
pub struct Params {
    #[serde(flatten)]
    paging: Request,
    cursor: Option<String>,
    limit: Option<String>,
    dir: Option<String>,
    asset: Option<String>,
    direction: Option<String>,
    from_ms: Option<String>,
    to_ms: Option<String>,
}

#[derive(Clone, Copy, Serialize)]
enum Asset {
    All,
    Erg,
    Token(Hash32),
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Direction {
    Received,
    Sent,
    Mixed,
    Neutral,
    Unknown,
}

#[derive(Serialize)]
struct Filters {
    asset: Asset,
    direction: Option<Direction>,
    from_ms: Option<u64>,
    to_ms: Option<u64>,
}

#[derive(Serialize)]
struct Coverage {
    complete: bool,
    resolved_inputs: usize,
    total_inputs: usize,
}

#[derive(Serialize)]
struct TokenDelta {
    id: String,
    name: Option<String>,
    decimals: Option<u8>,
    delta: Option<String>,
}

#[derive(Serialize)]
struct Activity {
    id: String,
    height: u32,
    block_id: String,
    timestamp: u64,
    index: u16,
    fee: String,
    input_count: usize,
    output_count: u16,
    coverage: Coverage,
    erg_delta: Option<String>,
    tokens: Vec<TokenDelta>,
    direction: Direction,
    asset_match: &'static str,
}

#[derive(Serialize)]
struct ActivityPage {
    #[serde(flatten)]
    page: PageDto<Activity>,
    scanned: usize,
    scan_limit_reached: bool,
    partial_from: Option<u32>,
}

fn time(raw: Option<&str>) -> Result<Option<u64>, ApiError> {
    raw.map(|s| {
        // Milliseconds stay exactly representable by browser Date and JSON consumers.
        s.parse::<u64>()
            .ok()
            .filter(|n| *n <= 8_640_000_000_000_000)
            .ok_or_else(|| ApiError::BadRequest("invalid UTC millisecond boundary".into()))
    })
    .transpose()
}

fn filters(p: &Params) -> Result<Filters, ApiError> {
    let asset = match p.asset.as_deref().unwrap_or("all") {
        "all" => Asset::All,
        "erg" => Asset::Erg,
        id => Asset::Token(parse_id(id)?),
    };
    let direction = match p.direction.as_deref().unwrap_or("all") {
        "all" => None,
        "received" => Some(Direction::Received),
        "sent" => Some(Direction::Sent),
        "mixed" => Some(Direction::Mixed),
        "neutral" => Some(Direction::Neutral),
        "unknown" => Some(Direction::Unknown),
        _ => return Err(ApiError::BadRequest("invalid activity direction".into())),
    };
    let from_ms = time(p.from_ms.as_deref())?;
    let to_ms = time(p.to_ms.as_deref())?;
    if from_ms.zip(to_ms).is_some_and(|(a, b)| a >= b) {
        return Err(ApiError::BadRequest("from_ms must precede to_ms".into()));
    }
    Ok(Filters {
        asset,
        direction,
        from_ms,
        to_ms,
    })
}

/// This route uses the shared admission/serialization budget but returns its own stable
/// errors, rather than advising activity consumers to use legacy transaction summaries.
fn activity_error(error: ApiError) -> ApiError {
    let ApiError::Expansion(code) = error else {
        return error;
    };
    let code = match code {
        "expansion_work_limit" => "activity_work_limit",
        "expansion_decode_limit" => "activity_decode_limit",
        "expansion_response_limit" => "activity_response_limit",
        _ => "activity_deadline",
    };
    ApiError::History {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        code,
        detail: "address activity budget exceeded; reduce the page limit or narrow the time range"
            .into(),
    }
}

fn check(budget: &Budget) -> Result<(), StoreError> {
    budget
        .check()
        .map_err(|_| StoreError::ReadLimit("expansion_deadline"))
}

fn box_row(rd: &Reader, budget: &mut Budget, id: &Hash32) -> Result<Option<BoxRow>, ApiError> {
    budget.admit_row(rd, ExpansionRow::Box, id)?;
    let row = rd.box_by_id_checked(id, || check(budget))?;
    if let Some(row) = &row {
        budget.work(row.tokens.len())?;
    }
    Ok(row)
}

fn accumulate(
    row: &BoxRow,
    sign: i128,
    tree: &Hash32,
    erg: &mut i128,
    tokens: &mut BTreeMap<Hash32, i128>,
) {
    if row.tree_hash != *tree {
        return;
    }
    // At most 10,000 admitted box/token entries, each u64: safely below i128.
    *erg += sign * i128::from(row.value);
    for (id, amount) in &row.tokens {
        *tokens.entry(*id).or_default() += sign * i128::from(*amount);
    }
}

fn direction(values: impl IntoIterator<Item = i128>) -> Direction {
    let (mut positive, mut negative) = (false, false);
    for value in values {
        positive |= value > 0;
        negative |= value < 0;
    }
    match (positive, negative) {
        (true, true) => Direction::Mixed,
        (true, false) => Direction::Received,
        (false, true) => Direction::Sent,
        (false, false) => Direction::Neutral,
    }
}

fn project(
    rd: &Reader,
    budget: &mut Budget,
    tree: &Hash32,
    id: Hash32,
    row: &TxRow,
    filter: &Filters,
    partial: bool,
) -> Result<Option<Activity>, ApiError> {
    budget.work(1 + row.inputs.len() + usize::from(row.output_count))?;
    let mut erg = 0i128;
    let mut tokens = BTreeMap::new();
    let mut resolved = 0;
    for input in &row.inputs {
        if let Some(input) = box_row(rd, budget, input)? {
            if input.spent != Some((id, row.height)) {
                return Err(ApiError::Integrity(
                    "activity input spending reference mismatch".into(),
                ));
            }
            resolved += 1;
            accumulate(&input, -1, tree, &mut erg, &mut tokens);
        } else if !partial {
            return Err(ApiError::Integrity("missing activity input".into()));
        }
    }
    for index in 0..row.output_count {
        let output_id = rd.output_id(row, index)?;
        let output = box_row(rd, budget, &output_id)?
            .ok_or_else(|| ApiError::Integrity("missing activity output".into()))?;
        if output.tx_id != id || output.index != index {
            return Err(ApiError::Integrity(
                "activity output transaction/index mismatch".into(),
            ));
        }
        accumulate(&output, 1, tree, &mut erg, &mut tokens);
    }
    let complete = resolved == row.inputs.len();
    let asset_match = match filter.asset {
        Asset::Token(id) if !tokens.contains_key(&id) => {
            if complete {
                return Ok(None);
            }
            "uncertain"
        }
        _ => "definite",
    };
    let direction = if !complete {
        Direction::Unknown
    } else {
        match filter.asset {
            Asset::All => direction(std::iter::once(erg).chain(tokens.values().copied())),
            Asset::Erg => direction([erg]),
            Asset::Token(id) => direction([tokens.get(&id).copied().unwrap_or(0)]),
        }
    };
    if filter.direction.is_some_and(|wanted| wanted != direction) {
        return Ok(None);
    }
    let mut token_deltas = Vec::new();
    for (token, delta) in tokens {
        budget.admit_row(rd, ExpansionRow::Token, &token)?;
        let metadata = rd.token_names_checked(&[token], || check(budget))?.pop();
        let (name, decimals) = metadata
            .map(|(_, name, decimals)| (Some(name), decimals))
            .unwrap_or_default();
        token_deltas.push(TokenDelta {
            id: hex32(&token),
            name,
            decimals,
            delta: complete.then(|| delta.to_string()),
        });
    }
    let block_id = rd
        .header_id_at(row.height)?
        .ok_or_else(|| ApiError::Integrity("missing activity block header".into()))?;
    Ok(Some(Activity {
        id: hex32(&id),
        height: row.height,
        block_id: hex32(&block_id),
        timestamp: row.timestamp,
        index: row.index,
        fee: row.fee.to_string(),
        input_count: row.inputs.len(),
        output_count: row.output_count,
        coverage: Coverage {
            complete,
            resolved_inputs: resolved,
            total_inputs: row.inputs.len(),
        },
        erg_delta: complete.then(|| erg.to_string()),
        tokens: token_deltas,
        direction,
        asset_match,
    }))
}

pub async fn list(
    State(state): State<AppState>,
    Path(addr): Path<String>,
    Query(mut p): Query<Params>,
) -> Result<Response, ApiError> {
    let filter = filters(&p)?;
    let limit = p
        .limit
        .as_deref()
        .unwrap_or("20")
        .parse::<usize>()
        .ok()
        .filter(|n| (1..=MAX_LIMIT).contains(n))
        .ok_or_else(|| ApiError::BadRequest("activity limit must be 1 through 100".into()))?;
    let cursor = parse_u64_cursor(p.cursor.as_deref())?;
    let dir = parse_dir(p.dir.as_deref())?;
    if p.paging
        .consistency
        .as_deref()
        .is_some_and(|value| value != "strict")
    {
        return Err(ApiError::BadRequest(
            "address activity requires strict consistency".into(),
        ));
    }
    p.paging.consistency = Some("strict".into());
    let query_hash = xp_wire::tree::blake2b256(
        &serde_json::to_vec(&filter).map_err(|e| ApiError::Internal(e.to_string()))?,
    );
    blocking(&state, move |rd| {
        let mut budget = Budget::new();
        let partial_from = rd.partial_from()?;
        let tree = p.paging.resolve(
            rd,
            Route::AddressActivity,
            dir.into(),
            p.cursor.as_deref(),
            || rd.tree_by_address(&addr)?.ok_or(ApiError::NotFound),
        )?;
        let mut scanned = 0;
        let mut scan_limit_reached = false;
        let page = p.paging.read(
            rd,
            Binding::new(
                Route::AddressActivity,
                dir.into(),
                Filter::Activity {
                    entity: tree,
                    query_hash,
                },
            )?,
            p.cursor.as_deref(),
            |ctx| {
                let mut items = Vec::new();
                let mut next = cursor;
                let end = ctx.tx_end()?;
                loop {
                    budget.work(1)?;
                    let page = rd.tree_txs_bounded_admitted(&tree, next, 1, dir, end, |bytes| {
                        budget.admit_tx_row(bytes)
                    })?;
                    let Some((id, row)) = page.items.into_iter().next() else {
                        next = None;
                        break;
                    };
                    scanned += 1;
                    next = Some(row.gidx);
                    if filter.from_ms.is_none_or(|from| row.timestamp >= from)
                        && filter.to_ms.is_none_or(|to| row.timestamp < to)
                    {
                        if let Some(item) = project(
                            rd,
                            &mut budget,
                            &tree,
                            id,
                            &row,
                            &filter,
                            partial_from.is_some(),
                        )? {
                            items.push(item);
                        }
                    }
                    if items.len() >= limit || scanned >= SCAN_LIMIT {
                        scan_limit_reached = scanned >= SCAN_LIMIT;
                        break;
                    }
                }
                Ok(PageDto {
                    items,
                    next_cursor: next.map(|c| c.to_string()),
                    paging: Default::default(),
                })
            },
        )?;
        budget.json(&ActivityPage {
            page,
            scanned,
            scan_limit_reached,
            partial_from,
        })
    })
    .await
    .map_err(activity_error)
}
