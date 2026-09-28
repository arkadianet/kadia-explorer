//! Token touches are transaction membership, not inferred payments or transfer amounts.
use crate::budget::Budget;
use crate::dto::{
    checked_tx_summary_dto, parse_dir, parse_id, parse_u64_cursor, ListParams, PageDto,
    TxSummaryDto,
};
use crate::paging::{Binding, Filter, Route};
use crate::{blocking, ApiError, AppState};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde::Serialize;
use xp_store::token_history::TOKEN_HISTORY_VERSION;

#[derive(Serialize)]
struct HistoryContext {
    scope: &'static str,
    partial_from: Option<u32>,
}
#[derive(Serialize)]
struct HistoryPage {
    #[serde(flatten)]
    page: PageDto<TxSummaryDto>,
    history_context: HistoryContext,
}

pub async fn list(
    State(state): State<AppState>,
    Path(raw): Path<String>,
    Query(mut p): Query<ListParams>,
) -> Result<Response, ApiError> {
    let id = parse_id(&raw)?;
    let cursor = parse_u64_cursor(p.cursor.as_deref())?;
    let dir = parse_dir(p.dir.as_deref())?;
    let limit = p
        .limit
        .as_deref()
        .unwrap_or("20")
        .parse::<usize>()
        .ok()
        .filter(|n| (1..=100).contains(n))
        .ok_or_else(|| ApiError::BadRequest("token history limit must be 1 through 100".into()))?;
    if p.paging
        .consistency
        .as_deref()
        .is_some_and(|s| s != "strict")
    {
        return Err(ApiError::BadRequest(
            "token history requires strict consistency".into(),
        ));
    }
    p.paging.consistency = Some("strict".into());
    blocking(&state, move |rd| {
        let mut budget = Budget::new();
        let binding = Binding::new(
            Route::TokenHistory,
            dir.into(),
            Filter::TokenHistory { token: id, version: TOKEN_HISTORY_VERSION },
        )?;
        let page = p.paging.read(rd, binding, p.cursor.as_deref(), |ctx| {
            let found = rd.token_txs_bounded_admitted(
                &id, cursor, limit, dir, ctx.tx_end()?,
                |bytes| budget.admit_tx_row(bytes),
            )?;
            let mut items = Vec::new();
            for (id, row) in found.items {
                budget.work(1)?;
                items.push(checked_tx_summary_dto(&id, &row)?);
            }
            Ok(PageDto {
                items,
                next_cursor: found.next_cursor.map(|c| c.to_string()),
                paging: Default::default(),
            })
        })?;
        budget.json(&HistoryPage {
            page,
            history_context: HistoryContext {
                scope: "indexed_token_touches",
                partial_from: rd.partial_from()?,
            },
        })
    })
    .await
    .map_err(|error| match error {
        ApiError::Expansion(_) => ApiError::History {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "token_history_budget",
            detail: "token transaction summaries exceed this request's resource budget; reduce the page limit".into(),
        },
        error => error,
    })
}
