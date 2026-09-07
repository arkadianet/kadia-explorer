use crate::dto::{
    box_dto_from_reader, enrich_boxes, parse_dir, parse_limit, parse_register,
    parse_register_value, parse_u64_cursor, BoxDto, ListParams, PageDto,
};
use crate::{blocking, ApiError, AppState};
use axum::extract::{Path, Query, State};
use axum::Json;
use xp_wire::tree::blake2b256;

/// Boxes whose register `{reg}` holds exactly `{valueHex}` — the serialised sigma constant,
/// which the handler hashes with blake2b-256 the same way the indexer keys `REGISTER_IDX`.
///
/// A value no box carries is an empty page, not a 404: unlike a token or a template there is
/// no row asserting the value ever existed, so "no boxes" is the only truthful answer.
pub async fn boxes(
    State(state): State<AppState>,
    Path((reg_raw, value_raw)): Path<(String, String)>,
    Query(p): Query<ListParams>,
) -> Result<Json<PageDto<BoxDto>>, ApiError> {
    let limit = parse_limit(p.limit.as_deref())?;
    let cursor = parse_u64_cursor(p.cursor.as_deref())?;
    let dir = parse_dir(p.dir.as_deref())?;
    let reg = parse_register(&reg_raw)?;
    let value_hash = blake2b256(&parse_register_value(&value_raw)?);
    let page = blocking(&state, move |rd| {
        let emission = rd.emission_tree_hash()?;
        let tip = rd.indexed_height()?;
        let page = rd.boxes_by_register(reg, &value_hash, cursor, limit, dir)?;
        let mut items = page
            .items
            .iter()
            .map(|(id, row)| box_dto_from_reader(rd, id, row, tip, emission.as_ref()))
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
