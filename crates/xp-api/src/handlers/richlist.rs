use crate::dto::{
    format_u64_id_cursor, parse_limit, parse_u64_id_cursor, ListParams, PageDto, RichlistItemDto,
};
use crate::{blocking, ApiError, AppState};
use axum::extract::{Query, State};
use axum::Json;
use xp_types::hex32;

/// Richest trees by nano-erg balance, descending. The cursor is `"<nano>:<tree hex>"`.
pub async fn list(
    State(state): State<AppState>,
    Query(p): Query<ListParams>,
) -> Result<Json<PageDto<RichlistItemDto>>, ApiError> {
    let limit = parse_limit(p.limit.as_deref())?;
    let cursor = parse_u64_id_cursor(p.cursor.as_deref())?;
    let page = blocking(&state, move |rd| {
        let (rows, next) = rd.richlist(cursor, limit)?;
        let mut items = Vec::with_capacity(rows.len());
        for (tree, nano) in rows {
            items.push(RichlistItemDto {
                address: rd.tree_row(&tree)?.map(|t| t.address),
                tree_hash: hex32(&tree),
                nano: nano.to_string(),
            });
        }
        Ok(PageDto {
            items,
            next_cursor: next.map(|(nano, tree)| format_u64_id_cursor(nano, &tree)),
        })
    })
    .await?;
    Ok(Json(page))
}
