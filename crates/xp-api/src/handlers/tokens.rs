use crate::dto::{
    box_dto_from_reader, enrich_boxes, format_u64_id_cursor, parse_bool_param, parse_dir, parse_id,
    parse_limit, parse_token_sort, parse_u64_cursor, parse_u64_id_cursor, share_pct,
    token_info_dto, AddrBoxParams, BoxDto, CursorParams, PageDto, TokenHolderDto, TokenInfoDto,
    TokenSort, TokensListParams,
};
use crate::{blocking, ApiError, AppState};
use axum::extract::{Path, Query, State};
use axum::Json;
use xp_store::rows::TokenRow;
use xp_store::Reader;
use xp_types::{hex32, Hash32};

/// The token's mint row, or a 404. Every `/v1/tokens/{id}/…` sub-route goes through this, so
/// an unknown token is a 404 rather than an empty page that looks like a token with nothing
/// in it.
fn token_of(rd: &Reader, id: &Hash32) -> Result<TokenRow, ApiError> {
    rd.token(id)?.ok_or(ApiError::NotFound)
}

/// All indexed tokens: newest mint first by default, or most-held first with
/// `sort=holders`. Both orderings are descending; there is no `dir`.
pub async fn list(
    State(state): State<AppState>,
    Query(p): Query<TokensListParams>,
) -> Result<Json<PageDto<TokenInfoDto>>, ApiError> {
    let limit = parse_limit(p.limit.as_deref())?;
    let sort = parse_token_sort(p.sort.as_deref())?;
    // The two sorts read the same `cursor` parameter but not the same cursor *shape*, so
    // each parses only its own.
    let (gidx_cursor, count_cursor) = match sort {
        TokenSort::Newest => (parse_u64_cursor(p.cursor.as_deref())?, None),
        TokenSort::Holders => (None, parse_u64_id_cursor(p.cursor.as_deref())?),
    };
    let page = blocking(&state, move |rd| match sort {
        TokenSort::Newest => {
            let page = rd.tokens_newest(gidx_cursor, limit)?;
            Ok(PageDto {
                items: page
                    .items
                    .iter()
                    .map(|(id, row)| token_info_dto(id, row))
                    .collect(),
                next_cursor: page.next_cursor.map(|c| c.to_string()),
            })
        }
        TokenSort::Holders => {
            let (rows, next) = rd.tokens_by_holders(count_cursor, limit)?;
            Ok(PageDto {
                items: rows
                    .iter()
                    .map(|(id, row)| token_info_dto(id, row))
                    .collect(),
                next_cursor: next.map(|(count, id)| format_u64_id_cursor(count, &id)),
            })
        }
    })
    .await?;
    Ok(Json(page))
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(raw): Path<String>,
) -> Result<Json<TokenInfoDto>, ApiError> {
    let dto = blocking(&state, move |rd| {
        let id = parse_id(&raw)?;
        Ok(token_info_dto(&id, &token_of(rd, &id)?))
    })
    .await?;
    Ok(Json(dto))
}

/// The token's holders, largest balance first. The cursor is `"<amount>:<tree hex>"`.
///
/// The ordering is the `TOKEN_HOLDERS` index's own, so there is no `dir`: the params struct
/// omits it rather than accepting one and ignoring it.
pub async fn holders(
    State(state): State<AppState>,
    Path(raw): Path<String>,
    Query(p): Query<CursorParams>,
) -> Result<Json<PageDto<TokenHolderDto>>, ApiError> {
    let limit = parse_limit(p.limit.as_deref())?;
    let cursor = parse_u64_id_cursor(p.cursor.as_deref())?;
    let page = blocking(&state, move |rd| {
        let id = parse_id(&raw)?;
        let token = token_of(rd, &id)?;
        // Shares are of the circulating supply, so burned units do not dilute a holder.
        let supply = token.emission.saturating_sub(token.burned);
        let (rows, next) = rd.token_holders(&id, cursor, limit)?;
        let mut items = Vec::with_capacity(rows.len());
        for (tree, amount) in rows {
            items.push(TokenHolderDto {
                address: rd.tree_row(&tree)?.map(|t| t.address),
                tree_hash: hex32(&tree),
                amount: amount.to_string(),
                share_pct: share_pct(amount, supply),
            });
        }
        Ok(PageDto {
            items,
            next_cursor: next.map(|(amount, tree)| format_u64_id_cursor(amount, &tree)),
        })
    })
    .await?;
    Ok(Json(page))
}

/// Boxes carrying the token: every one ever, or `unspent=true` for the live set.
pub async fn boxes(
    State(state): State<AppState>,
    Path(raw): Path<String>,
    Query(p): Query<AddrBoxParams>,
) -> Result<Json<PageDto<BoxDto>>, ApiError> {
    let limit = parse_limit(p.limit.as_deref())?;
    let cursor = parse_u64_cursor(p.cursor.as_deref())?;
    let dir = parse_dir(p.dir.as_deref())?;
    let unspent = parse_bool_param(p.unspent.as_deref(), "unspent")?;
    let page = blocking(&state, move |rd| {
        let emission = rd.emission_tree_hash()?;
        let id = parse_id(&raw)?;
        token_of(rd, &id)?;
        let tip = rd.indexed_height()?;
        let page = rd.token_boxes(&id, unspent, cursor, limit, dir)?;
        let mut items = page
            .items
            .iter()
            .map(|(bid, row)| box_dto_from_reader(rd, bid, row, tip, emission.as_ref()))
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
