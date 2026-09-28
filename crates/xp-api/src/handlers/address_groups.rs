//! Explicit, read-only group queries. Labels and grouping remain client-local.
use crate::budget::Budget;
use crate::{blocking, ApiError, AppState};
use axum::body::to_bytes;
use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::response::Response;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use xp_store::read::ExpansionRow;
use xp_store::{Reader, StoreError};
use xp_types::{hex32, Hash32};

pub const MAX_REQUEST_BYTES: usize = 512_000;
const MAX_ADDRESSES: usize = 100;
const MAX_ADDRESS_BYTES: usize = 4096;
const MAX_TOTAL_ADDRESS_BYTES: usize = 128_000;
const MAX_TOKEN_ENTRIES: usize = 5000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Query {
    #[serde(deserialize_with = "bounded_addresses")]
    addresses: Vec<String>,
}

fn bounded_addresses<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<String>, D::Error> {
    struct Addresses;
    impl<'de> serde::de::Visitor<'de> for Addresses {
        type Value = Vec<String>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an array of at most 100 address strings")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            let mut addresses = Vec::new();
            while let Some(address) = seq.next_element::<String>()? {
                if addresses.len() == MAX_ADDRESSES {
                    return Err(serde::de::Error::custom("too many addresses"));
                }
                addresses.push(address);
            }
            Ok(addresses)
        }
    }
    deserializer.deserialize_seq(Addresses)
}

#[derive(Serialize)]
struct Anchor {
    height: u32,
    block_id: String,
}
#[derive(Serialize)]
struct Token {
    id: String,
    amount: String,
}
#[derive(Serialize)]
struct Balance {
    nano: String,
    tokens: Vec<Token>,
}
#[derive(Serialize)]
struct Member {
    address: String,
    tree_hash: Option<String>,
    status: &'static str,
    duplicate_of: Option<usize>,
    balance: Option<Balance>,
}
#[derive(Serialize)]
struct Group {
    scope: &'static str,
    consistency: &'static str,
    indexed_height: Option<u32>,
    anchor: Option<Anchor>,
    full_history: bool,
    partial_from: Option<u32>,
    complete: bool,
    requested_count: usize,
    resolved_script_count: usize,
    members: Vec<Member>,
    observed_totals: Option<Balance>,
}

fn problem(status: StatusCode, code: &'static str, detail: &str) -> ApiError {
    ApiError::History {
        status,
        code,
        detail: detail.into(),
    }
}
fn invalid(detail: &str) -> ApiError {
    problem(StatusCode::BAD_REQUEST, "invalid_group_request", detail)
}
fn request_limit() -> ApiError {
    problem(
        StatusCode::PAYLOAD_TOO_LARGE,
        "group_request_limit",
        "Group request exceeds its byte limit; select fewer or shorter addresses.",
    )
}
fn group_error(error: ApiError) -> ApiError {
    let ApiError::Expansion(code) = error else {
        return error;
    };
    let code = match code {
        "expansion_decode_limit" => "group_decode_limit",
        "expansion_response_limit" => "group_response_limit",
        "expansion_deadline" => "group_deadline",
        _ => "group_work_limit",
    };
    problem(
        StatusCode::UNPROCESSABLE_ENTITY,
        code,
        "The selected group exceeds the synchronous balance budget; select fewer addresses. No partial monetary total was returned.",
    )
}
fn store_limit(error: ApiError) -> StoreError {
    match error {
        ApiError::Expansion(code) => StoreError::ReadLimit(code),
        _ => StoreError::Corrupt("unexpected balance admission failure"),
    }
}
fn add(a: u128, b: u64) -> Result<u128, ApiError> {
    a.checked_add(u128::from(b))
        .ok_or_else(|| ApiError::Integrity("group balance overflow".into()))
}

fn read(rd: &Reader, addresses: Vec<String>) -> Result<Response, ApiError> {
    let mut budget = Budget::new();
    let indexed_height = rd.indexed_height()?;
    let full_history = rd.full_history()?;
    let partial_from = rd.partial_from()?;
    let anchor = indexed_height
        .map(|height| {
            Ok::<_, ApiError>(rd.header_id_at(height)?.map(|id| Anchor {
                height,
                block_id: hex32(&id),
            }))
        })
        .transpose()?
        .flatten();
    if full_history && indexed_height.is_some_and(|h| h > 0) && anchor.is_none() {
        return Err(ApiError::Integrity("missing canonical group anchor".into()));
    }
    let requested_count = addresses.len();
    let mut seen = BTreeMap::<Hash32, usize>::new();
    let mut members = Vec::with_capacity(requested_count);
    let mut resolved_script_count = 0;
    let mut all_resolved = true;
    let mut nano = 0u128;
    let mut tokens = BTreeMap::<Hash32, u128>::new();
    let mut token_entries = 0usize;
    for (index, address) in addresses.into_iter().enumerate() {
        budget.work(1 + address.len().div_ceil(128))?;
        let tree = xp_wire::tree::address_tree_hash(&address);
        budget.check()?;
        let Ok(tree) = tree else {
            all_resolved = false;
            members.push(Member {
                address,
                tree_hash: None,
                status: "invalid",
                duplicate_of: None,
                balance: None,
            });
            continue;
        };
        let tree = tree.0;
        if let Some(&first) = seen.get(&tree) {
            members.push(Member {
                address,
                tree_hash: Some(hex32(&tree)),
                status: "duplicate",
                duplicate_of: Some(first),
                balance: None,
            });
            continue;
        }
        seen.insert(tree, index);
        let row = rd.balance_admitted(&tree, |bytes, count| {
            budget.admit_tx_row(bytes)?;
            token_entries = token_entries
                .checked_add(count)
                .filter(|total| *total <= MAX_TOKEN_ENTRIES)
                .ok_or(StoreError::ReadLimit("group_token_limit"))?;
            budget.work(count).map_err(store_limit)
        })?;
        budget.check()?;
        let Some(row) = row else {
            // A retained script always has a balance row, even after its last box is spent.
            if rd.expansion_row_len(ExpansionRow::Tree, &tree)? > 0 {
                return Err(ApiError::Integrity("missing indexed script balance".into()));
            }
            all_resolved = false;
            members.push(Member {
                address,
                tree_hash: Some(hex32(&tree)),
                status: "unseen",
                duplicate_of: None,
                balance: None,
            });
            continue;
        };
        nano = add(nano, row.nano)?;
        let mut member_tokens = BTreeMap::new();
        for (id, amount) in row.tokens {
            budget.check()?;
            if member_tokens.insert(id, amount).is_some() {
                return Err(ApiError::Integrity(
                    "duplicate token in script balance".into(),
                ));
            }
            let total = tokens.entry(id).or_default();
            *total = add(*total, amount)?;
        }
        resolved_script_count += 1;
        members.push(Member {
            address,
            tree_hash: Some(hex32(&tree)),
            status: "resolved",
            duplicate_of: None,
            balance: Some(Balance {
                nano: row.nano.to_string(),
                tokens: member_tokens
                    .into_iter()
                    .map(|(id, amount)| Token {
                        id: hex32(&id),
                        amount: amount.to_string(),
                    })
                    .collect(),
            }),
        });
    }
    let complete = all_resolved && full_history;
    let group = Group {
        scope: "selected_address_scripts",
        consistency: "single_reader",
        indexed_height,
        anchor,
        full_history,
        partial_from,
        complete,
        requested_count,
        resolved_script_count,
        members,
        observed_totals: all_resolved.then(|| Balance {
            nano: nano.to_string(),
            tokens: tokens
                .into_iter()
                .map(|(id, amount)| Token {
                    id: hex32(&id),
                    amount: amount.to_string(),
                })
                .collect(),
        }),
    };
    budget.json(&group)
}

pub async fn balances(
    State(state): State<AppState>,
    request: Request,
) -> Result<Response, ApiError> {
    let content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next());
    if !content_type.is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json")) {
        return Err(invalid(
            "Send an application/json object containing only addresses.",
        ));
    }
    let body = to_bytes(request.into_body(), MAX_REQUEST_BYTES)
        .await
        .map_err(|_| request_limit())?;
    let query: Query = serde_json::from_slice(&body).map_err(|_| {
        invalid("Send an object with an addresses array of strings; no labels or group names.")
    })?;
    if query.addresses.is_empty() || query.addresses.len() > MAX_ADDRESSES {
        return Err(invalid("Select between 1 and 100 addresses."));
    }
    if query
        .addresses
        .iter()
        .any(|address| address.len() > MAX_ADDRESS_BYTES)
        || query.addresses.iter().map(String::len).sum::<usize>() > MAX_TOTAL_ADDRESS_BYTES
    {
        return Err(request_limit());
    }
    blocking(&state, move |rd| read(rd, query.addresses))
        .await
        .map_err(group_error)
}
