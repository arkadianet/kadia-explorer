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
/// when its list is a truncated prefix rather than the whole set.
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

/// GET the node's unspent-by-address list; `None` means the node hit its own cap (the
/// returned array's length equals [`NODE_UNSPENT_CAP`]), so the set can't be compared.
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
    if arr.len() >= NODE_UNSPENT_CAP {
        return Ok(None);
    }
    let ids = arr
        .iter()
        .filter_map(|b| b["boxId"].as_str().map(|s| s.to_string()))
        .collect();
    Ok(Some(ids))
}

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
    Ok(json["confirmed"]["nanoErgs"].as_u64().unwrap_or(0))
}

async fn node_tx_total(client: &reqwest::Client, node: &str, addr: &str) -> Result<u64, String> {
    let url = format!("{node}/blockchain/transaction/byAddress/{addr}?offset=0&limit=1");
    let (status, json) = get_json(client, &url).await?;
    if status != 200 {
        return Err(format!("unexpected status {status} from {url}"));
    }
    Ok(json["total"].as_u64().unwrap_or(0))
}

/// One address's worth of comparisons, appended into `mismatches`. Runs the unspent-set
/// comparison twice (30s apart) before recording a mismatch there, since the node's default
/// unspent view and the explorer's can legitimately disagree transiently on mempool-spent
/// boxes.
async fn compare_address(
    client: &reqwest::Client,
    explorer: &str,
    node: &str,
    addr: &str,
    mismatches: &mut Vec<Mismatch>,
) {
    // --- balance ---
    let explorer_addr_url = format!("{explorer}/v1/addresses/{addr}");
    let explorer_get = get_json(client, &explorer_addr_url).await;
    let node_balance = node_balance_nano(client, node, addr).await;
    let node_tx_total_v = node_tx_total(client, node, addr).await;

    match explorer_get {
        Ok((404, _)) => {
            // Explorer has never seen this address. That's only a mismatch if the node
            // thinks otherwise.
            let node_known = matches!(&node_balance, Ok(n) if *n > 0)
                || matches!(&node_tx_total_v, Ok(t) if *t > 0);
            if node_known {
                mismatches.push(Mismatch {
                    address: addr.to_string(),
                    field: "existence",
                    detail: "explorer 404s but node knows this address".to_string(),
                });
            }
            return;
        }
        Ok((200, json)) => {
            let explorer_nano = json["balance"]["nano"]
                .as_str()
                .and_then(|s| s.parse::<u64>().ok());
            match (explorer_nano, node_balance) {
                (Some(e), Ok(n)) if e != n => {
                    mismatches.push(Mismatch {
                        address: addr.to_string(),
                        field: "balance.nano",
                        detail: format!("explorer={e} node={n}"),
                    });
                }
                (None, _) => mismatches.push(Mismatch {
                    address: addr.to_string(),
                    field: "balance.nano",
                    detail: format!("explorer balance.nano unparseable: {:?}", json["balance"]),
                }),
                (_, Err(e)) => mismatches.push(Mismatch {
                    address: addr.to_string(),
                    field: "balance.nano",
                    detail: format!("node query failed: {e}"),
                }),
                _ => {}
            }
        }
        Ok((status, _)) => mismatches.push(Mismatch {
            address: addr.to_string(),
            field: "existence",
            detail: format!("unexpected explorer status {status}"),
        }),
        Err(e) => mismatches.push(Mismatch {
            address: addr.to_string(),
            field: "existence",
            detail: format!("explorer query failed: {e}"),
        }),
    }

    // --- unspent box-id set ---
    match node_unspent_ids(client, node, addr).await {
        Ok(None) => mismatches.push(Mismatch {
            address: addr.to_string(),
            field: "unspent_ids",
            detail: "skipped: too many boxes (node cap hit)".to_string(),
        }),
        Ok(Some(node_ids)) => {
            let explorer_ids = match explorer_unspent_ids(client, explorer, addr).await {
                Ok(ids) => ids,
                Err(e) => {
                    mismatches.push(Mismatch {
                        address: addr.to_string(),
                        field: "unspent_ids",
                        detail: format!("explorer query failed: {e}"),
                    });
                    HashSet::new()
                }
            };
            if explorer_ids != node_ids {
                // Possibly explained by a mempool spend racing the two queries — re-check
                // once after a delay before treating it as a real mismatch.
                tokio::time::sleep(Duration::from_secs(30)).await;
                let node_ids_2 = node_unspent_ids(client, node, addr).await.ok().flatten();
                let explorer_ids_2 = explorer_unspent_ids(client, explorer, addr).await.ok();
                let still_mismatched = match (&node_ids_2, &explorer_ids_2) {
                    (Some(n2), Some(e2)) => n2 != e2,
                    _ => true,
                };
                if still_mismatched {
                    let (only_explorer, only_node): (Vec<_>, Vec<_>) = (
                        explorer_ids.difference(&node_ids).cloned().collect(),
                        node_ids.difference(&explorer_ids).cloned().collect(),
                    );
                    mismatches.push(Mismatch {
                        address: addr.to_string(),
                        field: "unspent_ids",
                        detail: format!(
                            "explorer_only={} node_only={} (first mismatch pass)",
                            only_explorer.len(),
                            only_node.len()
                        ),
                    });
                }
            }
        }
        Err(e) => mismatches.push(Mismatch {
            address: addr.to_string(),
            field: "unspent_ids",
            detail: format!("node query failed: {e}"),
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
        (Err(e), _) => mismatches.push(Mismatch {
            address: addr.to_string(),
            field: "tx_count",
            detail: format!("explorer query failed: {e}"),
        }),
        (_, Err(e)) => mismatches.push(Mismatch {
            address: addr.to_string(),
            field: "tx_count",
            detail: format!("node query failed: {e}"),
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
    for (i, addr) in addresses.iter().enumerate() {
        let before = mismatches.len();
        compare_address(&client, &explorer, &node, addr, &mut mismatches).await;
        println!(
            "parity: [{}/{}] {addr}: {} new mismatch(es)",
            i + 1,
            addresses.len(),
            mismatches.len() - before
        );
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
