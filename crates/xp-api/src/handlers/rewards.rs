//! Observed mainnet EIP-27 emission rewards. Never infer net miner income from height alone.
use crate::budget::Budget;
use crate::{blocking, ApiError, AppState};
use axum::{
    extract::{Path, State},
    Json,
};
use serde::Serialize;
use xp_store::{
    read::{Dir, ExpansionRow},
    rows::BoxRow,
    Reader,
};
use xp_types::Hash32;

const EMISSION_NFT: &str = "20fa2bf23962cdf51b07722d6237c0c7b8a44f78856c0f7ec308dc1ef1a92a51";
const REEMISSION_TOKEN: &str = "d9a2cc8a09abfaed87afacfbb7daee79a6b26f10c6613fc13d3f3953e5521d1a";

#[derive(Serialize)]
pub struct RewardBreakdown {
    pub block_id: String,
    pub height: u32,
    pub basis: &'static str,
    pub gross_reward: Option<String>,
    pub reemission_obligation: Option<String>,
    pub miner_subsidy: Option<String>,
    pub transaction_fees: String,
    pub reward_box_id: Option<String>,
    pub transaction_id: Option<String>,
    pub note: &'static str,
}

fn read_box(rd: &Reader, budget: &mut Budget, id: &Hash32) -> Result<BoxRow, ApiError> {
    budget.admit_row(rd, ExpansionRow::Box, id)?;
    rd.box_by_id_checked(id, || {
        budget
            .check()
            .map_err(|_| xp_store::StoreError::ReadLimit("expansion_deadline"))
    })?
    .ok_or_else(|| ApiError::Integrity("missing reward output".into()))
}

// Mainnet's 720-block delayed miner reward contract, with the header's compressed key.
// The complete byte equality deliberately rejects other scripts and future contract forms.
fn reward_script(miner_pk: &[u8; 33]) -> Vec<u8> {
    hex::decode(format!(
        "100204a00b08cd{}ea02d192a39a8cc7a70173007301",
        hex::encode(miner_pk)
    ))
    .expect("fixed reward script hex")
}

pub async fn get(
    State(state): State<AppState>,
    Path(raw): Path<String>,
) -> Result<Json<RewardBreakdown>, ApiError> {
    let result = blocking(&state, move |rd| {
        let height = super::blocks::resolve_height(rd, &raw)?;
        let mut budget = Budget::new();
        let h = rd.header_at_admitted(height, |n| budget.admit_tx_row(n))?.ok_or(ApiError::NotFound)?;
        let mut result = RewardBreakdown {
            block_id: hex::encode(h.id), height,
            basis: "unsupported", gross_reward: None, reemission_obligation: None,
            miner_subsidy: None, transaction_fees: h.fees.to_string(),
            reward_box_id: None, transaction_id: None,
            note: "No supported EIP-27 emission reward was identified. Fees are separate; storage-rent income is not included.",
        };
        // This recognizer covers the original emission phase after EIP-27 activation.
        // Re-emission withdrawals and pre-activation rewards need different evidence.
        if !(777_217..2_080_800).contains(&height) { return Ok(result); }
        let page = rd.txs_in_block_page_admitted(height, None, 1, Dir::Asc, |n| budget.admit_tx_row(n))?;
        let Some((tx_id, tx)) = page.items.first() else { return Ok(result); };
        if tx.index != 0 || tx.output_count != 2 { return Ok(result); }
        let reserve = read_box(rd, &mut budget, &rd.output_id(tx, 0)?)?;
        let nft = crate::dto::parse_id(EMISSION_NFT)?;
        if reserve.tx_id != *tx_id || reserve.index != 0 || reserve.creation_height != height || !reserve.tokens.contains(&(nft, 1)) { return Ok(result); }
        let id = rd.output_id(tx, 1)?;
        let reward = read_box(rd, &mut budget, &id)?;
        budget.admit_row(rd, ExpansionRow::Tree, &reward.tree_hash)?;
        let tree = rd.tree_row_checked(&reward.tree_hash, || budget.check().map_err(|_| xp_store::StoreError::ReadLimit("expansion_deadline")))?.ok_or_else(|| ApiError::Integrity("missing reward tree".into()))?;
        if reward.tx_id != *tx_id || reward.index != 1 || reward.creation_height != height || tree.tree_bytes != reward_script(&h.miner_pk) { return Ok(result); }
        let token = crate::dto::parse_id(REEMISSION_TOKEN)?;
        let Some((_, obligation)) = reward.tokens.iter().find(|(id, _)| *id == token) else { return Ok(result); };
        let Some(net) = reward.value.checked_sub(*obligation) else { return Ok(result); };
        budget.check()?;
        result.basis = "observed_eip27_reward_box";
        result.gross_reward = Some(reward.value.to_string());
        result.reemission_obligation = Some(obligation.to_string());
        result.miner_subsidy = Some(net.to_string());
        result.reward_box_id = Some(hex::encode(id));
        result.transaction_id = Some(hex::encode(tx_id));
        result.note = "Gross reward minus the observed EIP-27 token obligation. The obligation is paid when the reward box is spent, not necessarily in this block. Fees are separate; storage-rent income is not included.";
        Ok(result)
    }).await.map_err(|error| match error {
        ApiError::Expansion(_) => ApiError::History {
            status: axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            code: "reward_evidence_limit",
            detail: "Reward evidence exceeds this request's read budget.".into(),
        },
        other => other,
    })?;
    Ok(Json(result))
}
