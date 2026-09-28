use crate::dto::{parse_id, SearchDto, SearchMatchDto, SearchParams};
use crate::{blocking, ApiError, AppState};
use axum::extract::{Query, State};
use axum::Json;

/// Resolution order for a 64-hex term is header id, tx id, box id, token id, then template
/// hash — the same order the spec lists, and the order of decreasing cardinality on chain.
pub async fn search(
    State(state): State<AppState>,
    Query(p): Query<SearchParams>,
) -> Result<Json<SearchDto>, ApiError> {
    let q = p.q.unwrap_or_default().trim().to_owned();
    if q.is_empty() {
        return Err(ApiError::BadRequest("q must not be empty".into()));
    }
    let dto = blocking(&state, move |rd| {
        if q.bytes().all(|b| b.is_ascii_digit()) {
            let height: u32 = q
                .parse()
                .map_err(|_| ApiError::BadRequest(format!("height out of range: {q:?}")))?;
            return match rd.header_at(height)? {
                Some(_) => Ok(SearchDto::single("block", height.to_string())),
                None => Err(ApiError::NotFound),
            };
        }
        if q.len() == 64 && q.bytes().all(|b| b.is_ascii_hexdigit()) {
            let id = parse_id(&q)?;
            let mut matches = Vec::new();
            for (kind, found) in [
                ("block", rd.height_of_header(&id)?.is_some()),
                ("tx", rd.tx_by_id(&id)?.is_some()),
                ("box", rd.box_by_id(&id)?.is_some()),
                ("token", rd.token(&id)?.is_some()),
                ("template", rd.template(&id)?.is_some()),
            ] {
                if found {
                    matches.push(SearchMatchDto {
                        kind,
                        id: q.clone(),
                    });
                }
            }
            if let Some(first) = matches.first() {
                return Ok(SearchDto {
                    kind: first.kind,
                    id: q,
                    matches,
                });
            }
            return Err(ApiError::NotFound);
        }
        // Validate network, checksum and script. P2S prefixes vary with script length;
        // neither the first character nor the text length identifies an Ergo address.
        if xp_wire::tree::address_tree_hash(&q).is_ok() {
            return match rd.tree_by_address(&q)? {
                Some(_) => Ok(SearchDto::single("address", q)),
                None => Err(ApiError::NotFound),
            };
        }
        Err(ApiError::BadRequest(format!(
            "q is not a height, a 64-hex id, or an address: {q:?}"
        )))
    })
    .await?;
    Ok(Json(dto))
}
