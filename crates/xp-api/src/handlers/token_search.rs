//! Name discovery is an indexed lookup, never a claim of token authenticity.
use crate::dto::{parse_id, parse_limit, token_info_dto, PageDto, TokenInfoDto};
use crate::paging::{Binding, Filter, Order, Route};
use crate::{blocking, ApiError, AppState};
use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use xp_store::token_search::{
    normalize_token_name, TokenNameMatch, MAX_TOKEN_NAME_RESULTS, TOKEN_NAME_INDEX_VERSION,
};
use xp_types::hex32;

#[derive(Deserialize)]
pub struct Params {
    q: String,
    r#match: Option<String>,
    limit: Option<String>,
    cursor: Option<String>,
    #[serde(flatten)]
    paging: crate::paging::Request,
}

#[derive(Serialize)]
pub struct SearchPage {
    #[serde(flatten)]
    page: PageDto<TokenInfoDto>,
    search: SearchMetadata,
}

#[derive(Serialize)]
struct SearchMetadata {
    query: String,
    normalized_query: String,
    r#match: &'static str,
    index_version: u32,
    coverage: &'static str,
    partial_from: Option<u32>,
    indexed_names: u64,
    total_tokens: u64,
    unindexed_tokens: u64,
}

pub async fn search(
    State(state): State<AppState>,
    Query(p): Query<Params>,
) -> Result<Json<SearchPage>, ApiError> {
    let normalized_query = normalize_token_name(&p.q)?;
    let (mode, match_name) = match p.r#match.as_deref().unwrap_or("prefix") {
        "prefix" => (TokenNameMatch::Prefix, "prefix"),
        "exact" => (TokenNameMatch::Exact, "exact"),
        _ => return Err(ApiError::BadRequest("match must be prefix or exact".into())),
    };
    let limit = parse_limit(p.limit.as_deref())?.min(MAX_TOKEN_NAME_RESULTS);
    let cursor = p.cursor.as_deref().map(parse_id).transpose()?;
    let result = blocking(&state, move |rd| {
        let status = rd.token_name_index_status()?;
        if !status.ready {
            return Err(xp_store::StoreError::TokenSearchNotReady.into());
        }
        let partial_from = rd.partial_from()?;
        let binding = Binding::new(
            Route::TokensSearch,
            Order::Asc,
            Filter::TokenNames {
                query_hash: xp_wire::tree::blake2b256(normalized_query.as_bytes()),
                exact: mode == TokenNameMatch::Exact,
                index_version: TOKEN_NAME_INDEX_VERSION,
            },
        )?;
        let page = p.paging.read(rd, binding, p.cursor.as_deref(), |ctx| {
            let found = ctx.reader().search_token_names(&p.q, mode, cursor, limit)?;
            Ok(PageDto {
                paging: Default::default(),
                items: found
                    .items
                    .iter()
                    .map(|(id, row)| token_info_dto(id, row))
                    .collect(),
                next_cursor: found.next_cursor.as_ref().map(hex32),
            })
        })?;
        Ok(SearchPage {
            page,
            search: SearchMetadata {
                query: p.q,
                normalized_query,
                r#match: match_name,
                index_version: status.version,
                coverage: if partial_from.is_some() || status.unindexed_tokens > 0 {
                    "partial"
                } else {
                    "complete"
                },
                partial_from,
                indexed_names: status.indexed_names,
                total_tokens: status.total_tokens,
                unindexed_tokens: status.unindexed_tokens,
            },
        })
    })
    .await?;
    Ok(Json(result))
}
