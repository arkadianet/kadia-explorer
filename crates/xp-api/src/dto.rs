//! Wire shapes for every route, plus the query-parameter parsing they share.
//!
//! Conventions, applied uniformly:
//! - **Ids** (box/tx/header/token/tree/template ids, `miner_pk`, `ergo_tree`) are lowercase
//!   hex strings.
//! - **Amounts** (`value`, `fee`, `nano`, token `amount`, `due_nano`, `fees`, `reward`) and
//!   `difficulty` are **decimal strings**, not JSON numbers: they exceed the 2^53 range that
//!   JSON consumers can represent exactly.
//! - **Heights, indexes, counts, timestamps and sizes** stay JSON numbers.
//! - **Pages** are `{ "items": [...], "next_cursor": "…" | null }`. `next_cursor` is opaque
//!   to the client: pass it back verbatim as `?cursor=`.
//! - **Query params**: `limit` defaults to 50 and is clamped to 500; `limit=0` or a
//!   non-numeric `limit` is a 400. `dir` is `asc` or `desc` and defaults to `desc`, so lists
//!   are newest-first unless asked otherwise.

use crate::error::ApiError;
use serde::{Deserialize, Serialize};
use xp_store::read::Dir;
use xp_store::rows::{BalanceRow, BoxRow, HeaderRow, TreeRow, TxRow};
use xp_store::Reader;
use xp_types::rent::{maturity_height, rent_due};
use xp_types::{hex32, parse_hex32, Gidx, Hash32};

pub const DEFAULT_LIMIT: usize = 50;
pub const MAX_LIMIT: usize = 500;

/// Cursor/limit/direction, shared by every paged route.
#[derive(Debug, Default, Deserialize)]
pub struct ListParams {
    pub cursor: Option<String>,
    pub limit: Option<String>,
    pub dir: Option<String>,
}

/// `ListParams` plus the unspent-only filter of `/v1/addresses/{addr}/boxes`.
#[derive(Debug, Default, Deserialize)]
pub struct AddrBoxParams {
    pub cursor: Option<String>,
    pub limit: Option<String>,
    pub dir: Option<String>,
    pub unspent: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct RentUpcomingParams {
    pub blocks: Option<String>,
    pub limit: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    pub q: Option<String>,
}

/// `limit` default 50, clamped to [`MAX_LIMIT`]. `0` and non-numeric values are rejected
/// rather than silently coerced, so a client never gets a silently empty page.
pub fn parse_limit(raw: Option<&str>) -> Result<usize, ApiError> {
    match raw {
        None => Ok(DEFAULT_LIMIT),
        Some(s) => {
            let n: usize = s
                .parse()
                .map_err(|_| ApiError::BadRequest(format!("limit must be a number, got {s:?}")))?;
            if n == 0 {
                return Err(ApiError::BadRequest("limit must be >= 1".into()));
            }
            Ok(n.min(MAX_LIMIT))
        }
    }
}

pub fn parse_dir(raw: Option<&str>) -> Result<Dir, ApiError> {
    match raw {
        None | Some("desc") => Ok(Dir::Desc),
        Some("asc") => Ok(Dir::Asc),
        Some(other) => Err(ApiError::BadRequest(format!(
            "dir must be 'asc' or 'desc', got {other:?}"
        ))),
    }
}

/// A plain decimal cursor (a gidx, or a height for `/v1/blocks`).
pub fn parse_u64_cursor(raw: Option<&str>) -> Result<Option<u64>, ApiError> {
    raw.map(|s| {
        s.parse::<u64>()
            .map_err(|_| ApiError::BadRequest(format!("cursor must be a number, got {s:?}")))
    })
    .transpose()
}

pub fn parse_u32_param(raw: Option<&str>, name: &str, default: u32) -> Result<u32, ApiError> {
    match raw {
        None => Ok(default),
        Some(s) => s
            .parse()
            .map_err(|_| ApiError::BadRequest(format!("{name} must be a number, got {s:?}"))),
    }
}

pub fn parse_bool_param(raw: Option<&str>, name: &str) -> Result<bool, ApiError> {
    match raw {
        None => Ok(false),
        Some("true" | "1") => Ok(true),
        Some("false" | "0") => Ok(false),
        Some(other) => Err(ApiError::BadRequest(format!(
            "{name} must be a boolean, got {other:?}"
        ))),
    }
}

pub fn parse_id(raw: &str) -> Result<Hash32, ApiError> {
    parse_hex32(raw)
        .map_err(|_| ApiError::BadRequest(format!("expected 32-byte hex id, got {raw:?}")))
}

/// `"<nano>:<tree hex>"` — the richlist's composite cursor. Inverse of
/// [`parse_rich_cursor`].
pub fn format_rich_cursor(nano: u64, tree: &Hash32) -> String {
    format!("{nano}:{}", hex32(tree))
}

/// `"<maturity height>:<gidx>"` — the rent-eligible cursor. Inverse of
/// [`parse_rent_cursor`].
pub fn format_rent_cursor(height: u32, gidx: Gidx) -> String {
    format!("{height}:{gidx}")
}

/// `"<nano>:<tree hex>"` — the richlist's composite cursor.
pub fn parse_rich_cursor(raw: Option<&str>) -> Result<Option<(u64, Hash32)>, ApiError> {
    let Some(s) = raw else { return Ok(None) };
    let (nano, tree) = s.split_once(':').ok_or_else(|| {
        ApiError::BadRequest(format!("cursor must be '<nano>:<tree>', got {s:?}"))
    })?;
    let nano = nano
        .parse::<u64>()
        .map_err(|_| ApiError::BadRequest(format!("cursor nano must be a number, got {nano:?}")))?;
    Ok(Some((nano, parse_id(tree)?)))
}

/// `"<maturity height>:<gidx>"` — the rent-eligible cursor.
pub fn parse_rent_cursor(raw: Option<&str>) -> Result<Option<(u32, Gidx)>, ApiError> {
    let Some(s) = raw else { return Ok(None) };
    let (h, g) = s.split_once(':').ok_or_else(|| {
        ApiError::BadRequest(format!("cursor must be '<height>:<gidx>', got {s:?}"))
    })?;
    let h = h
        .parse::<u32>()
        .map_err(|_| ApiError::BadRequest(format!("cursor height must be a number, got {h:?}")))?;
    let g = g
        .parse::<Gidx>()
        .map_err(|_| ApiError::BadRequest(format!("cursor gidx must be a number, got {g:?}")))?;
    Ok(Some((h, g)))
}

#[derive(Debug, Serialize)]
pub struct PageDto<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StatusDto {
    pub indexed: Option<u32>,
    pub best: u32,
    pub mode: &'static str,
    pub source: String,
    pub halted: Option<String>,
    pub lag_blocks: u32,
}

#[derive(Debug, Serialize)]
pub struct BlockDto {
    pub id: String,
    pub height: u32,
    pub parent_id: String,
    pub timestamp: u64,
    pub difficulty: String,
    pub miner_pk: String,
    pub tx_count: u32,
    pub size: u32,
    pub fees: String,
    pub reward: String,
    pub version: u8,
}

pub fn block_dto(height: u32, h: &HeaderRow) -> BlockDto {
    BlockDto {
        id: hex32(&h.id),
        height,
        parent_id: hex32(&h.parent_id),
        timestamp: h.timestamp,
        difficulty: h.difficulty.to_string(),
        miner_pk: hex::encode(h.miner_pk),
        tx_count: h.tx_count,
        size: h.size,
        fees: h.fees.to_string(),
        reward: h.reward.to_string(),
        version: h.version,
    }
}

#[derive(Debug, Serialize)]
pub struct TokenDto {
    pub id: String,
    pub amount: String,
}

fn token_dtos(tokens: &[(Hash32, u64)]) -> Vec<TokenDto> {
    tokens
        .iter()
        .map(|(id, amount)| TokenDto {
            id: hex32(id),
            amount: amount.to_string(),
        })
        .collect()
}

#[derive(Debug, Serialize)]
pub struct RentDto {
    pub maturity_height: u32,
    pub due_nano: String,
    /// True when the box is still unspent and the indexed tip has reached its maturity.
    pub claimable_at_tip: bool,
}

pub fn rent_dto(row: &BoxRow, tip: Option<u32>) -> RentDto {
    let maturity = maturity_height(row.creation_height);
    RentDto {
        maturity_height: maturity,
        due_nano: rent_due(row.size, row.value).to_string(),
        claimable_at_tip: row.spent.is_none() && tip.is_some_and(|t| t >= maturity),
    }
}

#[derive(Debug, Serialize)]
pub struct BoxDto {
    pub id: String,
    pub tx_id: String,
    pub index: u16,
    pub value: String,
    pub creation_height: u32,
    /// Hex of the serialized ergo tree, and its encoded mainnet address / template hash.
    /// All three are `null` only if the tree row is missing, which the store's own
    /// invariants rule out.
    pub ergo_tree: Option<String>,
    pub address: Option<String>,
    pub template_hash: Option<String>,
    pub tree_hash: String,
    pub tokens: Vec<TokenDto>,
    /// The box's registers as stored, re-parsed into JSON (`null` if unparseable).
    pub registers: serde_json::Value,
    pub size: u32,
    pub spent_by: Option<String>,
    pub spent_height: Option<u32>,
    pub rent: RentDto,
}

pub fn box_dto(id: &Hash32, row: &BoxRow, tree: Option<&TreeRow>, tip: Option<u32>) -> BoxDto {
    BoxDto {
        id: hex32(id),
        tx_id: hex32(&row.tx_id),
        index: row.index,
        value: row.value.to_string(),
        creation_height: row.creation_height,
        ergo_tree: tree.map(|t| hex::encode(&t.tree_bytes)),
        address: tree.map(|t| t.address.clone()),
        template_hash: tree.map(|t| hex32(&t.template_hash)),
        tree_hash: hex32(&row.tree_hash),
        tokens: token_dtos(&row.tokens),
        registers: serde_json::from_str(&row.registers_json).unwrap_or(serde_json::Value::Null),
        size: row.size,
        spent_by: row.spent.map(|(tx, _)| hex32(&tx)),
        spent_height: row.spent.map(|(_, h)| h),
        rent: rent_dto(row, tip),
    }
}

/// Resolves the box's tree row before building the DTO.
pub fn box_dto_from_reader(
    rd: &Reader,
    id: &Hash32,
    row: &BoxRow,
    tip: Option<u32>,
) -> Result<BoxDto, ApiError> {
    let tree = rd.tree_row(&row.tree_hash)?;
    Ok(box_dto(id, row, tree.as_ref(), tip))
}

/// A transaction input: its box id always, and the resolved box when the store holds it (a
/// partial index, seeded above genesis, can legitimately not have the spent box's row).
#[derive(Debug, Serialize)]
pub struct InputDto {
    pub id: String,
    #[serde(rename = "box")]
    pub box_: Option<BoxDto>,
}

#[derive(Debug, Serialize)]
pub struct TxDto {
    pub id: String,
    pub height: u32,
    pub index: u16,
    pub timestamp: u64,
    pub size: u32,
    pub fee: String,
    pub inputs: Vec<InputDto>,
    pub data_inputs: Vec<String>,
    pub outputs: Vec<BoxDto>,
}

pub fn tx_dto(rd: &Reader, id: &Hash32, row: &TxRow, tip: Option<u32>) -> Result<TxDto, ApiError> {
    let mut inputs = Vec::with_capacity(row.inputs.len());
    for input_id in &row.inputs {
        let resolved = match rd.box_by_id(input_id)? {
            Some(b) => Some(box_dto_from_reader(rd, input_id, &b, tip)?),
            None => None,
        };
        inputs.push(InputDto {
            id: hex32(input_id),
            box_: resolved,
        });
    }
    let mut outputs = Vec::with_capacity(row.output_count as usize);
    for (box_id, box_row) in rd.boxes_of_tx(row, id)? {
        outputs.push(box_dto_from_reader(rd, &box_id, &box_row, tip)?);
    }
    Ok(TxDto {
        id: hex32(id),
        height: row.height,
        index: row.index,
        timestamp: row.timestamp,
        size: row.size,
        fee: row.fee.to_string(),
        inputs,
        data_inputs: row.data_inputs.iter().map(hex32).collect(),
        outputs,
    })
}

#[derive(Debug, Serialize)]
pub struct BalanceDto {
    pub nano: String,
    pub tokens: Vec<TokenDto>,
}

#[derive(Debug, Serialize)]
pub struct AddressDto {
    pub address: String,
    pub tree_hash: String,
    pub balance: BalanceDto,
    pub box_count: u64,
    pub first_seen: u32,
    pub last_seen: u32,
}

pub fn address_dto(address: String, tree: &Hash32, bal: Option<&BalanceRow>) -> AddressDto {
    AddressDto {
        address,
        tree_hash: hex32(tree),
        balance: BalanceDto {
            nano: bal.map(|b| b.nano).unwrap_or(0).to_string(),
            tokens: bal.map(|b| token_dtos(&b.tokens)).unwrap_or_default(),
        },
        box_count: bal.map(|b| b.box_count).unwrap_or(0),
        first_seen: bal.map(|b| b.first_seen).unwrap_or(0),
        last_seen: bal.map(|b| b.last_seen).unwrap_or(0),
    }
}

/// `/v1/boxes/{id}/rent`: the box id plus the flattened [`RentDto`], so the JSON is
/// `{ box_id, maturity_height, due_nano, claimable_at_tip }`.
#[derive(Debug, Serialize)]
pub struct BoxRentDto {
    pub box_id: String,
    #[serde(flatten)]
    pub rent: RentDto,
}

/// `/v1/addresses/{addr}/rent`: not a cursor page — the whole unspent set is scanned (up to
/// [`crate::handlers::addresses::RENT_SCAN_CAP`] boxes) so it can be sorted by maturity.
#[derive(Debug, Serialize)]
pub struct AddressRentDto {
    pub items: Vec<BoxDto>,
    /// True when the address holds more unspent boxes than the scan cap, so the list is a
    /// prefix of the tree's boxes rather than every one of them.
    pub truncated: bool,
}

#[derive(Debug, Serialize)]
pub struct RentItemDto {
    pub maturity_height: u32,
    #[serde(rename = "box")]
    pub box_: BoxDto,
}

#[derive(Debug, Serialize)]
pub struct RichlistItemDto {
    pub address: Option<String>,
    pub tree_hash: String,
    pub nano: String,
}

#[derive(Debug, Serialize)]
pub struct SearchDto {
    pub kind: &'static str,
    pub id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_defaults_clamps_and_rejects() {
        assert_eq!(parse_limit(None).unwrap(), DEFAULT_LIMIT);
        assert_eq!(parse_limit(Some("1")).unwrap(), 1);
        assert_eq!(parse_limit(Some("500")).unwrap(), MAX_LIMIT);
        // Anything above the cap is clamped, never rejected.
        assert_eq!(parse_limit(Some("100000")).unwrap(), MAX_LIMIT);
        assert!(matches!(
            parse_limit(Some("0")),
            Err(ApiError::BadRequest(_))
        ));
        assert!(matches!(
            parse_limit(Some("abc")),
            Err(ApiError::BadRequest(_))
        ));
        assert!(matches!(
            parse_limit(Some("")),
            Err(ApiError::BadRequest(_))
        ));
        assert!(matches!(
            parse_limit(Some("-1")),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn dir_defaults_to_desc_and_rejects_junk() {
        assert!(matches!(parse_dir(None).unwrap(), Dir::Desc));
        assert!(matches!(parse_dir(Some("desc")).unwrap(), Dir::Desc));
        assert!(matches!(parse_dir(Some("asc")).unwrap(), Dir::Asc));
        assert!(matches!(
            parse_dir(Some("sideways")),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn rich_cursor_round_trips_including_extremes() {
        for (nano, tree) in [(0u64, [0u8; 32]), (1, [0x11; 32]), (u64::MAX, [0xff; 32])] {
            let s = format_rich_cursor(nano, &tree);
            assert_eq!(parse_rich_cursor(Some(&s)).unwrap(), Some((nano, tree)));
        }
        assert_eq!(parse_rich_cursor(None).unwrap(), None);
        // A well-formed pair with a bad hash, and a pair with no separator, are both 400s.
        assert!(matches!(
            parse_rich_cursor(Some("12:zz")),
            Err(ApiError::BadRequest(_))
        ));
        assert!(matches!(
            parse_rich_cursor(Some("12")),
            Err(ApiError::BadRequest(_))
        ));
        assert!(matches!(
            parse_rich_cursor(Some("x:00")),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn rent_cursor_round_trips_including_extremes() {
        for (h, g) in [(0u32, 0u64), (1_866_002, 42), (u32::MAX, u64::MAX)] {
            let s = format_rent_cursor(h, g);
            assert_eq!(parse_rent_cursor(Some(&s)).unwrap(), Some((h, g)));
        }
        assert_eq!(parse_rent_cursor(None).unwrap(), None);
        assert!(matches!(
            parse_rent_cursor(Some("100")),
            Err(ApiError::BadRequest(_))
        ));
        assert!(matches!(
            parse_rent_cursor(Some("100:nope")),
            Err(ApiError::BadRequest(_))
        ));
        assert!(matches!(
            parse_rent_cursor(Some("nope:100")),
            Err(ApiError::BadRequest(_))
        ));
    }
}
