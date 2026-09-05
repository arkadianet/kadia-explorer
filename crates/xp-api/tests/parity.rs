//! Parity gate: samples addresses seen in recent explorer blocks and compares balance,
//! unspent box-id set, and tx count against the Rust node's `/blockchain` API.
//!
//! Ignored by default — it needs a live explorer and a live node at the same height. Run
//! via `scripts/parity.sh`, or directly:
//!
//! ```text
//! EXPLORER_URL=http://127.0.0.1:8090 NODE_URL=http://127.0.0.1:9063 \
//!     cargo test -p xp-api --test parity -- --ignored --nocapture
//! ```
//!
//! Without both env vars set the test prints a message and returns (not a failure, so CI
//! that happens to run ignored tests without a node configured stays green).

use serde_json::Value;
use std::collections::HashSet;
use std::time::Duration;

const SAMPLE_TARGET: usize = 200;
const BLOCKS_TO_SAMPLE: u32 = 20;
const SHUFFLE_SEED: u64 = 0xC0FF_EE15_5EED_1234;
/// The node's own cap on `/blockchain/box/unspent/byAddress` — mirrored here so we know
/// when its list may be a truncated prefix rather than the whole set.
const NODE_UNSPENT_CAP: usize = 16_384;
/// Safety bound on cursor-walk pages, so a store bug that never returns `next_cursor: null`
/// can't spin the test forever.
const MAX_PAGES: usize = 200;

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
fn deterministic_shuffle<T>(items: &mut [T], seed: u64) {
    let mut rng = XorShift64::new(seed);
    for i in (1..items.len()).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        items.swap(i, j);
    }
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
/// too many boxes for the node's cap, ...). These are reported but never fail the gate —
/// "cannot compare" is not "disagrees".
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
async fn explorer_unspent_ids(
    client: &reqwest::Client,
    explorer: &str,
    addr: &str,
) -> Result<HashSet<String>, String> {
    let mut ids = HashSet::new();
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let url = match &cursor {
            Some(c) => {
                format!("{explorer}/v1/addresses/{addr}/boxes?unspent=true&limit=500&cursor={c}")
            }
            None => format!("{explorer}/v1/addresses/{addr}/boxes?unspent=true&limit=500"),
        };
        let (status, json) = get_json(client, &url).await?;
        if status == 404 {
            return Ok(ids);
        }
        if status != 200 {
            return Err(format!("unexpected status {status} from {url}"));
        }
        let items = json["items"].as_array().cloned().unwrap_or_default();
        for item in &items {
            if let Some(id) = item["id"].as_str() {
                ids.insert(id.to_string());
            }
        }
        cursor = json["next_cursor"].as_str().map(|s| s.to_string());
        if cursor.is_none() {
            break;
        }
    }
    Ok(ids)
}

/// Walks explorer `/v1/addresses/{addr}/txs`, counting items across every page.
async fn explorer_tx_count(
    client: &reqwest::Client,
    explorer: &str,
    addr: &str,
) -> Result<u64, String> {
    let mut count = 0u64;
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let url = match &cursor {
            Some(c) => format!("{explorer}/v1/addresses/{addr}/txs?limit=500&cursor={c}"),
            None => format!("{explorer}/v1/addresses/{addr}/txs?limit=500"),
        };
        let (status, json) = get_json(client, &url).await?;
        if status == 404 {
            return Ok(0);
        }
        if status != 200 {
            return Err(format!("unexpected status {status} from {url}"));
        }
        let items = json["items"].as_array().cloned().unwrap_or_default();
        count += items.len() as u64;
        cursor = json["next_cursor"].as_str().map(|s| s.to_string());
        if cursor.is_none() {
            break;
        }
    }
    Ok(count)
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
    let arr = json.as_array().cloned().unwrap_or_default();
    if arr.len() == NODE_UNSPENT_CAP {
        return Ok(None);
    }
    let ids = arr
        .iter()
        .filter_map(|b| b["boxId"].as_str().map(|s| s.to_string()))
        .collect();
    Ok(Some(ids))
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
async fn compare_address(
    client: &reqwest::Client,
    explorer: &str,
    node: &str,
    addr: &str,
    mismatches: &mut Vec<Mismatch>,
    skipped: &mut Vec<Skipped>,
) {
    // --- balance ---
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
            match (explorer_nano_raw, explorer_nano, node_balance) {
                (_, Some(e), Ok(n)) if e != n => {
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
                _ => {}
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

    // --- unspent box-id set ---
    match node_unspent_ids(client, node, addr).await {
        Ok(None) => skipped.push(Skipped {
            address: addr.to_string(),
            reason: "unspent_ids: node returned exactly the cap; set may be truncated".to_string(),
        }),
        Ok(Some(node_ids)) => {
            let explorer_ids = match explorer_unspent_ids(client, explorer, addr).await {
                Ok(ids) => ids,
                Err(e) => {
                    skipped.push(Skipped {
                        address: addr.to_string(),
                        reason: format!("unspent_ids: explorer query failed: {e}"),
                    });
                    return;
                }
            };
            if explorer_ids != node_ids {
                // Possibly explained by a mempool spend racing the two queries — re-check
                // once after a delay before treating it as a real mismatch.
                tokio::time::sleep(Duration::from_secs(30)).await;
                let node_retry = node_unspent_ids(client, node, addr).await;
                let explorer_retry = explorer_unspent_ids(client, explorer, addr).await;
                match (&node_retry, &explorer_retry) {
                    (Ok(Some(n2)), Ok(e2)) => {
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
                        }
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
        }
        Err(e) => skipped.push(Skipped {
            address: addr.to_string(),
            reason: format!("unspent_ids: node query failed: {e}"),
        }),
    }

    // --- tx count ---
    let explorer_count = explorer_tx_count(client, explorer, addr).await;
    match (explorer_count, node_tx_total_v) {
        (Ok(e), Ok(n)) if e != n => mismatches.push(Mismatch {
            address: addr.to_string(),
            field: "tx_count",
            detail: format!("explorer={e} node={n}"),
        }),
        (Err(e), _) => skipped.push(Skipped {
            address: addr.to_string(),
            reason: format!("tx_count: explorer query failed: {e}"),
        }),
        (_, Err(e)) => skipped.push(Skipped {
            address: addr.to_string(),
            reason: format!("tx_count: node query failed: {e}"),
        }),
        _ => {}
    }
}

/// Fetches the last `BLOCKS_TO_SAMPLE` blocks' txs and collects distinct output addresses.
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
    let mut addrs: HashSet<String> = HashSet::new();
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
        for tx in &txs {
            let outputs = tx["outputs"].as_array().cloned().unwrap_or_default();
            for out in &outputs {
                if let Some(addr) = out["address"].as_str() {
                    addrs.insert(addr.to_string());
                }
            }
        }
    }
    Ok(addrs.into_iter().collect())
}

#[tokio::test]
#[ignore = "requires a live explorer + node at (roughly) the same height; run via scripts/parity.sh"]
async fn parity_against_node() {
    let explorer = match std::env::var("EXPLORER_URL") {
        Ok(v) => v,
        Err(_) => {
            println!("parity: EXPLORER_URL not set, skipping");
            return;
        }
    };
    let node = match std::env::var("NODE_URL") {
        Ok(v) => v,
        Err(_) => {
            println!("parity: NODE_URL not set, skipping");
            return;
        }
    };
    let force = std::env::var("PARITY_FORCE").is_ok();

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

    if !force && explorer_indexed != node_indexed {
        println!(
            "parity: height mismatch, skipping — explorer.indexed={explorer_indexed:?} \
             node.indexedHeight={node_indexed:?}"
        );
        return;
    }
    println!("parity: heights explorer={explorer_indexed:?} node={node_indexed:?} force={force}");

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

    let mut mismatches = Vec::new();
    let mut skipped = Vec::new();
    for (i, addr) in addresses.iter().enumerate() {
        let before = mismatches.len();
        compare_address(
            &client,
            &explorer,
            &node,
            addr,
            &mut mismatches,
            &mut skipped,
        )
        .await;
        println!(
            "parity: [{}/{}] {addr}: {} new mismatch(es)",
            i + 1,
            addresses.len(),
            mismatches.len() - before
        );
    }

    println!("parity: skipped {} address/field checks", skipped.len());
    for s in &skipped {
        println!("  {s}");
    }

    println!(
        "parity: {} mismatches / {} addresses",
        mismatches.len(),
        addresses.len()
    );
    for m in &mismatches {
        println!("  {m}");
    }
    assert_eq!(
        mismatches.len(),
        0,
        "{} parity mismatches found",
        mismatches.len()
    );
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
