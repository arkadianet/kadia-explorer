use crate::dto::{
    box_dto_from_reader, enrich_boxes, parse_bool_param, parse_dir, parse_id, parse_limit,
    parse_u64_cursor, template_dto, AddrBoxParams, BoxDto, PageDto, TemplateDto,
};
use crate::{blocking, ApiError, AppState};
use axum::extract::{Path, Query, State};
use axum::Json;

/// One script template: how many boxes have ever sat on it, how many still do, and an
/// example of a concrete tree using it.
pub async fn get_one(
    State(state): State<AppState>,
    Path(raw): Path<String>,
) -> Result<Json<TemplateDto>, ApiError> {
    let dto = blocking(&state, move |rd| {
        let hash = parse_id(&raw)?;
        let row = rd.template(&hash)?.ok_or(ApiError::NotFound)?;
        let example = rd.tree_row(&row.example_tree)?.map(|t| t.address);
        Ok(template_dto(&hash, &row, example))
    })
    .await?;
    Ok(Json(dto))
}

/// Boxes on this template — the same contract whatever its segregated constants.
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
        let hash = parse_id(&raw)?;
        // An unknown template is a 404, not an empty page.
        rd.template(&hash)?.ok_or(ApiError::NotFound)?;
        let tip = rd.indexed_height()?;
        let page = rd.template_boxes(&hash, unspent, cursor, limit, dir)?;
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
