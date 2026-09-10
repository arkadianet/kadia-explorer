//! End-of-block ownership from retained box lifetimes. All reads share one core snapshot.
use crate::dto::parse_limit;
use crate::{history_blocking, ApiError, AppState};
use axum::extract::{rejection::QueryRejection, Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};
use xp_store::{rows::BoxRow, Reader};
use xp_types::Hash32;

const CANDIDATE_CAP: usize = 100_000;
const SCALAR_DEADLINE: Duration = Duration::from_secs(4);
const PAGE_CAP: usize = 1_000;
const TOKEN_CAP: usize = 100_000;
const RESPONSE_CAP: usize = 2 * 1024 * 1024;
const DEADLINE: Duration = Duration::from_millis(250);

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Params {
    height: Option<String>,
    block_id: Option<String>,
    cursor: Option<String>,
    limit: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Anchor {
    height: u32,
    block_id: Option<String>,
}
#[derive(Serialize)]
pub struct Token {
    token_id: String,
    amount: String,
}
#[derive(Serialize)]
pub struct Balance {
    nano: String,
    box_count: u64,
    tokens: Vec<Token>,
}
#[derive(Serialize)]
pub struct BalanceAt {
    address: String,
    at: Anchor,
    indexed_height: Option<u32>,
    balance: Balance,
    complete: bool,
}
#[derive(Serialize)]
pub struct BoxAt {
    box_id: String,
    inclusion_height: u32,
    nano: String,
    tokens: Vec<Token>,
}
#[derive(Serialize)]
pub struct BoxesAt {
    at: Anchor,
    items: Vec<BoxAt>,
    next_cursor: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    version: u8,
    route: String,
    tree: Hash32,
    at: Anchor,
    last: u64,
}

fn problem(status: StatusCode, code: &'static str, detail: &str) -> ApiError {
    ApiError::History {
        status,
        code,
        detail: detail.into(),
    }
}
fn invalid(detail: &str) -> ApiError {
    problem(StatusCode::BAD_REQUEST, "invalid_history_query", detail)
}
fn changed() -> ApiError {
    problem(
        StatusCode::CONFLICT,
        "snapshot_changed",
        "The requested anchor changed or is unavailable; restart pagination.",
    )
}
fn exhausted() -> ApiError {
    problem(
        StatusCode::UNPROCESSABLE_ENTITY,
        "history_scan_limit",
        "Historical balance exceeds the synchronous work budget; sum the anchored box pages.",
    )
}
fn too_large() -> ApiError {
    problem(
        StatusCode::UNPROCESSABLE_ENTITY,
        "history_response_limit",
        "A historical result exceeds the token or response budget.",
    )
}
fn integrity(detail: &str) -> ApiError {
    ApiError::Internal(detail.into())
}
fn checked_add(a: u128, b: u128) -> Result<u128, ApiError> {
    a.checked_add(b)
        .ok_or_else(|| integrity("historical amount overflow"))
}
fn tokens(items: impl IntoIterator<Item = (Hash32, u128)>) -> Vec<Token> {
    let mut items: Vec<_> = items.into_iter().collect();
    items.sort_unstable_by_key(|(id, _)| *id);
    items
        .into_iter()
        .map(|(id, amount)| Token {
            token_id: hex::encode(id),
            amount: amount.to_string(),
        })
        .collect()
}

fn hash(text: &str) -> Result<Hash32, ApiError> {
    if text.len() != 64 {
        return Err(invalid("block_id must be a 32-byte hex id"));
    }
    hex::decode(text)
        .map_err(|_| invalid("invalid block_id"))?
        .try_into()
        .map_err(|_| invalid("invalid block_id"))
}
fn query(p: Result<Query<Params>, QueryRejection>) -> Result<Params, ApiError> {
    p.map(|Query(p)| p).map_err(|_| invalid("Use height (u32), optional block_id, limit and box-page cursor; timestamp is unsupported."))
}

struct Request {
    tree: Hash32,
    height: u32,
    block: Option<Hash32>,
    cursor: Option<Cursor>,
    limit: usize,
}
impl Request {
    fn parse(addr: &str, p: Params, paged: bool) -> Result<Self, ApiError> {
        // Bound Base58 decoding even when invoked independently of the HTTP URI limit.
        if addr.len() > 4096 {
            return Err(invalid("address too long"));
        }
        let tree = xp_wire::tree::address_tree_hash(addr)
            .map_err(|_| invalid("invalid mainnet address"))?
            .0;
        let h = p
            .height
            .as_deref()
            .ok_or_else(|| invalid("height is required"))?;
        if h.is_empty() || !h.bytes().all(|c| c.is_ascii_digit()) {
            return Err(invalid("height must be u32"));
        }
        let height = h.parse().map_err(|_| invalid("height must be u32"))?;
        let limit = parse_limit(p.limit.as_deref()).map_err(|_| invalid("invalid limit"))?;
        let block = p.block_id.as_deref().map(hash).transpose()?;
        let cursor = p
            .cursor
            .map(|c| {
                if !paged || c.len() > 1024 {
                    return Err(invalid("invalid cursor"));
                }
                let bytes = URL_SAFE_NO_PAD
                    .decode(&c)
                    .map_err(|_| invalid("invalid cursor"))?;
                let cursor: Cursor =
                    serde_json::from_slice(&bytes).map_err(|_| invalid("invalid cursor"))?;
                if cursor.version != 1
                    || cursor.route != "address_boxes_at"
                    || cursor.tree != tree
                    || cursor.at.height != height
                {
                    return Err(invalid("cursor does not match request"));
                }
                if (height == 0) != cursor.at.block_id.is_none() {
                    return Err(invalid("invalid cursor anchor"));
                }
                let cursor_block = cursor.at.block_id.as_deref().map(hash).transpose()?;
                if block.is_some() && cursor_block != block {
                    return Err(invalid("block_id does not match cursor"));
                }
                Ok(cursor)
            })
            .transpose()?;
        Ok(Self {
            tree,
            height,
            block,
            cursor,
            limit,
        })
    }

    fn anchor(&self, rd: &Reader) -> Result<(Anchor, Option<u32>), ApiError> {
        if !rd.mainnet_genesis()? {
            return Err(problem(
                StatusCode::SERVICE_UNAVAILABLE,
                "history_unavailable",
                "Exact history requires a full mainnet genesis-seeded store.",
            ));
        }
        let tip = rd.indexed_height()?;
        if self.height > tip.unwrap_or(0) {
            return Err(if self.cursor.is_some() {
                changed()
            } else {
                problem(
                    StatusCode::BAD_REQUEST,
                    "height_not_indexed",
                    "Requested height is above the indexed tip.",
                )
            });
        }
        let block_id = if self.height == 0 {
            None
        } else {
            Some(hex::encode(
                rd.header_at(self.height)?.ok_or_else(changed)?.id,
            ))
        };
        let at = Anchor {
            height: self.height,
            block_id,
        };
        if self
            .block
            .is_some_and(|b| at.block_id.as_deref() != Some(hex::encode(b).as_str()))
            || self.cursor.as_ref().is_some_and(|c| c.at != at)
        {
            return Err(changed());
        }
        Ok((at, tip))
    }
}

struct Scan {
    start: Instant,
    deadline: Duration,
    creators: HashMap<Hash32, u32>,
}
impl Scan {
    fn new() -> Self {
        Self {
            start: Instant::now(),
            deadline: DEADLINE,
            creators: HashMap::new(),
        }
    }
    fn check(&self) -> Result<(), ApiError> {
        if self.start.elapsed() >= self.deadline {
            Err(exhausted())
        } else {
            Ok(())
        }
    }
    fn page_full(&self, examined: usize) -> bool {
        examined == PAGE_CAP || (examined > 0 && self.start.elapsed() >= self.deadline)
    }
    fn birth(&mut self, rd: &Reader, id: &Hash32, row: &BoxRow) -> Result<u32, ApiError> {
        if row.tx_id == [0; 32] {
            if xp_store::read::MAINNET_GENESIS
                .iter()
                .any(|(g, _)| *g == hex::encode(id))
            {
                return Ok(0);
            }
            return Err(integrity("unknown box with zero creator id"));
        }
        if let Some(h) = self.creators.get(&row.tx_id) {
            return Ok(*h);
        }
        let tx = rd
            .tx_by_id(&row.tx_id)?
            .ok_or_else(|| integrity("missing historical creator"))?;
        self.creators.insert(row.tx_id, tx.height);
        Ok(tx.height)
    }
}

fn bounded<T: Serialize>(value: T) -> Result<T, ApiError> {
    let size = serde_json::to_vec(&value)
        .map_err(|_| integrity("serialize history"))?
        .len();
    if size > RESPONSE_CAP {
        Err(too_large())
    } else {
        Ok(value)
    }
}

pub async fn balance(
    State(state): State<AppState>,
    Path(addr): Path<String>,
    p: Result<Query<Params>, QueryRejection>,
) -> Result<Json<BalanceAt>, ApiError> {
    let request = Request::parse(&addr, query(p)?, false)?;
    Ok(Json(
        history_blocking(&state, move |rd| {
            let mut scan = Scan::new();
            scan.deadline = SCALAR_DEADLINE;
            let (at, indexed_height) = request.anchor(rd)?;
            let mut nano = 0u128;
            let mut count = 0u64;
            let mut amounts = BTreeMap::<Hash32, u128>::new();
            let mut token_count = 0;
            if request.height == indexed_height.unwrap_or(0) {
                if let Some(b) = rd.balance(&request.tree)? {
                    if b.tokens.len() > TOKEN_CAP {
                        return Err(exhausted());
                    }
                    nano = b.nano.into();
                    count = b.box_count;
                    for (id, amount) in b.tokens {
                        scan.check()?;
                        let entry = amounts.entry(id).or_default();
                        *entry = checked_add(*entry, amount.into())?;
                    }
                } else if rd.tree_row(&request.tree)?.is_some() {
                    return Err(integrity("missing historical tip balance"));
                }
            } else {
                let mut examined = 0;
                rd.visit_history_candidates(
                    &request.tree,
                    None,
                    false,
                    |id, row| -> Result<bool, ApiError> {
                        scan.check()?;
                        if examined == CANDIDATE_CAP {
                            return Err(exhausted());
                        }
                        examined += 1;
                        // A box spent by H cannot contribute, regardless of its birth.
                        if row.spent.is_some_and(|(_, h)| h <= request.height) {
                            return Ok(true);
                        }
                        let birth = scan.birth(rd, &id, &row)?;
                        if birth > request.height {
                            return Ok(true);
                        }
                        if token_count + row.tokens.len() > TOKEN_CAP {
                            return Err(exhausted());
                        }
                        token_count += row.tokens.len();
                        nano = checked_add(nano, row.value.into())?;
                        count = count
                            .checked_add(1)
                            .ok_or_else(|| integrity("historical box count overflow"))?;
                        for (id, amount) in row.tokens {
                            scan.check()?;
                            let entry = amounts.entry(id).or_default();
                            *entry = checked_add(*entry, amount.into())?;
                        }
                        // Each serialized token needs at least 90 bytes; stop allocating early.
                        if amounts.len() > RESPONSE_CAP / 90 {
                            return Err(too_large());
                        }
                        Ok(true)
                    },
                )?;
            }
            scan.check()?;
            bounded(BalanceAt {
                address: addr,
                at,
                indexed_height,
                balance: Balance {
                    nano: nano.to_string(),
                    box_count: count,
                    tokens: tokens(amounts),
                },
                complete: true,
            })
        })
        .await?,
    ))
}

pub async fn boxes(
    State(state): State<AppState>,
    Path(addr): Path<String>,
    p: Result<Query<Params>, QueryRejection>,
) -> Result<Json<BoxesAt>, ApiError> {
    let request = Request::parse(&addr, query(p)?, true)?;
    Ok(Json(
        history_blocking(&state, move |rd| {
            let mut scan = Scan::new();
            let (at, indexed_height) = request.anchor(rd)?;
            let mut last = request.cursor.as_ref().map(|c| c.last);
            let mut more = false;
            let mut examined = 0;
            let mut token_count = 0;
            let mut size = 2048; // envelope and cursor headroom
            let mut items = Vec::new();
            rd.visit_history_candidates(
                &request.tree,
                last,
                request.height == indexed_height.unwrap_or(0),
                |id, row| -> Result<bool, ApiError> {
                    // Always consume at least one candidate. A deadline yields a page,
                    // not an error that makes repeating the same cursor futile.
                    if scan.page_full(examined) || items.len() == request.limit {
                        more = true;
                        return Ok(false);
                    }
                    if row.spent.is_some_and(|(_, h)| h <= request.height) {
                        examined += 1;
                        last = Some(row.gidx);
                        return Ok(true);
                    }
                    let birth = scan.birth(rd, &id, &row)?;
                    if birth > request.height {
                        examined += 1;
                        last = Some(row.gidx);
                        return Ok(true);
                    }
                    if row.tokens.len() > TOKEN_CAP {
                        return Err(too_large());
                    }
                    if token_count + row.tokens.len() > TOKEN_CAP {
                        more = true;
                        return Ok(false);
                    }
                    if row.spent.is_none_or(|(_, h)| h > request.height) {
                        let item = BoxAt {
                            box_id: hex::encode(id),
                            inclusion_height: birth,
                            nano: row.value.to_string(),
                            tokens: tokens(row.tokens.iter().map(|(id, n)| (*id, u128::from(*n)))),
                        };
                        let bytes = serde_json::to_vec(&item)
                            .map_err(|_| integrity("serialize historical box"))?
                            .len()
                            + 1;
                        if bytes + 2048 > RESPONSE_CAP {
                            return Err(too_large());
                        }
                        if size + bytes > RESPONSE_CAP {
                            more = true;
                            return Ok(false);
                        }
                        size += bytes;
                        items.push(item);
                    }
                    token_count += row.tokens.len();
                    examined += 1;
                    last = Some(row.gidx);
                    Ok(true)
                },
            )?;
            let next_cursor = if more {
                Some(
                    URL_SAFE_NO_PAD.encode(
                        serde_json::to_vec(&Cursor {
                            version: 1,
                            route: "address_boxes_at".into(),
                            tree: request.tree,
                            at: at.clone(),
                            last: last.ok_or_else(too_large)?,
                        })
                        .map_err(|_| integrity("serialize cursor"))?,
                    ),
                )
            } else {
                None
            };
            bounded(BoxesAt {
                at,
                items,
                next_cursor,
            })
        })
        .await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address() -> String {
        xp_wire::tree_info(&[0, 8, 0xd3]).unwrap().address
    }

    #[test]
    fn cursor_validation_binds_route_filter_anchor_and_width() {
        let tree = xp_wire::tree::address_tree_hash(&address()).unwrap().0;
        let good = Cursor {
            version: 1,
            route: "address_boxes_at".into(),
            tree,
            at: Anchor {
                height: 4,
                block_id: Some(hex::encode([7; 32])),
            },
            last: 8,
        };
        let raw = serde_json::to_value(good).unwrap();
        for (field, value) in [
            ("version", serde_json::json!(2)),
            ("route", serde_json::json!("rent")),
            ("tree", serde_json::json!([1, 2])),
            ("last", serde_json::json!(-1)),
            ("extra", serde_json::json!(1)),
        ] {
            let mut bad = raw.clone();
            bad[field] = value;
            let cursor = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&bad).unwrap());
            assert!(Request::parse(
                &address(),
                Params {
                    height: Some("4".into()),
                    cursor: Some(cursor),
                    ..Params::default()
                },
                true
            )
            .is_err());
        }
        let cursor = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&raw).unwrap());
        assert!(Request::parse(
            &address(),
            Params {
                height: Some("4".into()),
                cursor: Some(cursor.clone()),
                ..Params::default()
            },
            true
        )
        .is_ok());
        assert!(Request::parse(
            &address(),
            Params {
                height: Some("5".into()),
                cursor: Some(cursor.clone()),
                ..Params::default()
            },
            true
        )
        .is_err());
        assert!(Request::parse(
            &address(),
            Params {
                height: Some("4".into()),
                cursor: Some(cursor),
                ..Params::default()
            },
            false
        )
        .is_err());
        for c in ["!".to_owned(), "a".repeat(1025)] {
            assert!(Request::parse(
                &address(),
                Params {
                    height: Some("4".into()),
                    cursor: Some(c),
                    ..Params::default()
                },
                true
            )
            .is_err());
        }
    }

    #[test]
    fn cooperative_deadline_overflow_and_output_bounds_fail_closed() {
        let mut scan = Scan::new();
        scan.start = Instant::now() - DEADLINE;
        assert!(matches!(
            scan.check(),
            Err(ApiError::History {
                code: "history_scan_limit",
                ..
            })
        ));
        assert!(
            !scan.page_full(0),
            "even an expired page must consume a candidate"
        );
        assert!(
            scan.page_full(1),
            "an expired page yields after making progress"
        );
        assert!(checked_add(u128::MAX, 1).is_err());
        assert!(matches!(
            bounded("x".repeat(RESPONSE_CAP)),
            Err(ApiError::History {
                code: "history_response_limit",
                ..
            })
        ));
    }
}
