//! Parity gate: samples addresses seen in recent explorer blocks and compares balance,
//! unspent box-id set, and tx count against the Rust node's `/blockchain` API. Also samples
//! token ids off the boxes seen during the address comparison and compares each token's
//! emission, name, and unspent box count against the node's token endpoints.
//!
//! Ignored by default — it needs a live explorer and a live node at the same height. Run
//! via `scripts/parity.sh`, or directly:
//!
//! ```text
//! EXPLORER_URL=http://127.0.0.1:8090 NODE_URL=http://127.0.0.1:9063 \
//!     cargo test -p xp-api --test parity -- --ignored --nocapture
//! ```
//!
//! Missing configuration or insufficient evidence is inconclusive and cannot pass a release run.

use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

const SAMPLE_TARGET: usize = 200;
const BLOCKS_TO_SAMPLE: u32 = 20;
const SHUFFLE_SEED: u64 = 0xC0FF_EE15_5EED_1234;
/// How many distinct token ids (sampled off boxes seen while comparing addresses) get their
/// own emission/name/unspent-count comparison against the node.
const TOKEN_SAMPLE_TARGET: usize = 20;
/// The node's own cap on `/blockchain/box/unspent/byAddress` and
/// `/blockchain/box/unspent/byTokenId` — mirrored here so we know when its list may be a
/// truncated prefix rather than the whole set.
const NODE_UNSPENT_CAP: usize = 16_384;
/// Safety bound on cursor-walk pages, so a store bug that never returns `next_cursor: null`
/// can't spin the test forever.
const MAX_PAGES: usize = 200;
/// Page size used by explorer cursor walks; with [`MAX_PAGES`] it bounds how many
/// boxes a walk can see before it is truncated.
const PAGE_LIMIT: usize = 500;
/// Most boxes the explorer's unspent-box walk can observe. An address the node says has more
/// unspent boxes than this is not comparable by walking, so it is skipped up front rather
/// than compared against a silently truncated walk.
const WALK_CAP: u64 = (MAX_PAGES * PAGE_LIMIT) as u64;
/// Mainnet miner-fee contract address. Every tx that pays a fee creates an output here, so
/// it appears in essentially every block and holds a huge, constantly churning box set: it
/// is a guaranteed page-cap skip and a guaranteed mempool-race flake, and comparing it
/// tells us nothing the ordinary addresses in the sample don't.
const MINER_FEE_ADDRESS: &str = "2iHkR7CWvD1R4j1yZg5bkeDRQavjAaVPeTDFGGLZduHyfWMuYpmhHocX8GJoaieTx78FntzJbCBVL6rf96ocJoZdmWBL2fci7NqWgAirppPQmZ7fN9V6z13Ay6brPriBKYqLp1bT2Fk4FkFLCfdPpe";
/// An address appearing as an output in more than this many of the [`BLOCKS_TO_SAMPLE`]
/// sampled blocks is a pool, exchange or miner payout address rather than an ordinary user
/// address. Those are exactly the addresses with box/tx counts past [`WALK_CAP`] and with
/// mempool churn fast enough to make the unspent-set recheck flaky, so they are excluded
/// from the sample: the gate is there to catch indexing bugs, and a sample made of ten
/// unverifiable hot addresses catches none.
const HOT_ADDRESS_BLOCK_THRESHOLD: usize = 5;

/// A tiny deterministic PRNG (xorshift64) so the sample is reproducible across runs without
/// pulling in a `rand` dependency.
struct XorShift64(u64);

impl XorShift64 {
    fn new(seed: u64) -> Self {
        // xorshift is undefined for a zero state.
        Self(seed | 1)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

/// Fisher-Yates shuffle driven by [`XorShift64`], so the same seed always produces the same
/// permutation regardless of input order beyond the swap sequence itself.
fn deterministic_shuffle<T: Ord>(items: &mut [T], seed: u64) {
    items.sort();
    let mut rng = XorShift64::new(seed);
    for i in (1..items.len()).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        items.swap(i, j);
    }
}

fn strict_ids(items: &[Value], key: &str) -> Result<HashSet<String>, String> {
    let mut ids = HashSet::new();
    for item in items {
        let id = item[key].as_str().ok_or("missing box ID")?;
        if !ids.insert(id.to_string()) {
            return Err("duplicate box ID".into());
        }
    }
    Ok(ids)
}

fn supply_agrees(token: &Value, held: u128) -> Result<bool, String> {
    let number = |key: &str| {
        token[key]
            .as_str()
            .ok_or_else(|| format!("missing {key}"))?
            .parse::<u128>()
            .map_err(|_| format!("invalid {key}"))
    };
    let (emission, burned, supply) = (number("emission")?, number("burned")?, number("supply")?);
    Ok(emission.checked_sub(burned) == Some(supply) && held == supply)
}

async fn holder_sum(client: &reqwest::Client, explorer: &str, token: &str) -> Result<u128, String> {
    let mut cursor = None::<String>;
    let mut seen = HashSet::new();
    let mut total = 0u128;
    for _ in 0..MAX_PAGES {
        let suffix = cursor
            .as_ref()
            .map(|c| format!("&cursor={c}"))
            .unwrap_or_default();
        let (status, page) = get_json(
            client,
            &format!("{explorer}/v1/tokens/{token}/holders?limit={PAGE_LIMIT}{suffix}"),
        )
        .await?;
        if status != 200 {
            return Err(format!("holder status {status}"));
        }
        for item in page["items"].as_array().ok_or("missing holders")? {
            let address = item["address"].as_str().ok_or("missing holder address")?;
            if !seen.insert(address.to_string()) {
                return Err("duplicate holder".into());
            }
            let amount = item["amount"]
                .as_str()
                .ok_or("missing holder amount")?
                .parse::<u128>()
                .map_err(|_| "invalid holder amount")?;
            total = total.checked_add(amount).ok_or("holder sum overflow")?;
        }
        match page.get("next_cursor") {
            Some(Value::Null) => return Ok(total),
            Some(Value::String(c)) => cursor = Some(c.clone()),
            _ => return Err("missing holder cursor".into()),
        }
    }
    Err("holder page cap reached".into())
}

async fn snapshot(
    client: &reqwest::Client,
    explorer: &str,
    node: &str,
) -> Result<(u64, String), String> {
    let (es, e) = get_json(client, &format!("{explorer}/v1/status")).await?;
    let (ns, n) = get_json(client, &format!("{node}/blockchain/indexedHeight")).await?;
    let height = e["indexed"].as_u64().ok_or("missing explorer height")?;
    if es != 200 || ns != 200 || n["indexedHeight"].as_u64() != Some(height) {
        return Err("unmatched tip heights".into());
    }
    let (es, e) = get_json(client, &format!("{explorer}/v1/blocks/{height}")).await?;
    let (ns, n) = get_json(
        client,
        &format!("{node}/blocks/chainSlice?fromHeight={height}&toHeight={height}"),
    )
    .await?;
    let id = e["id"].as_str().ok_or("missing explorer tip hash")?;
    if es != 200 || ns != 200 || n[0]["id"].as_str() != Some(id) {
        return Err("unmatched best-chain hashes".into());
    }
    Ok((height, id.into()))
}

#[derive(Default)]
struct Evidence {
    mismatches: Vec<Mismatch>,
    skipped: Vec<Skipped>,
    passed: Vec<&'static str>,
}

const FIELDS: [(&str, usize); 7] = [
    ("balance.nano", 20),
    ("tx_count", 20),
    ("unspent_ids", 20),
    ("token.emission", 5),
    ("token.name", 5),
    ("token.supply_internal", 5),
    ("token.unspent_box_ids", 5),
];

fn coverage(e: &Evidence, addresses: usize, tokens: usize) -> Value {
    let mut out = serde_json::Map::new();
    for (field, floor) in FIELDS {
        let attempted = if field.starts_with("token.") {
            tokens
        } else {
            addresses
        };
        let passed = e.passed.iter().filter(|f| **f == field).count();
        let failed = e.mismatches.iter().filter(|m| m.field == field).count();
        out.insert(field.into(), serde_json::json!({ "attempted": attempted, "passed": passed,
            "failed": failed, "skipped": attempted.saturating_sub(passed + failed), "floor": floor }));
    }
    Value::Object(out)
}

fn verdict(e: &Evidence, coverage: &Value) -> &'static str {
    if !e.mismatches.is_empty() {
        return "fail";
    }
    if FIELDS
        .iter()
        .any(|(field, floor)| coverage[*field]["passed"].as_u64().unwrap_or(0) < *floor as u64)
    {
        return "inconclusive";
    }
    // Only explicit reference/page caps may be excluded in a passing release run.
    if e.skipped.iter().any(|s| !s.reason.contains("cap")) {
        return "inconclusive";
    }
    "pass"
}

/// A real disagreement between explorer and node for one address/field — these are the only
/// things the final `assert_eq!` counts.
#[derive(Debug)]
struct Mismatch {
    address: String,
    field: &'static str,
    detail: String,
}

impl std::fmt::Display for Mismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}: {}", self.address, self.field, self.detail)
    }
}

/// An address/field that could not be compared at all (transport error, unparseable field,
/// too many boxes for the node's cap, ...). Non-cap errors make the release inconclusive.
#[derive(Debug)]
struct Skipped {
    address: String,
    reason: String,
}

impl std::fmt::Display for Skipped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.address, self.reason)
    }
}

async fn get_json(client: &reqwest::Client, url: &str) -> Result<(u16, Value), String> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("GET {url} failed: {e}"))?;
    let status = resp.status().as_u16();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("GET {url} body read failed: {e}"))?;
    let json = if body.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&body).map_err(|e| format!("GET {url} bad json: {e} body={body}"))?
    };
    Ok((status, json))
}

/// Walks explorer `/v1/addresses/{addr}/boxes?unspent=true`, collecting every box id.
///
/// When `token_ids` is `Some`, every distinct token id carried on a box seen during the walk
/// is also recorded there — this is how the token sample for [`compare_token`] is built,
/// piggybacking on box pages the address comparison fetches anyway rather than issuing
/// separate requests just to discover token ids.
///
/// [`Capped::Truncated`] means the walk stopped at [`MAX_PAGES`] with a `next_cursor` still
/// outstanding, so the returned set is a prefix of the address's boxes, not the whole set.
/// The caller must treat that as unverifiable rather than compare the prefix against the
/// node's full set and call the difference a mismatch.
async fn explorer_unspent_ids(
    client: &reqwest::Client,
    explorer: &str,
    addr: &str,
    mut token_ids: Option<&mut HashSet<String>>,
) -> Result<(HashSet<String>, Capped), String> {
    let mut ids = HashSet::new();
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let url = match &cursor {
            Some(c) => format!(
                "{explorer}/v1/addresses/{addr}/boxes?unspent=true&limit={PAGE_LIMIT}&cursor={c}"
            ),
            None => {
                format!("{explorer}/v1/addresses/{addr}/boxes?unspent=true&limit={PAGE_LIMIT}")
            }
        };
        let (status, json) = get_json(client, &url).await?;

        if status != 200 {
            return Err(format!("unexpected status {status} from {url}"));
        }
        let items = json["items"]
            .as_array()
            .cloned()
            .ok_or("missing items array")?;
        for item in &items {
            let id = item["id"].as_str().ok_or("missing box ID")?;
            if !ids.insert(id.to_string()) {
                return Err("duplicate box ID".into());
            }
            if let Some(collector) = token_ids.as_deref_mut() {
                let tokens = item["tokens"].as_array().ok_or("missing box tokens")?;
                for tok in tokens {
                    if let Some(tid) = tok["id"].as_str() {
                        collector.insert(tid.to_string());
                    }
                }
            }
        }
        cursor = match json.get("next_cursor") {
            Some(Value::Null) => None,
            Some(Value::String(c)) => Some(c.clone()),
            _ => return Err("missing or invalid cursor".into()),
        };
        if cursor.is_none() {
            return Ok((ids, Capped::Complete));
        }
    }
    Ok((ids, Capped::Truncated))
}

/// Whether a cursor walk saw the whole set or stopped at [`MAX_PAGES`] with more to come.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Capped {
    Complete,
    Truncated,
}

/// Walks explorer `/v1/tokens/{id}/boxes?unspent=true`, counting items across every page.
///
/// As with [`explorer_unspent_ids`], [`Capped::Truncated`] means the count is a lower bound
/// (the walk hit [`MAX_PAGES`]), never a value to compare against the node's total.
async fn explorer_token_unspent_ids(
    client: &reqwest::Client,
    explorer: &str,
    token_id: &str,
) -> Result<(HashSet<String>, Capped), String> {
    let mut count = HashSet::new();
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let url = match &cursor {
            Some(c) => format!(
                "{explorer}/v1/tokens/{token_id}/boxes?unspent=true&limit={PAGE_LIMIT}&cursor={c}"
            ),
            None => {
                format!("{explorer}/v1/tokens/{token_id}/boxes?unspent=true&limit={PAGE_LIMIT}")
            }
        };
        let (status, json) = get_json(client, &url).await?;

        if status != 200 {
            return Err(format!("unexpected status {status} from {url}"));
        }
        let items = json["items"]
            .as_array()
            .cloned()
            .ok_or("missing items array")?;
        for id in strict_ids(&items, "id")? {
            if !count.insert(id) {
                return Err("duplicate token box ID".into());
            }
        }
        cursor = match json.get("next_cursor") {
            Some(Value::Null) => None,
            Some(Value::String(c)) => Some(c.clone()),
            _ => return Err("missing or invalid cursor".into()),
        };
        if cursor.is_none() {
            return Ok((count, Capped::Complete));
        }
    }
    Ok((count, Capped::Truncated))
}

/// GET the node's unspent-by-address list; `None` means the node returned exactly its own
/// cap ([`NODE_UNSPENT_CAP`]) worth of boxes, so the set may be a truncated prefix rather
/// than the whole set and can't be safely compared.
async fn node_unspent_ids(
    client: &reqwest::Client,
    node: &str,
    addr: &str,
) -> Result<Option<HashSet<String>>, String> {
    let url =
        format!("{node}/blockchain/box/unspent/byAddress/{addr}?offset=0&limit={NODE_UNSPENT_CAP}");
    let (status, json) = get_json(client, &url).await?;
    if status != 200 {
        return Err(format!("unexpected status {status} from {url}"));
    }
    let arr = json.as_array().cloned().ok_or("missing response array")?;
    if arr.len() == NODE_UNSPENT_CAP {
        return Ok(None);
    }
    Ok(Some(strict_ids(&arr, "boxId")?))
}

/// GET the node's unspent-by-token-id list; `None` means the node returned exactly its own
/// cap ([`NODE_UNSPENT_CAP`]) worth of boxes, so the count may be a truncated prefix rather
/// than the whole set — same convention as [`node_unspent_ids`], but only the count is
/// needed here, not the ids.
async fn node_token_unspent_ids(
    client: &reqwest::Client,
    node: &str,
    token_id: &str,
) -> Result<Option<HashSet<String>>, String> {
    let url = format!(
        "{node}/blockchain/box/unspent/byTokenId/{token_id}?offset=0&limit={NODE_UNSPENT_CAP}"
    );
    let (status, json) = get_json(client, &url).await?;
    if status != 200 {
        return Err(format!("unexpected status {status} from {url}"));
    }
    let arr = json.as_array().cloned().ok_or("missing response array")?;
    if arr.len() == NODE_UNSPENT_CAP {
        return Ok(None);
    }
    Ok(Some(strict_ids(&arr, "boxId")?))
}

/// Node's `confirmed.nanoErgs`. A missing or non-u64 field is a hard error — never coerced
/// to 0 — so it surfaces as "skipped", not as a silent (and wrong) balance of zero.
async fn node_balance_nano(
    client: &reqwest::Client,
    node: &str,
    addr: &str,
) -> Result<u64, String> {
    let url = format!("{node}/blockchain/balanceForAddress/{addr}");
    let (status, json) = get_json(client, &url).await?;
    if status != 200 {
        return Err(format!("unexpected status {status} from {url}"));
    }
    json["confirmed"]["nanoErgs"].as_u64().ok_or_else(|| {
        format!(
            "confirmed.nanoErgs missing or not a u64: {:?}",
            json["confirmed"]
        )
    })
}

/// Node's `.total`. Same hard-error treatment as [`node_balance_nano`].
async fn node_tx_total(client: &reqwest::Client, node: &str, addr: &str) -> Result<u64, String> {
    let url = format!("{node}/blockchain/transaction/byAddress/{addr}?offset=0&limit=1");
    let (status, json) = get_json(client, &url).await?;
    if status != 200 {
        return Err(format!("unexpected status {status} from {url}"));
    }
    json["total"]
        .as_u64()
        .ok_or_else(|| format!("total missing or not a u64: {:?}", json["total"]))
}

/// One address's worth of comparisons. Real disagreements go into `mismatches`; anything
/// that couldn't be compared at all (transport/status errors, unparseable fields, the node's
/// unspent-list cap) goes into `skipped` instead — only `mismatches` counts against the gate.
/// Runs the unspent-set comparison twice (30s apart) before recording a mismatch there,
/// since the node's default unspent view and the explorer's can legitimately disagree
/// transiently on mempool-spent boxes.
///
/// `token_ids` collects every token id seen on a box fetched during the unspent-set walk, so
/// the caller can sample [`TOKEN_SAMPLE_TARGET`] of them for [`compare_token`] without
/// issuing any requests beyond what this comparison already makes.
async fn compare_address(
    client: &reqwest::Client,
    explorer: &str,
    node: &str,
    addr: &str,
    evidence: &mut Evidence,
    token_ids: &mut HashSet<String>,
) {
    let Evidence {
        mismatches,
        skipped,
        passed,
    } = evidence;
    // --- balance + tx count ---
    let explorer_addr_url = format!("{explorer}/v1/addresses/{addr}");
    let explorer_get = get_json(client, &explorer_addr_url).await;
    let node_balance = node_balance_nano(client, node, addr).await;
    let node_tx_total_v = node_tx_total(client, node, addr).await;

    match explorer_get {
        Ok((404, _)) => {
            // Explorer has never seen this address. That's only a real disagreement if the
            // node thinks otherwise; a node query failure here just means we can't tell.
            let node_known = matches!(&node_balance, Ok(n) if *n > 0)
                || matches!(&node_tx_total_v, Ok(t) if *t > 0);
            if node_known {
                mismatches.push(Mismatch {
                    address: addr.to_string(),
                    field: "existence",
                    detail: "explorer 404s but node knows this address".to_string(),
                });
            } else if node_balance.is_err() || node_tx_total_v.is_err() {
                skipped.push(Skipped {
                    address: addr.to_string(),
                    reason: format!(
                        "explorer 404s and node existence check failed: balance={node_balance:?} tx_total={node_tx_total_v:?}"
                    ),
                });
            }
            return;
        }
        Ok((200, json)) => {
            let explorer_nano_raw = json["balance"]["nano"].as_str().map(|s| s.to_string());
            let explorer_nano = explorer_nano_raw
                .as_deref()
                .and_then(|s| s.parse::<u64>().ok());
            match (explorer_nano_raw, explorer_nano, &node_balance) {
                (_, Some(e), Ok(n)) if e != *n => {
                    mismatches.push(Mismatch {
                        address: addr.to_string(),
                        field: "balance.nano",
                        detail: format!("explorer={e} node={n}"),
                    });
                }
                (Some(raw), None, _) => skipped.push(Skipped {
                    address: addr.to_string(),
                    reason: format!("balance.nano: explorer value {raw:?} does not parse as u64"),
                }),
                (None, _, _) => skipped.push(Skipped {
                    address: addr.to_string(),
                    reason: format!(
                        "balance.nano: explorer response missing balance.nano: {:?}",
                        json["balance"]
                    ),
                }),
                (_, _, Err(e)) => skipped.push(Skipped {
                    address: addr.to_string(),
                    reason: format!("balance.nano: node query failed: {e}"),
                }),
                _ => passed.push("balance.nano"),
            }

            // `tx_count` now comes straight off the address response (Task 6), so unlike
            // `unspent_ids` below it needs no walk and no page-cap skip.
            let explorer_tx_count = json["tx_count"].as_u64();
            match (explorer_tx_count, &node_tx_total_v) {
                (Some(e), Ok(n)) if e != *n => mismatches.push(Mismatch {
                    address: addr.to_string(),
                    field: "tx_count",
                    detail: format!("explorer={e} node={n}"),
                }),
                (None, _) => skipped.push(Skipped {
                    address: addr.to_string(),
                    reason: format!(
                        "tx_count: explorer response missing tx_count: {:?}",
                        json.get("tx_count")
                    ),
                }),
                (_, Err(e)) => skipped.push(Skipped {
                    address: addr.to_string(),
                    reason: format!("tx_count: node query failed: {e}"),
                }),
                _ => passed.push("tx_count"),
            }
        }
        Ok((status, _)) => skipped.push(Skipped {
            address: addr.to_string(),
            reason: format!("existence: unexpected explorer status {status}"),
        }),
        Err(e) => skipped.push(Skipped {
            address: addr.to_string(),
            reason: format!("existence: explorer query failed: {e}"),
        }),
    }

    // --- unspent box-id set page-cap pre-check ---
    // The walk below is bounded by MAX_PAGES * PAGE_LIMIT items. If the node already says
    // this address has more txs than that, the walk can't produce a comparable answer, so
    // skip it rather than spend ~200 requests to reach a truncated result.
    if let Ok(total) = node_tx_total_v {
        if total > WALK_CAP {
            skipped.push(Skipped {
                address: addr.to_string(),
                reason: format!("unspent_ids: node reports {total} txs > page cap {WALK_CAP}"),
            });
            return;
        }
    }

    // --- unspent box-id set ---
    match node_unspent_ids(client, node, addr).await {
        Ok(None) => skipped.push(Skipped {
            address: addr.to_string(),
            reason: "unspent_ids: node returned exactly the cap; set may be truncated".to_string(),
        }),
        Ok(Some(node_ids)) => {
            match explorer_unspent_ids(client, explorer, addr, Some(token_ids)).await {
                Ok((_, Capped::Truncated)) => skipped.push(Skipped {
                    address: addr.to_string(),
                    reason: "unspent_ids: explorer page cap reached".to_string(),
                }),
                Ok((explorer_ids, Capped::Complete)) if explorer_ids != node_ids => {
                    // Possibly explained by a mempool spend racing the two queries —
                    // re-check once after a delay before treating it as a real mismatch.
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    let node_retry = node_unspent_ids(client, node, addr).await;
                    let explorer_retry = explorer_unspent_ids(client, explorer, addr, None).await;
                    match (&node_retry, &explorer_retry) {
                        (_, Ok((_, Capped::Truncated))) => skipped.push(Skipped {
                            address: addr.to_string(),
                            reason: "unspent_ids: explorer page cap reached on recheck"
                                .to_string(),
                        }),
                        (Ok(Some(n2)), Ok((e2, Capped::Complete))) => {
                            if n2 != e2 {
                                let (only_explorer, only_node): (Vec<_>, Vec<_>) = (
                                    e2.difference(n2).cloned().collect(),
                                    n2.difference(e2).cloned().collect(),
                                );
                                mismatches.push(Mismatch {
                                    address: addr.to_string(),
                                    field: "unspent_ids",
                                    detail: format!(
                                        "explorer_only={} node_only={} (persisted after 30s recheck)",
                                        only_explorer.len(),
                                        only_node.len()
                                    ),
                                });
                            } else { passed.push("unspent_ids"); }
                        }
                        (Ok(None), _) => skipped.push(Skipped {
                            address: addr.to_string(),
                            reason:
                                "unspent_ids: node returned exactly the cap on recheck; set may be truncated"
                                    .to_string(),
                        }),
                        (Err(e), _) => skipped.push(Skipped {
                            address: addr.to_string(),
                            reason: format!("unspent_ids: node recheck query failed: {e}"),
                        }),
                        (_, Err(e)) => skipped.push(Skipped {
                            address: addr.to_string(),
                            reason: format!("unspent_ids: explorer recheck query failed: {e}"),
                        }),
                    }
                }
                Ok(_) => passed.push("unspent_ids"),
                Err(e) => skipped.push(Skipped {
                    address: addr.to_string(),
                    reason: format!("unspent_ids: explorer query failed: {e}"),
                }),
            }
        }
        Err(e) => skipped.push(Skipped {
            address: addr.to_string(),
            reason: format!("unspent_ids: node query failed: {e}"),
        }),
    }
}

/// One token's worth of comparisons: emission, name, and unspent box count against the
/// node's `/blockchain/token/byId` and `/blockchain/box/unspent/byTokenId` endpoints.
///
/// This release gate requires full history: a sampled token 404 is a mismatch. Reuses
/// [`Mismatch`]/[`Skipped`] with `address` holding the token id, matching how address checks
/// report, per the brief's "same summary structure" requirement.
async fn compare_token(
    client: &reqwest::Client,
    explorer: &str,
    node: &str,
    token_id: &str,
    evidence: &mut Evidence,
) {
    let Evidence {
        mismatches,
        skipped,
        passed,
    } = evidence;
    let explorer_url = format!("{explorer}/v1/tokens/{token_id}");
    match get_json(client, &explorer_url).await {
        Ok((404, _)) => {
            mismatches.push(Mismatch {
                address: token_id.to_string(),
                field: "token.existence",
                detail: "sampled token missing from full-history explorer".into(),
            });
            return;
        }
        Ok((200, json)) => {
            match holder_sum(client, explorer, token_id).await.and_then(|sum| supply_agrees(&json, sum)) {
                Ok(true) => passed.push("token.supply_internal"),
                Ok(false) => mismatches.push(Mismatch { address: token_id.into(), field: "token.supply_internal", detail: "holder sum / supply / emission minus burned disagree (internal consistency)".into() }),
                Err(reason) => skipped.push(Skipped { address: token_id.into(), reason }),
            }
            let explorer_name = json["name"].as_str().map(|s| s.to_string());
            let explorer_emission_raw = json["emission"].as_str().map(|s| s.to_string());
            let explorer_emission = explorer_emission_raw
                .as_deref()
                .and_then(|s| s.parse::<u64>().ok());

            let node_url = format!("{node}/blockchain/token/byId/{token_id}");
            match get_json(client, &node_url).await {
                Ok((200, node_json)) => {
                    let node_name = node_json["name"].as_str().map(|s| s.to_string());
                    let node_emission = node_json["emissionAmount"].as_u64();

                    match (&explorer_emission_raw, explorer_emission, node_emission) {
                        (_, Some(e), Some(n)) if e != n => mismatches.push(Mismatch {
                            address: token_id.to_string(),
                            field: "token.emission",
                            detail: format!("explorer={e} node={n}"),
                        }),
                        (Some(raw), None, _) => skipped.push(Skipped {
                            address: token_id.to_string(),
                            reason: format!(
                                "token.emission: explorer value {raw:?} does not parse as u64"
                            ),
                        }),
                        (None, _, _) => skipped.push(Skipped {
                            address: token_id.to_string(),
                            reason: format!(
                                "token.emission: explorer response missing emission: {:?}",
                                json.get("emission")
                            ),
                        }),
                        (_, _, None) => skipped.push(Skipped {
                            address: token_id.to_string(),
                            reason: format!(
                                "token.emission: node emissionAmount missing or not a u64: {:?}",
                                node_json.get("emissionAmount")
                            ),
                        }),
                        _ => passed.push("token.emission"),
                    }

                    match (&explorer_name, &node_name) {
                        (Some(e), Some(n)) if e != n => mismatches.push(Mismatch {
                            address: token_id.to_string(),
                            field: "token.name",
                            detail: format!("explorer={e:?} node={n:?}"),
                        }),
                        (None, _) => skipped.push(Skipped {
                            address: token_id.to_string(),
                            reason: format!(
                                "token.name: explorer response missing name: {:?}",
                                json.get("name")
                            ),
                        }),
                        (_, None) => skipped.push(Skipped {
                            address: token_id.to_string(),
                            reason: format!(
                                "token.name: node response missing name: {:?}",
                                node_json.get("name")
                            ),
                        }),
                        _ => passed.push("token.name"),
                    }
                }
                Ok((status, _)) => skipped.push(Skipped {
                    address: token_id.to_string(),
                    reason: format!("token: unexpected node status {status}"),
                }),
                Err(e) => skipped.push(Skipped {
                    address: token_id.to_string(),
                    reason: format!("token: node query failed: {e}"),
                }),
            }
        }
        Ok((status, _)) => {
            skipped.push(Skipped {
                address: token_id.to_string(),
                reason: format!("token: unexpected explorer status {status}"),
            });
            return;
        }
        Err(e) => {
            skipped.push(Skipped {
                address: token_id.to_string(),
                reason: format!("token: explorer query failed: {e}"),
            });
            return;
        }
    }

    // --- unspent box count ---
    match node_token_unspent_ids(client, node, token_id).await {
        Ok(None) => skipped.push(Skipped {
            address: token_id.to_string(),
            reason: "token.unspent_box_ids: node returned exactly the cap; count may be truncated"
                .to_string(),
        }),
        Ok(Some(node_count)) => {
            match explorer_token_unspent_ids(client, explorer, token_id).await {
                Ok((_, Capped::Truncated)) => skipped.push(Skipped {
                    address: token_id.to_string(),
                    reason: "token.unspent_box_ids: explorer page cap reached".to_string(),
                }),
                Ok((explorer_count, Capped::Complete)) if explorer_count != node_count => {
                    mismatches.push(Mismatch {
                        address: token_id.to_string(),
                        field: "token.unspent_box_ids",
                        detail: format!("explorer={explorer_count:?} node={node_count:?}"),
                    })
                }
                Ok(_) => passed.push("token.unspent_box_ids"),
                Err(e) => skipped.push(Skipped {
                    address: token_id.to_string(),
                    reason: format!("token.unspent_box_ids: explorer query failed: {e}"),
                }),
            }
        }
        Err(e) => skipped.push(Skipped {
            address: token_id.to_string(),
            reason: format!("token.unspent_box_ids: node query failed: {e}"),
        }),
    }
}

/// Turns per-block sets of output addresses into the candidate sample, dropping addresses
/// that are not usefully comparable (see [`MINER_FEE_ADDRESS`] and
/// [`HOT_ADDRESS_BLOCK_THRESHOLD`] for why each exclusion exists).
///
/// Pure, so the exclusion rule is unit-tested without a live node.
fn select_sample_addresses(per_block: &[HashSet<String>]) -> Vec<String> {
    let mut blocks_seen_in: HashMap<&str, usize> = HashMap::new();
    for block in per_block {
        for addr in block {
            *blocks_seen_in.entry(addr.as_str()).or_insert(0) += 1;
        }
    }
    blocks_seen_in
        .into_iter()
        .filter(|(addr, blocks)| {
            *addr != MINER_FEE_ADDRESS && *blocks <= HOT_ADDRESS_BLOCK_THRESHOLD
        })
        .map(|(addr, _)| addr.to_string())
        .collect()
}

/// Fetches the last `BLOCKS_TO_SAMPLE` blocks' txs and collects distinct output addresses,
/// then applies [`select_sample_addresses`]'s exclusions.
async fn sample_addresses(client: &reqwest::Client, explorer: &str) -> Result<Vec<String>, String> {
    let (status, blocks) = get_json(
        client,
        &format!("{explorer}/v1/blocks?limit={BLOCKS_TO_SAMPLE}"),
    )
    .await?;
    if status != 200 {
        return Err(format!("unexpected status {status} from /v1/blocks"));
    }
    let items = blocks["items"].as_array().cloned().unwrap_or_default();
    let mut per_block: Vec<HashSet<String>> = Vec::new();
    for item in &items {
        let height = item["height"]
            .as_u64()
            .ok_or_else(|| "block item missing height".to_string())?;
        let (tx_status, txs) =
            get_json(client, &format!("{explorer}/v1/blocks/{height}/txs")).await?;
        if tx_status != 200 {
            return Err(format!(
                "unexpected status {tx_status} from /v1/blocks/{height}/txs"
            ));
        }
        let txs = txs.as_array().cloned().unwrap_or_default();
        let mut in_this_block: HashSet<String> = HashSet::new();
        for tx in &txs {
            let outputs = tx["outputs"].as_array().cloned().unwrap_or_default();
            for out in &outputs {
                if let Some(addr) = out["address"].as_str() {
                    in_this_block.insert(addr.to_string());
                }
            }
        }
        per_block.push(in_this_block);
    }
    Ok(select_sample_addresses(&per_block))
}

#[tokio::test]
#[ignore = "requires a live explorer + node at (roughly) the same height; run via scripts/parity.sh"]
async fn parity_against_node() {
    let explorer = match std::env::var("EXPLORER_URL") {
        Ok(v) => v,
        Err(_) => {
            panic!("parity: inconclusive — EXPLORER_URL not set");
        }
    };
    let node = match std::env::var("NODE_URL") {
        Ok(v) => v,
        Err(_) => {
            panic!("parity: inconclusive — NODE_URL not set");
        }
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .expect("build reqwest client");

    let (status_status, status_json) = get_json(&client, &format!("{explorer}/v1/status"))
        .await
        .expect("explorer status");
    assert_eq!(status_status, 200, "explorer /v1/status: {status_json}");
    let explorer_indexed = status_json["indexed"].as_u64();

    let (node_status, node_json) = get_json(&client, &format!("{node}/blockchain/indexedHeight"))
        .await
        .expect("node indexedHeight");
    assert_eq!(
        node_status, 200,
        "node /blockchain/indexedHeight: {node_json}"
    );
    let node_indexed = node_json["indexedHeight"].as_u64();

    assert!(
        explorer_indexed.is_some() && explorer_indexed == node_indexed,
        "parity: inconclusive — missing or mismatched indexed heights"
    );

    let anchor = snapshot(&client, &explorer, &node)
        .await
        .expect("parity: inconclusive — snapshot mismatch");
    let mut addresses = sample_addresses(&client, &explorer)
        .await
        .expect("sample addresses from recent blocks");
    println!(
        "parity: {} distinct output addresses collected",
        addresses.len()
    );
    deterministic_shuffle(&mut addresses, SHUFFLE_SEED);
    addresses.truncate(SAMPLE_TARGET);
    println!("parity: comparing {} addresses", addresses.len());

    let mut evidence = Evidence::default();
    let mut token_ids: HashSet<String> = HashSet::new();
    for (i, addr) in addresses.iter().enumerate() {
        let before = evidence.mismatches.len();
        compare_address(
            &client,
            &explorer,
            &node,
            addr,
            &mut evidence,
            &mut token_ids,
        )
        .await;
        println!(
            "parity: [{}/{}] {addr}: {} new mismatch(es)",
            i + 1,
            addresses.len(),
            evidence.mismatches.len() - before
        );
    }

    println!(
        "parity: {} distinct token ids collected off sampled boxes",
        token_ids.len()
    );
    let mut token_ids: Vec<String> = token_ids.into_iter().collect();
    deterministic_shuffle(&mut token_ids, SHUFFLE_SEED);
    token_ids.truncate(TOKEN_SAMPLE_TARGET);
    println!("parity: comparing {} tokens", token_ids.len());

    for (i, token_id) in token_ids.iter().enumerate() {
        let before = evidence.mismatches.len();
        compare_token(&client, &explorer, &node, token_id, &mut evidence).await;
        println!(
            "parity: [token {}/{}] {token_id}: {} new mismatch(es)",
            i + 1,
            token_ids.len(),
            evidence.mismatches.len() - before
        );
    }

    let coverage = coverage(&evidence, addresses.len(), token_ids.len());
    let final_anchor = snapshot(&client, &explorer, &node).await;
    let result = if final_anchor.as_ref().ok() == Some(&anchor) {
        verdict(&evidence, &coverage)
    } else {
        "inconclusive"
    };
    println!(
        "{}",
        serde_json::json!({
            "outcome": result, "coverage": coverage, "addresses": addresses, "tokens": token_ids,
            "explorer": explorer, "reference": node, "indexed_height": explorer_indexed,
            "full_history": true, "schema": 2, "commit": std::env::var("PARITY_COMMIT").ok(),
        "matched_tip": anchor, "final_tip": final_anchor.ok(),
            "mismatches": evidence.mismatches.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "skipped": evidence.skipped.iter().map(ToString::to_string).collect::<Vec<_>>()
        })
    );
    assert_eq!(result, "pass", "parity release gate: {result}");
}

#[test]
fn shuffle_is_deterministic_for_a_fixed_seed() {
    let mut a: Vec<u32> = (0..50).collect();
    let mut b: Vec<u32> = (0..50).collect();
    deterministic_shuffle(&mut a, SHUFFLE_SEED);
    deterministic_shuffle(&mut b, SHUFFLE_SEED);
    assert_eq!(a, b);
    // And it actually permutes (astronomically unlikely to be the identity by chance).
    assert_ne!(a, (0..50).collect::<Vec<u32>>());
}

#[test]
fn sample_excludes_the_fee_contract_and_addresses_in_too_many_blocks() {
    let mut per_block: Vec<HashSet<String>> = Vec::new();
    for i in 0..BLOCKS_TO_SAMPLE {
        let mut b = HashSet::new();
        b.insert(MINER_FEE_ADDRESS.to_string()); // in every block
        b.insert("hot".to_string()); // in every block
        if i < HOT_ADDRESS_BLOCK_THRESHOLD as u32 {
            b.insert("warm".to_string()); // in exactly the threshold many blocks
        }
        b.insert(format!("cold{i}")); // in one block each
        per_block.push(b);
    }
    let mut got = select_sample_addresses(&per_block);
    got.sort();

    assert!(!got.contains(&MINER_FEE_ADDRESS.to_string()));
    assert!(!got.contains(&"hot".to_string()));
    // The threshold is inclusive: "more than N blocks" is excluded, exactly N is kept.
    assert!(got.contains(&"warm".to_string()));
    assert_eq!(got.len(), BLOCKS_TO_SAMPLE as usize + 1);
}

#[test]
fn shuffle_is_independent_of_candidate_insertion_order() {
    let mut a = vec!["a", "b", "c", "d"];
    let mut b = vec!["d", "c", "b", "a"];
    deterministic_shuffle(&mut a, SHUFFLE_SEED);
    deterministic_shuffle(&mut b, SHUFFLE_SEED);
    assert_eq!(a, b);
}

#[test]
fn release_verdict_requires_coverage_and_rejects_missing_fields() {
    let mut e = Evidence::default();
    assert_eq!(verdict(&e, &coverage(&e, 200, 20)), "inconclusive");
    for (field, floor) in FIELDS {
        e.passed.extend(std::iter::repeat_n(field, floor));
    }
    assert_eq!(verdict(&e, &coverage(&e, 200, 20)), "pass");
    e.skipped.push(Skipped {
        address: "fixture".into(),
        reason: "missing required emission".into(),
    });
    assert_eq!(verdict(&e, &coverage(&e, 200, 20)), "inconclusive");
    e.mismatches.push(Mismatch {
        address: "fixture".into(),
        field: "token.existence",
        detail: "404 on full history".into(),
    });
    assert_eq!(verdict(&e, &coverage(&e, 200, 20)), "fail");
}

#[test]
fn box_id_sets_detect_substitution_missing_ids_and_duplicates() {
    let a = strict_ids(&[serde_json::json!({"id":"a"})], "id").unwrap();
    let b = strict_ids(&[serde_json::json!({"id":"b"})], "id").unwrap();
    assert_ne!(a, b);
    assert!(strict_ids(&[serde_json::json!({})], "id").is_err());
    assert!(strict_ids(
        &[serde_json::json!({"id":"a"}), serde_json::json!({"id":"a"})],
        "id"
    )
    .is_err());
}

#[test]
fn holder_mutation_and_supply_arithmetic_are_detected_without_rounding() {
    let token = serde_json::json!({ "emission": "9007199254740995", "burned": "2", "supply": "9007199254740993" });
    assert_eq!(supply_agrees(&token, 9007199254740993), Ok(true));
    assert_eq!(supply_agrees(&token, 9007199254740992), Ok(false));
    assert!(supply_agrees(&serde_json::json!({}), 0).is_err());
}
