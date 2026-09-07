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
use std::collections::HashMap;
use xp_store::read::Dir;
use xp_store::rows::{BalanceRow, BoxRow, HeaderRow, TemplateRow, TokenRow, TreeRow, TxRow};
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

/// Cursor/limit with no `dir`, for routes whose ordering is fixed by the index they read:
/// `/v1/tokens/{id}/holders` is always largest-balance-first.
#[derive(Debug, Default, Deserialize)]
pub struct CursorParams {
    pub cursor: Option<String>,
    pub limit: Option<String>,
}

/// `/v1/tokens`: cursor/limit plus the `sort` selector (there is no `dir` — both orderings
/// are descending by construction).
#[derive(Debug, Default, Deserialize)]
pub struct TokensListParams {
    pub cursor: Option<String>,
    pub limit: Option<String>,
    pub sort: Option<String>,
}

/// How `/v1/tokens` orders its page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenSort {
    /// Newest mint first, cursored by mint gidx.
    Newest,
    /// Most holders first, cursored by `"<count>:<token id>"`.
    Holders,
}

/// `sort` defaults to `newest`; anything but the two known values is a 400 rather than a
/// silent fallback, so a client never gets an ordering it did not ask for.
pub fn parse_token_sort(raw: Option<&str>) -> Result<TokenSort, ApiError> {
    match raw {
        None | Some("newest") => Ok(TokenSort::Newest),
        Some("holders") => Ok(TokenSort::Holders),
        Some(other) => Err(ApiError::BadRequest(format!(
            "sort must be 'newest' or 'holders', got {other:?}"
        ))),
    }
}

/// `{reg}` of `/v1/registers/{reg}/{valueHex}/boxes`: `R4`..`R9`, case-insensitive. The `R`
/// is required — a bare digit is rejected so the path segment can only ever mean a register.
pub fn parse_register(raw: &str) -> Result<u8, ApiError> {
    let bad = || ApiError::BadRequest(format!("register must be R4..R9, got {raw:?}"));
    let digit = raw.strip_prefix(['R', 'r']).ok_or_else(bad)?;
    match digit.parse::<u8>() {
        Ok(n) if digit.len() == 1 && (4..=9).contains(&n) => Ok(n),
        _ => Err(bad()),
    }
}

/// `{valueHex}` of the register route: the serialised sigma constant, as even-length hex.
/// The handler hashes the decoded bytes exactly as the indexer does.
pub fn parse_register_value(raw: &str) -> Result<Vec<u8>, ApiError> {
    hex::decode(raw).map_err(|_| {
        ApiError::BadRequest(format!(
            "register value must be even-length hex, got {raw:?}"
        ))
    })
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

/// `"<n>:<32-byte hex>"` — the composite cursor shared by every route ordered by a
/// non-unique u64 with a hash as tie-breaker: the richlist (`<nano>:<tree>`), token holders
/// (`<amount>:<tree>`) and `/v1/tokens?sort=holders` (`<count>:<token id>`). Inverse of
/// [`parse_u64_id_cursor`].
pub fn format_u64_id_cursor(n: u64, id: &Hash32) -> String {
    format!("{n}:{}", hex32(id))
}

/// `"<maturity height>:<gidx>"` — the rent-eligible cursor. Inverse of
/// [`parse_rent_cursor`].
pub fn format_rent_cursor(height: u32, gidx: Gidx) -> String {
    format!("{height}:{gidx}")
}

/// `"<n>:<32-byte hex>"` — see [`format_u64_id_cursor`].
pub fn parse_u64_id_cursor(raw: Option<&str>) -> Result<Option<(u64, Hash32)>, ApiError> {
    let Some(s) = raw else { return Ok(None) };
    let (n, id) = s
        .split_once(':')
        .ok_or_else(|| ApiError::BadRequest(format!("cursor must be '<n>:<hex>', got {s:?}")))?;
    let n = n
        .parse::<u64>()
        .map_err(|_| ApiError::BadRequest(format!("cursor count must be a number, got {n:?}")))?;
    Ok(Some((n, parse_id(id)?)))
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
    /// `null` while ingest is progressing (or merely idle at the tip). Non-null means
    /// `indexed` is frozen on a source-side hole that ingest is still retrying — a stall, not
    /// a halt: the process is alive and `halted` stays `null`.
    pub stalled: Option<StalledDto>,
}

#[derive(Debug, Serialize)]
pub struct StalledDto {
    pub height: u32,
    pub since_secs: u64,
    pub reason: String,
}

pub fn stalled_dto(s: &xp_ingest::StalledInfo) -> StalledDto {
    StalledDto {
        height: s.height,
        since_secs: s.since_secs,
        reason: s.reason.clone(),
    }
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

/// A token held by a box or a balance. `name`/`decimals` are filled in by the one-batch
/// [`enrich_boxes`]/[`enrich_txs`]/[`enrich_balance`] pass a handler runs over its finished
/// DTOs; they stay `null` for a token the store has no mint row for (legitimate on a store
/// seeded above the mint height).
#[derive(Debug, Serialize)]
pub struct TokenDto {
    pub id: String,
    pub amount: String,
    pub name: Option<String>,
    pub decimals: Option<u8>,
}

fn token_dtos(tokens: &[(Hash32, u64)]) -> Vec<TokenDto> {
    tokens
        .iter()
        .map(|(id, amount)| TokenDto {
            id: hex32(id),
            amount: amount.to_string(),
            name: None,
            decimals: None,
        })
        .collect()
}

/// Fills `name`/`decimals` on every listed token with **one** `token_names` batch. Ids are
/// re-parsed from the DTOs (they were rendered from `Hash32`s, so they always parse) and
/// looked up through a map, never positionally: `token_names` skips unknown ids and does not
/// deduplicate.
fn fill_token_names(rd: &Reader, tokens: Vec<&mut TokenDto>) -> Result<(), ApiError> {
    if tokens.is_empty() {
        return Ok(());
    }
    let ids = tokens
        .iter()
        .map(|t| parse_id(&t.id))
        .collect::<Result<Vec<Hash32>, _>>()?;
    let known: HashMap<Hash32, (String, Option<u8>)> = rd
        .token_names(&ids)?
        .into_iter()
        .map(|(id, name, decimals)| (id, (name, decimals)))
        .collect();
    for (dto, id) in tokens.into_iter().zip(ids) {
        if let Some((name, decimals)) = known.get(&id) {
            dto.name = Some(name.clone());
            dto.decimals = *decimals;
        }
    }
    Ok(())
}

/// One `token_names` batch over every token of every listed box.
pub fn enrich_boxes<'a>(
    rd: &Reader,
    boxes: impl IntoIterator<Item = &'a mut BoxDto>,
) -> Result<(), ApiError> {
    fill_token_names(
        rd,
        boxes
            .into_iter()
            .flat_map(|b| b.tokens.iter_mut())
            .collect(),
    )
}

/// One `token_names` batch over every token of every listed tx — outputs and resolved inputs
/// alike.
pub fn enrich_txs<'a>(
    rd: &Reader,
    txs: impl IntoIterator<Item = &'a mut TxDto>,
) -> Result<(), ApiError> {
    fill_token_names(
        rd,
        txs.into_iter()
            .flat_map(|t| {
                t.inputs
                    .iter_mut()
                    .filter_map(|i| i.box_.as_mut())
                    .chain(t.outputs.iter_mut())
            })
            .flat_map(|b| b.tokens.iter_mut())
            .collect(),
    )
}

/// One `token_names` batch over an address balance's token list.
pub fn enrich_balance(rd: &Reader, balance: &mut BalanceDto) -> Result<(), ApiError> {
    fill_token_names(rd, balance.tokens.iter_mut().collect())
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
    /// What kind of box this is, for clients that want to label it without knowing any
    /// contract hashes: `"fee"` for the miner-fee contract, `"emission"` for the chain's
    /// emission contract, `"box"` for everything else.
    pub kind: &'static str,
}

/// `emission` is the emission box's tree hash as recorded at genesis seeding
/// (`Reader::emission_tree_hash`); `None` on a store that never seeded genesis, in which case
/// emission boxes are simply labelled `"box"`.
///
/// Handlers resolve it once per request from their own `Reader` rather than snapshotting it
/// into `AppState` at startup: genesis seeding runs inside ingest, so a store created fresh
/// alongside the process is still unseeded when the API boots, and a startup snapshot would
/// stay `None` for the life of the process. It is one META read per request, not per box.
fn box_kind(tree: &Hash32, emission: Option<&Hash32>) -> &'static str {
    if xp_store::is_fee_tree(tree) {
        "fee"
    } else if emission == Some(tree) {
        "emission"
    } else {
        "box"
    }
}

pub fn box_dto(
    id: &Hash32,
    row: &BoxRow,
    tree: Option<&TreeRow>,
    tip: Option<u32>,
    emission: Option<&Hash32>,
) -> BoxDto {
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
        kind: box_kind(&row.tree_hash, emission),
    }
}

/// Resolves the box's tree row before building the DTO.
pub fn box_dto_from_reader(
    rd: &Reader,
    id: &Hash32,
    row: &BoxRow,
    tip: Option<u32>,
    emission: Option<&Hash32>,
) -> Result<BoxDto, ApiError> {
    let tree = rd.tree_row(&row.tree_hash)?;
    Ok(box_dto(id, row, tree.as_ref(), tip, emission))
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

pub fn tx_dto(
    rd: &Reader,
    id: &Hash32,
    row: &TxRow,
    tip: Option<u32>,
    emission: Option<&Hash32>,
) -> Result<TxDto, ApiError> {
    let mut inputs = Vec::with_capacity(row.inputs.len());
    for input_id in &row.inputs {
        let resolved = match rd.box_by_id(input_id)? {
            Some(b) => Some(box_dto_from_reader(rd, input_id, &b, tip, emission)?),
            None => None,
        };
        inputs.push(InputDto {
            id: hex32(input_id),
            box_: resolved,
        });
    }
    let mut outputs = Vec::with_capacity(row.output_count as usize);
    for (box_id, box_row) in rd.boxes_of_tx(row, id)? {
        outputs.push(box_dto_from_reader(rd, &box_id, &box_row, tip, emission)?);
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
    /// Distinct transactions that credited or debited this address.
    pub tx_count: u64,
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
        tx_count: bal.map(|b| b.tx_count).unwrap_or(0),
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

/// EIP-4's R7 `Coll[Byte]` type tag, as stored on [`TokenRow::token_type`] (hex of the
/// constant's payload), mapped to a client-facing label. Anything unrecognised — including
/// an absent R7 — is a plain `"token"`.
pub fn token_kind(token_type: Option<&str>) -> &'static str {
    match token_type {
        Some("0101") => "nft-picture",
        Some("0102") => "nft-audio",
        Some("0103") => "nft-video",
        Some("0201") => "membership",
        _ => "token",
    }
}

/// A holder's share of the circulating supply, as a decimal percentage string.
///
/// A share of 0.01% or more renders with exactly two decimals (`"12.34"`), which is the
/// width the holders column is laid out for. Below that, two decimals would round every
/// dust holder of a large supply to `"0.00"` — a claim that a real holder owns nothing —
/// so a smaller non-zero share renders with up to four decimals, trailing zeros trimmed
/// (`"0.0007"`, `"0.001"`). A share too small even for four decimals is floored to the
/// smallest value this rendering can express rather than to zero.
///
/// `"0"` — with no decimal point at all, so the two cases are distinguishable on the wire —
/// is reserved for a holder of nothing and for a zero supply (every minted unit burned),
/// which has no meaningful share to divide.
pub fn share_pct(amount: u64, supply: u64) -> String {
    if supply == 0 || amount == 0 {
        return "0".to_owned();
    }
    // Ten-thousandths of a percent, in u128: `amount * 1_000_000` overflows u64 for large
    // supplies. 100 units of this is 0.01%, the two-decimal threshold.
    let t = (u128::from(amount) * 1_000_000 / u128::from(supply)).max(1);
    if t >= 100 {
        let bp = t / 100;
        format!("{}.{:02}", bp / 100, bp % 100)
    } else {
        // `t` is in 1..=99, so at least one of the four digits is non-zero and trimming
        // trailing zeros can never leave a bare "0." or collapse to "0".
        format!("0.{t:04}").trim_end_matches('0').to_owned()
    }
}

#[derive(Debug, Serialize)]
pub struct TokenInfoDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub decimals: Option<u8>,
    /// The raw R7 hex as indexed; `kind` is its interpretation.
    pub token_type: Option<String>,
    pub kind: &'static str,
    pub emission: String,
    pub burned: String,
    /// `emission - burned`, saturating: a store seeded above the mint can see burns it never
    /// saw minted.
    pub supply: String,
    pub holder_count: u64,
    pub box_count: u64,
    pub mint_tx: String,
    pub mint_box: String,
    pub mint_height: u32,
}

pub fn token_info_dto(id: &Hash32, row: &TokenRow) -> TokenInfoDto {
    TokenInfoDto {
        id: hex32(id),
        name: row.name.clone(),
        description: row.description.clone(),
        decimals: row.decimals,
        kind: token_kind(row.token_type.as_deref()),
        token_type: row.token_type.clone(),
        emission: row.emission.to_string(),
        burned: row.burned.to_string(),
        supply: row.emission.saturating_sub(row.burned).to_string(),
        holder_count: row.holder_count,
        box_count: row.box_count,
        mint_tx: hex32(&row.mint_tx),
        mint_box: hex32(&row.mint_box),
        mint_height: row.mint_height,
    }
}

#[derive(Debug, Serialize)]
pub struct TokenHolderDto {
    /// The holder's encoded address — mainnet P2PK for a key, P2S for a contract, so a
    /// non-P2PK holder is a long base58 string rather than a `null`. `null` only when the
    /// store has no tree row at all, which its own invariants rule out; the option matches
    /// the richlist's convention rather than describing a reachable state.
    pub address: Option<String>,
    pub tree_hash: String,
    pub amount: String,
    pub share_pct: String,
}

#[derive(Debug, Serialize)]
pub struct TemplateDto {
    pub hash: String,
    pub box_count: u64,
    pub unspent_count: u64,
    pub first_seen: u32,
    /// The address of one tree using this template, as an example of its parameterisation.
    /// Contract templates encode as P2S, so this is normally a long base58 string, not
    /// `null`; `null` means the example tree has no row, which the store's invariants rule
    /// out.
    pub example_address: Option<String>,
}

pub fn template_dto(
    hash: &Hash32,
    row: &TemplateRow,
    example_address: Option<String>,
) -> TemplateDto {
    TemplateDto {
        hash: hex32(hash),
        box_count: row.box_count,
        unspent_count: row.unspent_count,
        first_seen: row.first_seen,
        example_address,
    }
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
    fn u64_id_cursor_round_trips_including_extremes() {
        for (nano, tree) in [(0u64, [0u8; 32]), (1, [0x11; 32]), (u64::MAX, [0xff; 32])] {
            let s = format_u64_id_cursor(nano, &tree);
            assert_eq!(parse_u64_id_cursor(Some(&s)).unwrap(), Some((nano, tree)));
        }
        assert_eq!(parse_u64_id_cursor(None).unwrap(), None);
        // A well-formed pair with a bad hash, and a pair with no separator, are both 400s.
        assert!(matches!(
            parse_u64_id_cursor(Some("12:zz")),
            Err(ApiError::BadRequest(_))
        ));
        assert!(matches!(
            parse_u64_id_cursor(Some("12")),
            Err(ApiError::BadRequest(_))
        ));
        assert!(matches!(
            parse_u64_id_cursor(Some("x:00")),
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

    /// Every EIP-4 R7 type tag we recognise, plus the two ways a token can fail to declare
    /// one. The tag is the hex of R7's `Coll[Byte]` payload, exactly as `TokenRow` stores it.
    #[test]
    fn token_kind_maps_every_eip4_type_tag() {
        assert_eq!(token_kind(Some("0101")), "nft-picture");
        assert_eq!(token_kind(Some("0102")), "nft-audio");
        assert_eq!(token_kind(Some("0103")), "nft-video");
        assert_eq!(token_kind(Some("0201")), "membership");
        // No R7 at all, and an R7 carrying something we do not know, are both plain tokens.
        assert_eq!(token_kind(None), "token");
        assert_eq!(token_kind(Some("0104")), "token");
        assert_eq!(token_kind(Some("")), "token");
        assert_eq!(token_kind(Some("deadbeef")), "token");
        // The mapping is on the exact payload: no prefix or case folding.
        assert_eq!(token_kind(Some("0101ff")), "token");
        assert_eq!(token_kind(Some("0E0101")), "token");
    }

    /// `kind` labels a box by the contract it sits on: the miner-fee contract, the chain's
    /// emission contract (known only once genesis has been seeded), or neither.
    #[test]
    fn box_kind_labels_fee_emission_and_ordinary_trees() {
        let emission = [7u8; 32];
        let ordinary = [9u8; 32];
        assert_eq!(box_kind(&xp_store::FEE_TREE_HASH, None), "fee");
        assert_eq!(box_kind(&xp_store::FEE_TREE_HASH, Some(&emission)), "fee");
        assert_eq!(box_kind(&emission, Some(&emission)), "emission");
        assert_eq!(box_kind(&ordinary, Some(&emission)), "box");
        // Without a seeded genesis there is no emission tree to compare against.
        assert_eq!(box_kind(&emission, None), "box");
    }
}
