use crate::dto::{parse_id, SearchDto, SearchMatchDto, SearchParams};
use crate::{blocking, ApiError, AppState};
use axum::extract::{Query, State};
use axum::Json;

const BASE58: &str = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

/// Cheap shape test for a mainnet address, used only to choose between "this looked like an
/// address but is unknown" (404) and "this is not a searchable term at all" (400). Real
/// parsing happens in `Reader::tree_by_address`, which cannot report *why* it failed.
fn looks_like_address(q: &str) -> bool {
    matches!(q.chars().next(), Some('9' | '3' | '2' | '8'))
        // No upper length bound: a mainnet P2S address encodes the whole script, so it can
        // run to several hundred characters.
        && q.len() >= 40
        && q.chars().all(|c| BASE58.contains(c))
}

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
        if q.len() == 64 {
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
        if looks_like_address(&q) {
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
