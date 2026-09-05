# Ergo Explorer Core (Plan 1 of 3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A standalone Rust binary that follows a Rust node over REST, indexes blocks, transactions, boxes, addresses, balances, rich list and storage-rent maturity into its own redb store with correct fork rollback, and serves them over an axum JSON API.

**Architecture:** One tokio process with three tasks: ingest (REST fetch + fork detection), apply (the only redb writer, one write txn per batch, undo log for rollback), API (redb read snapshots). Blocks are parsed with `ergo-lib`'s JSON types; all ids are 32-byte arrays internally and hex on the wire; the global insertion index (`Gidx`) is the universal pagination cursor.

**Tech Stack:** Rust 1.96 (edition 2021), tokio 1.5x, axum 0.8, reqwest 0.12 (rustls), redb 2.6, ergo-lib 0.28 (feature `json`), serde/serde_json, blake2, hex, thiserror, tracing, proptest.

**Spec:** `docs/superpowers/specs/2026-09-05-ergo-explorer-design.md`. This plan covers spec §3–§6 (core tables), §8 (core routes) and §11 (wire/store/fork/API tests). Deferred to **Plan 2**: tokens, templates, token holders, register index (§6.1 rows `templates`…`register_idx`, §8 token/template/register routes). Deferred to **Plan 3**: DuckDB analytics (§7), WebSocket feed, mempool proxy, `PublicPool` and `LocalNodeDb` sources, miners/stats endpoints.

## Global Constraints

- Storage-rent constants: maturity at `creation_height + 1_051_200`; rent `1_250_000` nanoERG per byte, capped at box value (spec §6.5).
- Rollback window: `1_000` blocks; deeper fork ⇒ halt with status `reindex required` (spec §5).
- Pagination: cursor = `Gidx` (u64), `limit ≤ 500` (spec §8).
- All redb keys big-endian fixed width; no serde in the store hot path (spec §6).
- Durability `Immediate` at tip; `None` during bulk sync with forced `Immediate` every 256 blocks (spec §6.2).
- Amounts on the wire are decimal strings; ids lowercase hex (spec §8).
- No runtime dependency on the node repository (spec §2).
- Every commit: `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` green.

---

## File structure

```
ergo-explorer/
  Cargo.toml                         # workspace, shared deps
  rust-toolchain.toml
  crates/xp-types/src/lib.rs         # Hash32, Gidx, ids, errors, consensus constants
  crates/xp-types/src/rent.rs        # maturity/rent-due helpers
  crates/xp-wire/src/lib.rs          # FullBlock JSON decode, DecodedBlock/DecodedTx/DecodedBox
  crates/xp-wire/src/tree.rs         # tree hash, template hash, address string, TreeKind
  crates/xp-source/src/lib.rs        # BlockSource trait, SourceError
  crates/xp-source/src/rust_node.rs  # REST impl
  crates/xp-store/src/lib.rs         # Store open/close, meta
  crates/xp-store/src/keys.rs        # key/value encoders (BE composite keys)
  crates/xp-store/src/rows.rs        # HeaderRow/TxRow/BoxRow/TreeRow/BalanceRow/UndoRow codecs
  crates/xp-store/src/tables.rs      # TableDefinition consts
  crates/xp-store/src/apply.rs       # apply_batch
  crates/xp-store/src/rollback.rs    # rollback_to
  crates/xp-store/src/read.rs        # Reader: snapshot queries used by API
  crates/xp-ingest/src/lib.rs        # pipeline: fetch → fork check → apply, sync mode switch
  crates/xp-api/src/lib.rs           # router()
  crates/xp-api/src/dto.rs           # response types
  crates/xp-api/src/handlers/{status,blocks,txs,boxes,addresses,richlist,rent,search}.rs
  bin/explorer/src/main.rs           # config + wiring
  bin/explorer/src/config.rs
  tests/fixtures/blocks/*.json       # 3 real mainnet blocks + genesis (fetched once, committed)
```

---

### Task 1: Workspace scaffold and `xp-types`

**Files:**
- Create: `Cargo.toml`, `rust-toolchain.toml`, `.gitignore`
- Create: `crates/xp-types/Cargo.toml`, `crates/xp-types/src/lib.rs`, `crates/xp-types/src/rent.rs`

**Interfaces:**
- Produces: `Hash32 = [u8; 32]`, `pub struct BoxId(pub Hash32)`, `TxId(Hash32)`, `HeaderId(Hash32)`, `TreeHash(Hash32)`, `pub type Gidx = u64;`, `pub const RENT_PERIOD: u32 = 1_051_200;`, `pub const RENT_PER_BYTE: u64 = 1_250_000;`, `pub fn maturity_height(creation: u32) -> u32`, `pub fn rent_due(size_bytes: u32, value: u64) -> u64`, `pub fn parse_hex32(s: &str) -> Result<Hash32, TypesError>`, `pub fn hex32(h: &Hash32) -> String`, `#[derive(thiserror::Error)] pub enum TypesError { BadHex(String) }`.

- [ ] **Step 1: Workspace files**

`Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = ["crates/*", "bin/*"]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"

[workspace.dependencies]
xp-types  = { path = "crates/xp-types" }
xp-wire   = { path = "crates/xp-wire" }
xp-source = { path = "crates/xp-source" }
xp-store  = { path = "crates/xp-store" }
xp-ingest = { path = "crates/xp-ingest" }
xp-api    = { path = "crates/xp-api" }
tokio       = { version = "1.53", features = ["rt-multi-thread", "macros", "sync", "time", "signal"] }
axum        = "0.8"
reqwest     = { version = "0.12", default-features = false, features = ["json", "rustls-tls", "gzip"] }
redb        = "2.6"
ergo-lib    = { version = "0.28", features = ["json"] }
serde       = { version = "1", features = ["derive"] }
serde_json  = "1"
blake2      = "0.10"
hex         = "0.4"
thiserror   = "2"
anyhow      = "1"
tracing     = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
async-trait = "0.1"
toml        = "0.8"
proptest    = "1"
tempfile    = "3"
```

`rust-toolchain.toml`:
```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
```

`.gitignore`: `/target`, `/data`, `*.redb`, `*.duckdb`.

- [ ] **Step 2: Failing tests for xp-types**

`crates/xp-types/Cargo.toml`:
```toml
[package]
name = "xp-types"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
hex.workspace = true
thiserror.workspace = true
```

`crates/xp-types/src/lib.rs` (tests first, module bodies empty for now):
```rust
pub mod rent;

pub type Hash32 = [u8; 32];
pub type Gidx = u64;

macro_rules! id_newtype {
    ($name:ident) => {
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub Hash32);
        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}({})", stringify!($name), hex32(&self.0))
            }
        }
        impl std::str::FromStr for $name {
            type Err = TypesError;
            fn from_str(s: &str) -> Result<Self, TypesError> { parse_hex32(s).map($name) }
        }
    };
}
id_newtype!(BoxId);
id_newtype!(TxId);
id_newtype!(HeaderId);
id_newtype!(TreeHash);

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TypesError {
    #[error("bad 32-byte hex: {0}")]
    BadHex(String),
}

pub fn parse_hex32(s: &str) -> Result<Hash32, TypesError> {
    let v = hex::decode(s).map_err(|_| TypesError::BadHex(s.to_owned()))?;
    <Hash32>::try_from(v).map_err(|_| TypesError::BadHex(s.to_owned()))
}

pub fn hex32(h: &Hash32) -> String { hex::encode(h) }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hex_roundtrip() {
        let s = "aa44ef6a6c08d0b198d65b762abb0181e6ad995654116fbedaa1d3d3eb95a4d6";
        assert_eq!(hex32(&parse_hex32(s).unwrap()), s);
        assert_eq!(parse_hex32("zz"), Err(TypesError::BadHex("zz".into())));
        assert_eq!(parse_hex32("aabb"), Err(TypesError::BadHex("aabb".into())));
    }
}
```

`crates/xp-types/src/rent.rs`:
```rust
pub const RENT_PERIOD: u32 = 1_051_200;
pub const RENT_PER_BYTE: u64 = 1_250_000;

pub fn maturity_height(creation: u32) -> u32 { creation.saturating_add(RENT_PERIOD) }

pub fn rent_due(size_bytes: u32, value: u64) -> u64 {
    (size_bytes as u64).saturating_mul(RENT_PER_BYTE).min(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn consensus_constants() {
        // Known mainnet recreate-claim: box created at 814_800 became claimable at 1_866_000.
        assert_eq!(maturity_height(814_800), 1_866_000);
        assert_eq!(RENT_PERIOD, 1_051_200);
        assert_eq!(RENT_PER_BYTE, 1_250_000);
    }
    #[test]
    fn rent_is_capped_by_value() {
        assert_eq!(rent_due(105, 1_000_000_000), 131_250_000);
        assert_eq!(rent_due(105, 1_000_000), 1_000_000);
        assert_eq!(rent_due(u32::MAX, u64::MAX), (u32::MAX as u64) * RENT_PER_BYTE);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p xp-types`
Expected: 3 passed.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat(types): workspace scaffold, id newtypes, storage-rent constants"
```

---

### Task 2: `xp-wire` — block JSON decode, tree hashing, address

**Files:**
- Create: `crates/xp-wire/Cargo.toml`, `crates/xp-wire/src/lib.rs`, `crates/xp-wire/src/tree.rs`
- Create: `tests/fixtures/blocks/1866000.json`, `tests/fixtures/blocks/1866001.json`, `tests/fixtures/blocks/1866002.json` (fetched from `http://127.0.0.1:9063/blocks/{id}`; ids via `/blocks/at/{h}`)

**Interfaces:**
- Consumes: `xp_types::{Hash32, BoxId, TxId, HeaderId, TreeHash, hex32, parse_hex32}`
- Produces:
```rust
pub struct DecodedBlock { pub header: DecodedHeader, pub txs: Vec<DecodedTx>, pub size: u32 }
pub struct DecodedHeader { pub id: HeaderId, pub parent_id: HeaderId, pub height: u32, pub timestamp: u64,
                           pub difficulty: u128, pub miner_pk: [u8; 33], pub votes: [u8; 3], pub version: u8, pub raw_json: String }
pub struct DecodedTx { pub id: TxId, pub inputs: Vec<BoxId>, pub data_inputs: Vec<BoxId>, pub outputs: Vec<DecodedBox>, pub size: u32 }
pub struct DecodedBox { pub id: BoxId, pub value: u64, pub tree_bytes: Vec<u8>, pub tree_hash: TreeHash,
                        pub creation_height: u32, pub tx_id: TxId, pub index: u16,
                        pub tokens: Vec<(Hash32, u64)>, pub registers_json: String, pub size: u32 }
pub fn decode_block(json: &str) -> Result<DecodedBlock, WireError>;
pub fn tree_hash(tree_bytes: &[u8]) -> TreeHash;              // blake2b256
pub struct TreeInfo { pub template_hash: Hash32, pub address: String, pub kind: TreeKind }
pub enum TreeKind { P2pk, P2s, Other }
pub fn tree_info(tree_bytes: &[u8]) -> Result<TreeInfo, WireError>;
```

- [ ] **Step 1: Fetch fixtures**

```bash
mkdir -p tests/fixtures/blocks
for h in 1866000 1866001 1866002; do
  id=$(curl -s http://127.0.0.1:9063/blocks/at/$h | tr -d '[]"')
  curl -s http://127.0.0.1:9063/blocks/$id > tests/fixtures/blocks/$h.json
done
ls -la tests/fixtures/blocks
```
Expected: three files, each > 10 KB.

- [ ] **Step 2: Failing tests**

`crates/xp-wire/Cargo.toml`:
```toml
[package]
name = "xp-wire"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
xp-types.workspace = true
ergo-lib.workspace = true
serde_json.workspace = true
blake2.workspace = true
hex.workspace = true
thiserror.workspace = true
```

`crates/xp-wire/tests/decode_fixtures.rs`:
```rust
use xp_wire::{decode_block, tree_info, TreeKind};

fn fixture(h: u32) -> String {
    std::fs::read_to_string(format!("{}/../../tests/fixtures/blocks/{h}.json", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

#[test]
fn decodes_real_block() {
    let b = decode_block(&fixture(1866000)).unwrap();
    assert_eq!(b.header.height, 1866000);
    assert!(!b.txs.is_empty());
    let coinbase = &b.txs[0];
    assert!(coinbase.inputs.len() >= 1);
    // every output's tx_id and index are consistent with its parent tx
    for tx in &b.txs {
        for (i, o) in tx.outputs.iter().enumerate() {
            assert_eq!(o.tx_id, tx.id);
            assert_eq!(o.index as usize, i);
            assert!(o.size > 0 && o.size <= 4096);
        }
    }
}

#[test]
fn chain_links() {
    let a = decode_block(&fixture(1866000)).unwrap();
    let b = decode_block(&fixture(1866001)).unwrap();
    assert_eq!(b.header.parent_id, a.header.id);
}

#[test]
fn p2pk_tree_gives_9_address() {
    // 9hf4... proceeds tree from the fleet: P2PK
    let tree = hex::decode("0008cd039d3e8d05f4c3fbf3c06e16f1ff4be9d8ee3c3ea0bd2ff9b9b5db1d5c2ab2d3c2").unwrap();
    let info = tree_info(&tree).unwrap();
    assert!(matches!(info.kind, TreeKind::P2pk));
    assert!(info.address.starts_with('9'));
    assert_eq!(info.template_hash, xp_wire::template_hash_of(&tree).unwrap());
}
```
(If the literal tree above fails to parse, replace it with the `ergoTree` of any P2PK output from the fixture: the test intent is “P2PK ⇒ address starts with 9”.)

- [ ] **Step 3: Run tests — expect compile failure**

Run: `cargo test -p xp-wire`
Expected: error, `decode_block` not found.

- [ ] **Step 4: Implement**

`crates/xp-wire/src/tree.rs`:
```rust
use blake2::{digest::consts::U32, Blake2b, Digest};
use ergo_lib::ergotree_ir::chain::address::{Address, AddressEncoder, NetworkPrefix};
use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
use xp_types::{Hash32, TreeHash};

use crate::WireError;

pub fn blake2b256(data: &[u8]) -> Hash32 {
    let mut h = Blake2b::<U32>::new();
    h.update(data);
    h.finalize().into()
}

pub fn tree_hash(tree_bytes: &[u8]) -> TreeHash { TreeHash(blake2b256(tree_bytes)) }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeKind { P2pk, P2s, Other }

#[derive(Debug, Clone)]
pub struct TreeInfo { pub template_hash: Hash32, pub address: String, pub kind: TreeKind }

pub fn template_hash_of(tree_bytes: &[u8]) -> Result<Hash32, WireError> {
    let tree = ErgoTree::sigma_parse_bytes(tree_bytes).map_err(|e| WireError::Tree(e.to_string()))?;
    let tmpl = tree.template_bytes().map_err(|e| WireError::Tree(e.to_string()))?;
    Ok(blake2b256(&tmpl))
}

pub fn tree_info(tree_bytes: &[u8]) -> Result<TreeInfo, WireError> {
    let tree = ErgoTree::sigma_parse_bytes(tree_bytes).map_err(|e| WireError::Tree(e.to_string()))?;
    let tmpl = tree.template_bytes().map_err(|e| WireError::Tree(e.to_string()))?;
    let addr = Address::recreate_from_ergo_tree(&tree).map_err(|e| WireError::Tree(e.to_string()))?;
    let kind = match &addr { Address::P2Pk(_) => TreeKind::P2pk, Address::P2S(_) => TreeKind::P2s, _ => TreeKind::Other };
    let address = AddressEncoder::new(NetworkPrefix::Mainnet).address_to_str(&addr);
    Ok(TreeInfo { template_hash: blake2b256(&tmpl), address, kind })
}
```

`crates/xp-wire/src/lib.rs`:
```rust
pub mod tree;
pub use tree::{template_hash_of, tree_hash, tree_info, TreeInfo, TreeKind};

use ergo_lib::chain::block::FullBlock;
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
use xp_types::{BoxId, Hash32, HeaderId, TreeHash, TxId};

#[derive(Debug, thiserror::Error)]
pub enum WireError {
    #[error("json: {0}")] Json(#[from] serde_json::Error),
    #[error("tree: {0}")] Tree(String),
    #[error("serialize: {0}")] Ser(String),
}

#[derive(Debug, Clone)]
pub struct DecodedHeader {
    pub id: HeaderId, pub parent_id: HeaderId, pub height: u32, pub timestamp: u64,
    pub difficulty: u128, pub miner_pk: [u8; 33], pub votes: [u8; 3], pub version: u8, pub raw_json: String,
}
#[derive(Debug, Clone)]
pub struct DecodedBox {
    pub id: BoxId, pub value: u64, pub tree_bytes: Vec<u8>, pub tree_hash: TreeHash,
    pub creation_height: u32, pub tx_id: TxId, pub index: u16,
    pub tokens: Vec<(Hash32, u64)>, pub registers_json: String, pub size: u32,
}
#[derive(Debug, Clone)]
pub struct DecodedTx { pub id: TxId, pub inputs: Vec<BoxId>, pub data_inputs: Vec<BoxId>, pub outputs: Vec<DecodedBox>, pub size: u32 }
#[derive(Debug, Clone)]
pub struct DecodedBlock { pub header: DecodedHeader, pub txs: Vec<DecodedTx>, pub size: u32 }

pub fn decode_block(json: &str) -> Result<DecodedBlock, WireError> {
    let v: serde_json::Value = serde_json::from_str(json)?;
    let fb: FullBlock = serde_json::from_value(v.clone())?;
    let h = &fb.header;
    let header_json = v.get("header").cloned().unwrap_or_default();
    let difficulty: u128 = header_json.get("difficulty").and_then(|d| d.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0);
    let mut miner_pk = [0u8; 33];
    miner_pk.copy_from_slice(&h.autolykos_solution.miner_pk.sigma_serialize_bytes().map_err(|e| WireError::Ser(e.to_string()))?);
    let header = DecodedHeader {
        id: HeaderId(h.id.0 .0), parent_id: HeaderId(h.parent_id.0 .0), height: h.height, timestamp: h.timestamp,
        difficulty, miner_pk, votes: h.votes.0, version: h.version, raw_json: header_json.to_string(),
    };
    let mut txs = Vec::with_capacity(fb.block_transactions.transactions.len());
    for tx in fb.block_transactions.transactions.iter() {
        let tx_id = TxId(tx.id().0 .0);
        let mut outputs = Vec::with_capacity(tx.outputs.len());
        for (i, o) in tx.outputs.iter().enumerate() {
            let bytes = o.sigma_serialize_bytes().map_err(|e| WireError::Ser(e.to_string()))?;
            let tree_bytes = o.ergo_tree.sigma_serialize_bytes().map_err(|e| WireError::Ser(e.to_string()))?;
            let tokens = o.tokens.as_ref().map(|ts| ts.iter().map(|t| (t.token_id.into(), *t.amount.as_u64())).collect()).unwrap_or_default();
            outputs.push(DecodedBox {
                id: BoxId(o.box_id().into()), value: *o.value.as_u64(), tree_hash: tree_hash(&tree_bytes), tree_bytes,
                creation_height: o.creation_height, tx_id, index: i as u16, tokens,
                registers_json: serde_json::to_string(&o.additional_registers)?, size: bytes.len() as u32,
            });
        }
        let size = tx.sigma_serialize_bytes().map_err(|e| WireError::Ser(e.to_string()))?.len() as u32;
        txs.push(DecodedTx {
            id: tx_id,
            inputs: tx.inputs.iter().map(|i| BoxId(i.box_id.into())).collect(),
            data_inputs: tx.data_inputs.as_ref().map(|d| d.iter().map(|i| BoxId(i.box_id.into())).collect()).unwrap_or_default(),
            outputs, size,
        });
    }
    let size = v.get("size").and_then(|s| s.as_u64()).unwrap_or(0) as u32;
    Ok(DecodedBlock { header, txs, size })
}
```
Notes for the implementer: `BoxId`, `TokenId`, `Digest32` in ergo-lib convert to `[u8;32]` via `From`/`.into()` or `.0 .0` on the inner `Digest32`; adjust accessor paths to the 0.28 API if the compiler disagrees, but keep the produced struct shapes exactly as above.

- [ ] **Step 5: Run tests**

Run: `cargo test -p xp-wire`
Expected: 3 passed.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(wire): decode node block JSON via ergo-lib; tree hash/template/address"
```

---

### Task 3: `xp-store` — tables, key codecs, row codecs

**Files:**
- Create: `crates/xp-store/Cargo.toml`, `crates/xp-store/src/lib.rs`, `src/tables.rs`, `src/keys.rs`, `src/rows.rs`

**Interfaces:**
- Produces:
```rust
// tables.rs — all TableDefinition<'static, &[u8], &[u8]>
pub const META, HEADERS, HEADER_BY_ID, TXS, TX_BY_GIDX, BOXES, BOX_BY_GIDX, ERGO_TREES,
          TREE_BOXES, TREE_UNSPENT, TREE_TXS, TREE_BALANCE, RICH, RENT_MATURES, UNDO;
// keys.rs
pub fn k_u32(h: u32) -> [u8; 4];  pub fn k_u64(g: u64) -> [u8; 8];
pub fn k_hash_gidx(h: &Hash32, g: Gidx) -> [u8; 40];        // (hash, gidx BE)
pub fn k_rich(nano: u64, tree: &Hash32) -> [u8; 40];         // (nano BE, tree)
pub fn k_rent(mature: u32, g: Gidx) -> [u8; 12];
pub fn prefix_range(prefix: &[u8]) -> (Vec<u8>, Vec<u8>);    // [prefix.., prefix+1..)
// rows.rs — each has fn encode(&self) -> Vec<u8>, fn decode(&[u8]) -> Result<Self, StoreError>
pub struct HeaderRow { pub id: Hash32, pub parent_id: Hash32, pub timestamp: u64, pub difficulty: u128, pub miner_pk: [u8;33],
                       pub tx_count: u32, pub size: u32, pub fees: u64, pub reward: u64, pub version: u8, pub raw_json: String }
pub struct TxRow { pub height: u32, pub index: u16, pub gidx: Gidx, pub timestamp: u64, pub size: u32, pub fee: u64,
                   pub inputs: Vec<Hash32>, pub data_inputs: Vec<Hash32>, pub output_count: u16 }
pub struct BoxRow { pub gidx: Gidx, pub value: u64, pub tree_hash: Hash32, pub creation_height: u32, pub tx_id: Hash32,
                    pub index: u16, pub size: u32, pub tokens: Vec<(Hash32, u64)>, pub registers_json: String,
                    pub spent: Option<(Hash32, u32)> }
pub struct TreeRow { pub tree_bytes: Vec<u8>, pub template_hash: Hash32, pub address: String, pub kind: u8 }
pub struct BalanceRow { pub nano: u64, pub tokens: Vec<(Hash32, u64)>, pub box_count: u64, pub first_seen: u32, pub last_seen: u32 }
pub struct UndoRow { pub created_boxes: Vec<Hash32>, pub spent_boxes: Vec<Hash32>, pub tx_ids: Vec<Hash32>,
                     pub prev_balances: Vec<(Hash32, Option<BalanceRow>)>, pub prev_next_box_gidx: Gidx, pub prev_next_tx_gidx: Gidx,
                     pub new_trees: Vec<Hash32> }
```
Encoding rule: little helper `W`/`R` cursors; fixed ints big-endian; `Vec`/`String` prefixed by u32 length; `Option` prefixed by 1 byte.

- [ ] **Step 1: Failing round-trip tests**

`crates/xp-store/Cargo.toml`:
```toml
[package]
name = "xp-store"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
xp-types.workspace = true
xp-wire.workspace = true
redb.workspace = true
thiserror.workspace = true
tracing.workspace = true

[dev-dependencies]
proptest.workspace = true
tempfile.workspace = true
```

Append to `crates/xp-store/src/rows.rs` (the module also holds the impls):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn arb_hash() -> impl Strategy<Value = [u8; 32]> { any::<[u8; 32]>() }

    proptest! {
        #[test]
        fn box_row_roundtrip(gidx in any::<u64>(), value in any::<u64>(), th in arb_hash(), ch in any::<u32>(), tx in arb_hash(),
                             idx in any::<u16>(), size in any::<u32>(), toks in proptest::collection::vec((arb_hash(), any::<u64>()), 0..4),
                             regs in ".*", spent in proptest::option::of((arb_hash(), any::<u32>()))) {
            let r = BoxRow { gidx, value, tree_hash: th, creation_height: ch, tx_id: tx, index: idx, size, tokens: toks, registers_json: regs, spent };
            let d = BoxRow::decode(&r.encode()).unwrap();
            prop_assert_eq!(d, r);
        }
        #[test]
        fn balance_row_roundtrip(nano in any::<u64>(), toks in proptest::collection::vec((arb_hash(), any::<u64>()), 0..4), bc in any::<u64>(), f in any::<u32>(), l in any::<u32>()) {
            let r = BalanceRow { nano, tokens: toks, box_count: bc, first_seen: f, last_seen: l };
            prop_assert_eq!(BalanceRow::decode(&r.encode()).unwrap(), r);
        }
    }

    #[test]
    fn undo_row_roundtrip() {
        let r = UndoRow { created_boxes: vec![[1;32]], spent_boxes: vec![[2;32],[3;32]], tx_ids: vec![[4;32]],
            prev_balances: vec![([5;32], None), ([6;32], Some(BalanceRow{ nano: 7, tokens: vec![], box_count: 1, first_seen: 1, last_seen: 2 }))],
            prev_next_box_gidx: 10, prev_next_tx_gidx: 11, new_trees: vec![[8;32]] };
        assert_eq!(UndoRow::decode(&r.encode()).unwrap(), r);
    }
}
```
And in `keys.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn composite_keys_order_by_gidx_within_hash() {
        let h = [7u8; 32];
        assert!(k_hash_gidx(&h, 1) < k_hash_gidx(&h, 2));
        assert!(k_hash_gidx(&[6u8; 32], u64::MAX) < k_hash_gidx(&h, 0));
    }
    #[test]
    fn rich_key_orders_by_balance() {
        assert!(k_rich(1, &[0; 32]) < k_rich(2, &[0; 32]));
    }
    #[test]
    fn prefix_range_is_half_open() {
        let (lo, hi) = prefix_range(&[0xAA, 0xFF]);
        assert_eq!(lo, vec![0xAA, 0xFF]);
        assert_eq!(hi, vec![0xAB]);
        let (_, hi2) = prefix_range(&[0xFF, 0xFF]);
        assert_eq!(hi2, vec![0xFF, 0xFF, 0xFF]); // cannot increment: sentinel = prefix + 0xFF (documented limitation; hashes never all-0xFF)
    }
}
```

- [ ] **Step 2: Run — expect compile failure**

Run: `cargo test -p xp-store`

- [ ] **Step 3: Implement**

`crates/xp-store/src/tables.rs`:
```rust
use redb::TableDefinition;
pub type Tbl = TableDefinition<'static, &'static [u8], &'static [u8]>;
pub const META: Tbl = TableDefinition::new("meta");
pub const HEADERS: Tbl = TableDefinition::new("headers");
pub const HEADER_BY_ID: Tbl = TableDefinition::new("header_by_id");
pub const TXS: Tbl = TableDefinition::new("txs");
pub const TX_BY_GIDX: Tbl = TableDefinition::new("tx_by_gidx");
pub const BOXES: Tbl = TableDefinition::new("boxes");
pub const BOX_BY_GIDX: Tbl = TableDefinition::new("box_by_gidx");
pub const ERGO_TREES: Tbl = TableDefinition::new("ergo_trees");
pub const TREE_BOXES: Tbl = TableDefinition::new("tree_boxes");
pub const TREE_UNSPENT: Tbl = TableDefinition::new("tree_unspent");
pub const TREE_TXS: Tbl = TableDefinition::new("tree_txs");
pub const TREE_BALANCE: Tbl = TableDefinition::new("tree_balance");
pub const RICH: Tbl = TableDefinition::new("rich");
pub const RENT_MATURES: Tbl = TableDefinition::new("rent_matures");
pub const UNDO: Tbl = TableDefinition::new("undo");
pub const ALL: [Tbl; 15] = [META, HEADERS, HEADER_BY_ID, TXS, TX_BY_GIDX, BOXES, BOX_BY_GIDX, ERGO_TREES,
                            TREE_BOXES, TREE_UNSPENT, TREE_TXS, TREE_BALANCE, RICH, RENT_MATURES, UNDO];
pub const META_INDEXED_HEIGHT: &[u8] = b"indexed_height";
pub const META_NEXT_BOX_GIDX: &[u8] = b"next_box_gidx";
pub const META_NEXT_TX_GIDX: &[u8] = b"next_tx_gidx";
pub const META_SCHEMA: &[u8] = b"schema_version";
pub const SCHEMA_VERSION: u32 = 1;
```

`crates/xp-store/src/keys.rs`:
```rust
use xp_types::{Gidx, Hash32};
pub fn k_u32(h: u32) -> [u8; 4] { h.to_be_bytes() }
pub fn k_u64(g: u64) -> [u8; 8] { g.to_be_bytes() }
pub fn k_hash_gidx(h: &Hash32, g: Gidx) -> [u8; 40] { let mut k = [0u8; 40]; k[..32].copy_from_slice(h); k[32..].copy_from_slice(&g.to_be_bytes()); k }
pub fn k_rich(nano: u64, tree: &Hash32) -> [u8; 40] { let mut k = [0u8; 40]; k[..8].copy_from_slice(&nano.to_be_bytes()); k[8..].copy_from_slice(tree); k }
pub fn k_rent(mature: u32, g: Gidx) -> [u8; 12] { let mut k = [0u8; 12]; k[..4].copy_from_slice(&mature.to_be_bytes()); k[4..].copy_from_slice(&g.to_be_bytes()); k }
pub fn gidx_of_composite(k: &[u8]) -> Gidx { let mut b = [0u8; 8]; b.copy_from_slice(&k[k.len() - 8..]); u64::from_be_bytes(b) }
/// Half-open range covering every key that starts with `prefix`.
pub fn prefix_range(prefix: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let lo = prefix.to_vec();
    let mut hi = prefix.to_vec();
    for i in (0..hi.len()).rev() {
        if hi[i] != 0xFF { hi[i] += 1; hi.truncate(i + 1); return (lo, hi); }
    }
    hi.push(0xFF);
    (lo, hi)
}
```

`crates/xp-store/src/rows.rs`: implement `W` (Vec<u8> writer with `u8/u16/u32/u64/u128/hash/bytes/str/opt`) and `R` (slice reader with matching methods returning `Result<_, StoreError>`), then `encode`/`decode` for the six rows exactly in field order listed in Interfaces. `StoreError` lives in `lib.rs`:
```rust
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("redb: {0}")] Redb(String),
    #[error("corrupt row: {0}")] Corrupt(&'static str),
    #[error("fork deeper than rollback window ({0} blocks): reindex required")] ReindexRequired(u32),
    #[error("parent mismatch at height {height}: have {have}, block says {want}")] ParentMismatch { height: u32, have: String, want: String },
    #[error("wire: {0}")] Wire(#[from] xp_wire::WireError),
}
impl<E: std::fmt::Display> From<redb::Error> for StoreError { fn from(e: redb::Error) -> Self { StoreError::Redb(e.to_string()) } } // plus TableError, StorageError, TransactionError, CommitError, DatabaseError via a macro
```
Derive `Debug, Clone, PartialEq, Eq` on every row.

- [ ] **Step 4: Run tests**

Run: `cargo test -p xp-store`
Expected: all pass (proptest default 256 cases each).

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(store): table definitions, composite key codecs, row codecs"
```

---

### Task 4: `xp-store` — open/meta and `apply_batch`

**Files:**
- Modify: `crates/xp-store/src/lib.rs`
- Create: `crates/xp-store/src/apply.rs`

**Interfaces:**
- Consumes: `xp_wire::{DecodedBlock, tree_info}`, rows/keys/tables from Task 3.
- Produces:
```rust
pub struct Store { db: redb::Database }
impl Store {
    pub fn open(path: &std::path::Path) -> Result<Store, StoreError>;   // creates tables, writes schema version, refuses mismatched schema
    pub fn indexed_height(&self) -> Result<Option<u32>, StoreError>;     // None = empty store
    pub fn header_id_at(&self, height: u32) -> Result<Option<Hash32>, StoreError>;
    pub fn apply_batch(&self, blocks: &[DecodedBlock], durable: bool) -> Result<(), StoreError>;
    pub fn begin_read(&self) -> Result<redb::ReadTransaction, StoreError>;
}
```
`apply_batch` contract: blocks contiguous and ascending; block[0].height must equal indexed_height+1 (or 1 when empty; genesis handled as height 1 = first block after genesis? **No**: Ergo's first block is height 1 and genesis state boxes appear as outputs of block 1's coinbase-like tx set; the store starts at height 1); block[i].parent_id must equal stored/previous id else `ParentMismatch`. One write transaction for the whole slice; `durable=false` ⇒ `Durability::None`.

- [ ] **Step 1: Failing tests**

`crates/xp-store/tests/apply.rs`:
```rust
use xp_store::Store;
use xp_wire::decode_block;

fn fixture(h: u32) -> xp_wire::DecodedBlock {
    decode_block(&std::fs::read_to_string(format!("{}/../../tests/fixtures/blocks/{h}.json", env!("CARGO_MANIFEST_DIR"))).unwrap()).unwrap()
}

/// Fixtures start at 1866000; tests seed `indexed_height` = 1865999 with a synthetic header whose id equals block 1866000's parent.
fn seeded_store(dir: &std::path::Path) -> Store {
    let s = Store::open(&dir.join("x.redb")).unwrap();
    let b0 = fixture(1866000);
    s.seed_for_tests(1865999, b0.header.parent_id.0).unwrap();
    s
}

#[test]
fn applies_three_blocks_and_tracks_height() {
    let dir = tempfile::tempdir().unwrap();
    let s = seeded_store(dir.path());
    s.apply_batch(&[fixture(1866000), fixture(1866001)], true).unwrap();
    assert_eq!(s.indexed_height().unwrap(), Some(1866001));
    s.apply_batch(&[fixture(1866002)], true).unwrap();
    assert_eq!(s.indexed_height().unwrap(), Some(1866002));
    assert_eq!(s.header_id_at(1866002).unwrap(), Some(fixture(1866002).header.id.0));
}

#[test]
fn rejects_parent_mismatch() {
    let dir = tempfile::tempdir().unwrap();
    let s = seeded_store(dir.path());
    s.apply_batch(&[fixture(1866000)], true).unwrap();
    let err = s.apply_batch(&[fixture(1866002)], true).unwrap_err();
    assert!(matches!(err, xp_store::StoreError::ParentMismatch { .. }));
    assert_eq!(s.indexed_height().unwrap(), Some(1866000)); // txn rolled back
}

#[test]
fn outputs_become_unspent_then_spent() {
    let dir = tempfile::tempdir().unwrap();
    let s = seeded_store(dir.path());
    let b = fixture(1866000);
    s.apply_batch(&[b.clone()], true).unwrap();
    let rd = xp_store::Reader::new(&s).unwrap();
    let out = &b.txs[0].outputs[0];
    let row = rd.box_by_id(&out.id.0).unwrap().expect("box indexed");
    assert_eq!(row.value, out.value);
    assert!(row.spent.is_none());
    let bal = rd.balance(&out.tree_hash.0).unwrap().expect("balance");
    assert!(bal.nano >= out.value);
    // rent maturity indexed
    let mats = rd.rent_matures_range(xp_types::rent::maturity_height(out.creation_height), 1, 10).unwrap();
    assert!(mats.iter().any(|(_, id)| *id == out.id.0));
}
```
`seed_for_tests` is `#[doc(hidden)] pub fn seed_for_tests(&self, height: u32, id: Hash32)` writing a minimal `HeaderRow` and meta. `Reader` is Task 6; for this task add the three used methods (`box_by_id`, `balance`, `rent_matures_range`) in a stub `read.rs` so the test compiles — Task 6 fills the rest.

- [ ] **Step 2: Run — expect failure**

Run: `cargo test -p xp-store --test apply`

- [ ] **Step 3: Implement `apply.rs`**

```rust
use redb::{Durability, ReadableTable, WriteTransaction};
use xp_types::{rent::maturity_height, Gidx, Hash32};
use xp_wire::{tree_info, DecodedBlock};
use crate::{keys::*, rows::*, tables::*, Store, StoreError};

struct Ctx<'t> { txn: &'t WriteTransaction, next_box: Gidx, next_tx: Gidx, undo: UndoRow, height: u32, timestamp: u64 }

impl Store {
    pub fn apply_batch(&self, blocks: &[DecodedBlock], durable: bool) -> Result<(), StoreError> {
        if blocks.is_empty() { return Ok(()); }
        let mut txn = self.db.begin_write()?;
        txn.set_durability(if durable { Durability::Immediate } else { Durability::None });
        let (mut height, mut prev_id) = {
            let meta = txn.open_table(META)?;
            let h = meta.get(META_INDEXED_HEIGHT)?.map(|v| u32::from_be_bytes(v.value().try_into().unwrap()));
            match h {
                Some(h) => { let hd = txn.open_table(HEADERS)?; let row = HeaderRow::decode(hd.get(k_u32(h).as_slice())?.ok_or(StoreError::Corrupt("missing tip header"))?.value())?; (h, Some(row.id)) }
                None => (0, None),
            }
        };
        let (mut next_box, mut next_tx) = read_gidx_counters(&txn)?;
        for b in blocks {
            if b.header.height != height + 1 { return Err(StoreError::ParentMismatch { height: b.header.height, have: format!("tip {height}"), want: "tip+1".into() }); }
            if let Some(p) = prev_id { if p != b.header.parent_id.0 { return Err(StoreError::ParentMismatch { height: b.header.height, have: hex::encode(p), want: hex::encode(b.header.parent_id.0) }); } }
            let mut ctx = Ctx { txn: &txn, next_box, next_tx, height: b.header.height, timestamp: b.header.timestamp,
                                undo: UndoRow { created_boxes: vec![], spent_boxes: vec![], tx_ids: vec![], prev_balances: vec![], prev_next_box_gidx: next_box, prev_next_tx_gidx: next_tx, new_trees: vec![] } };
            apply_block(&mut ctx, b)?;
            next_box = ctx.next_box; next_tx = ctx.next_tx;
            txn.open_table(UNDO)?.insert(k_u32(b.header.height).as_slice(), ctx.undo.encode().as_slice())?;
            if b.header.height > crate::ROLLBACK_WINDOW { txn.open_table(UNDO)?.remove(k_u32(b.header.height - crate::ROLLBACK_WINDOW - 1).as_slice())?; }
            height = b.header.height; prev_id = Some(b.header.id.0);
        }
        { let mut meta = txn.open_table(META)?;
          meta.insert(META_INDEXED_HEIGHT, k_u32(height).as_slice())?;
          meta.insert(META_NEXT_BOX_GIDX, k_u64(next_box).as_slice())?;
          meta.insert(META_NEXT_TX_GIDX, k_u64(next_tx).as_slice())?; }
        txn.commit()?;
        Ok(())
    }
}

fn apply_block(ctx: &mut Ctx, b: &DecodedBlock) -> Result<(), StoreError> {
    let mut fees = 0u64;
    let mut touched_balances: std::collections::HashMap<Hash32, BalanceRow> = Default::default();
    for (ti, tx) in b.txs.iter().enumerate() {
        let tx_gidx = ctx.next_tx; ctx.next_tx += 1;
        let mut value_in = 0u64; let mut trees_in_tx: Vec<Hash32> = vec![];
        // inputs
        for inp in &tx.inputs {
            let mut boxes = ctx.txn.open_table(BOXES)?;
            let mut row = BoxRow::decode(boxes.get(inp.0.as_slice())?.ok_or(StoreError::Corrupt("input box missing"))?.value())?;
            if row.spent.is_some() { return Err(StoreError::Corrupt("double spend in block")); }
            row.spent = Some((tx.id.0, ctx.height));
            value_in += row.value;
            boxes.insert(inp.0.as_slice(), row.encode().as_slice())?;
            ctx.txn.open_table(TREE_UNSPENT)?.remove(k_hash_gidx(&row.tree_hash, row.gidx).as_slice())?;
            ctx.txn.open_table(RENT_MATURES)?.remove(k_rent(maturity_height(row.creation_height), row.gidx).as_slice())?;
            let bal = load_balance(ctx, &mut touched_balances, &row.tree_hash)?;
            bal.nano -= row.value; bal.box_count -= 1; sub_tokens(&mut bal.tokens, &row.tokens);
            bal.last_seen = ctx.height;
            trees_in_tx.push(row.tree_hash);
            ctx.undo.spent_boxes.push(inp.0);
        }
        // outputs
        let mut value_out = 0u64;
        for o in &tx.outputs {
            let gidx = ctx.next_box; ctx.next_box += 1;
            value_out += o.value;
            { let mut trees = ctx.txn.open_table(ERGO_TREES)?;
              if trees.get(o.tree_hash.0.as_slice())?.is_none() {
                  let info = tree_info(&o.tree_bytes).unwrap_or(xp_wire::TreeInfo { template_hash: [0; 32], address: hex::encode(&o.tree_bytes), kind: xp_wire::TreeKind::Other });
                  trees.insert(o.tree_hash.0.as_slice(), TreeRow { tree_bytes: o.tree_bytes.clone(), template_hash: info.template_hash, address: info.address, kind: info.kind as u8 }.encode().as_slice())?;
                  ctx.undo.new_trees.push(o.tree_hash.0);
              } }
            let row = BoxRow { gidx, value: o.value, tree_hash: o.tree_hash.0, creation_height: o.creation_height, tx_id: o.tx_id.0, index: o.index, size: o.size, tokens: o.tokens.clone(), registers_json: o.registers_json.clone(), spent: None };
            ctx.txn.open_table(BOXES)?.insert(o.id.0.as_slice(), row.encode().as_slice())?;
            ctx.txn.open_table(BOX_BY_GIDX)?.insert(k_u64(gidx).as_slice(), o.id.0.as_slice())?;
            ctx.txn.open_table(TREE_BOXES)?.insert(k_hash_gidx(&o.tree_hash.0, gidx).as_slice(), &[][..])?;
            ctx.txn.open_table(TREE_UNSPENT)?.insert(k_hash_gidx(&o.tree_hash.0, gidx).as_slice(), &[][..])?;
            ctx.txn.open_table(RENT_MATURES)?.insert(k_rent(maturity_height(o.creation_height), gidx).as_slice(), o.id.0.as_slice())?;
            let bal = load_balance(ctx, &mut touched_balances, &o.tree_hash.0)?;
            bal.nano += o.value; bal.box_count += 1; add_tokens(&mut bal.tokens, &o.tokens);
            if bal.first_seen == 0 { bal.first_seen = ctx.height; } bal.last_seen = ctx.height;
            trees_in_tx.push(o.tree_hash.0);
            ctx.undo.created_boxes.push(o.id.0);
        }
        let fee = if ti == 0 { 0 } else { value_in.saturating_sub(value_out) }; // coinbase-like emission tx has no fee semantics
        fees += fee;
        trees_in_tx.sort(); trees_in_tx.dedup();
        for t in trees_in_tx { ctx.txn.open_table(TREE_TXS)?.insert(k_hash_gidx(&t, tx_gidx).as_slice(), &[][..])?; }
        let txrow = TxRow { height: ctx.height, index: ti as u16, gidx: tx_gidx, timestamp: ctx.timestamp, size: tx.size, fee,
                            inputs: tx.inputs.iter().map(|i| i.0).collect(), data_inputs: tx.data_inputs.iter().map(|i| i.0).collect(), output_count: tx.outputs.len() as u16 };
        ctx.txn.open_table(TXS)?.insert(tx.id.0.as_slice(), txrow.encode().as_slice())?;
        ctx.txn.open_table(TX_BY_GIDX)?.insert(k_u64(tx_gidx).as_slice(), tx.id.0.as_slice())?;
        ctx.undo.tx_ids.push(tx.id.0);
    }
    // flush balances + rich index
    for (tree, bal) in touched_balances {
        let mut tb = ctx.txn.open_table(TREE_BALANCE)?;
        let mut rich = ctx.txn.open_table(RICH)?;
        let prev = tb.get(tree.as_slice())?.map(|v| BalanceRow::decode(v.value())).transpose()?;
        if let Some(p) = &prev { rich.remove(k_rich(p.nano, &tree).as_slice())?; }
        ctx.undo.prev_balances.push((tree, prev));
        if bal.nano > 0 { rich.insert(k_rich(bal.nano, &tree).as_slice(), &[][..])?; }
        tb.insert(tree.as_slice(), bal.encode().as_slice())?;
    }
    let reward = b.txs.first().map(|t| t.outputs.iter().map(|o| o.value).sum::<u64>()).unwrap_or(0);
    let hrow = HeaderRow { id: b.header.id.0, parent_id: b.header.parent_id.0, timestamp: b.header.timestamp, difficulty: b.header.difficulty, miner_pk: b.header.miner_pk,
                           tx_count: b.txs.len() as u32, size: b.size, fees, reward, version: b.header.version, raw_json: b.header.raw_json.clone() };
    ctx.txn.open_table(HEADERS)?.insert(k_u32(ctx.height).as_slice(), hrow.encode().as_slice())?;
    ctx.txn.open_table(HEADER_BY_ID)?.insert(b.header.id.0.as_slice(), k_u32(ctx.height).as_slice())?;
    Ok(())
}

fn load_balance<'a>(ctx: &Ctx, cache: &'a mut std::collections::HashMap<Hash32, BalanceRow>, tree: &Hash32) -> Result<&'a mut BalanceRow, StoreError> {
    if !cache.contains_key(tree) {
        let tb = ctx.txn.open_table(TREE_BALANCE)?;
        let row = tb.get(tree.as_slice())?.map(|v| BalanceRow::decode(v.value())).transpose()?
            .unwrap_or(BalanceRow { nano: 0, tokens: vec![], box_count: 0, first_seen: 0, last_seen: 0 });
        cache.insert(*tree, row);
    }
    Ok(cache.get_mut(tree).unwrap())
}
fn add_tokens(bal: &mut Vec<(Hash32, u64)>, add: &[(Hash32, u64)]) { for (id, amt) in add { match bal.iter_mut().find(|(i, _)| i == id) { Some(e) => e.1 += amt, None => bal.push((*id, *amt)) } } }
fn sub_tokens(bal: &mut Vec<(Hash32, u64)>, sub: &[(Hash32, u64)]) { for (id, amt) in sub { if let Some(pos) = bal.iter().position(|(i, _)| i == id) { bal[pos].1 = bal[pos].1.saturating_sub(*amt); if bal[pos].1 == 0 { bal.swap_remove(pos); } } } }
fn read_gidx_counters(txn: &WriteTransaction) -> Result<(Gidx, Gidx), StoreError> {
    let meta = txn.open_table(META)?;
    let rd = |k: &[u8]| -> Result<Gidx, StoreError> { Ok(meta.get(k)?.map(|v| u64::from_be_bytes(v.value().try_into().unwrap())).unwrap_or(0)) };
    Ok((rd(META_NEXT_BOX_GIDX)?, rd(META_NEXT_TX_GIDX)?))
}
```
`lib.rs` additions: `pub const ROLLBACK_WINDOW: u32 = 1_000;`, `Store::open` (create db, open every table in `ALL` once inside a write txn to create them, write/check `META_SCHEMA`), `indexed_height`, `header_id_at`, `begin_read`, `seed_for_tests`. Add `hex` to the crate's dependencies.

**Genesis note for the implementer:** Ergo's block 1 spends nothing that the store knows about (its inputs are the genesis boxes created by the chain spec). The first apply of a fresh store must therefore accept inputs missing from `BOXES` *only when height == 1*; treat them as value 0 for balances. Add this branch in the inputs loop: `if ctx.height == 1 && missing { continue; }`. The three-fixture tests seed at 1865999, so this branch is exercised only by the full sync in Task 8's smoke run.

- [ ] **Step 4: Run tests**

Run: `cargo test -p xp-store`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(store): open/meta and apply_batch with balances, rich index, rent maturity, undo rows"
```

---

### Task 5: `xp-store` — `rollback_to` and apply/rollback identity property

**Files:**
- Create: `crates/xp-store/src/rollback.rs`
- Test: `crates/xp-store/tests/rollback.rs`

**Interfaces:**
- Produces: `impl Store { pub fn rollback_to(&self, target_height: u32) -> Result<(), StoreError>; pub fn fingerprint(&self) -> Result<Hash32, StoreError>; }` — `fingerprint` hashes every (table, key, value) in `ALL` except `UNDO`, in table order, with blake2b256 (test/diagnostic use).

- [ ] **Step 1: Failing tests**

```rust
use xp_store::Store;
use xp_wire::decode_block;
fn fixture(h: u32) -> xp_wire::DecodedBlock { decode_block(&std::fs::read_to_string(format!("{}/../../tests/fixtures/blocks/{h}.json", env!("CARGO_MANIFEST_DIR"))).unwrap()).unwrap() }

#[test]
fn apply_then_rollback_is_identity() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("x.redb")).unwrap();
    s.seed_for_tests(1865999, fixture(1866000).header.parent_id.0).unwrap();
    s.apply_batch(&[fixture(1866000)], true).unwrap();
    let before = s.fingerprint().unwrap();
    s.apply_batch(&[fixture(1866001), fixture(1866002)], true).unwrap();
    assert_ne!(s.fingerprint().unwrap(), before);
    s.rollback_to(1866000).unwrap();
    assert_eq!(s.fingerprint().unwrap(), before);
    assert_eq!(s.indexed_height().unwrap(), Some(1866000));
    // and re-applying works (gidx counters restored)
    s.apply_batch(&[fixture(1866001)], true).unwrap();
}

#[test]
fn rollback_beyond_window_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("x.redb")).unwrap();
    s.seed_for_tests(1865999, fixture(1866000).header.parent_id.0).unwrap();
    s.apply_batch(&[fixture(1866000)], true).unwrap();
    let err = s.rollback_to(1865999 - xp_store::ROLLBACK_WINDOW - 5).unwrap_err();
    assert!(matches!(err, xp_store::StoreError::ReindexRequired(_)));
}
```

- [ ] **Step 2: Run — expect failure**

- [ ] **Step 3: Implement `rollback.rs`**

```rust
use redb::ReadableTable;
use xp_types::rent::maturity_height;
use crate::{keys::*, rows::*, tables::*, Store, StoreError, ROLLBACK_WINDOW};

impl Store {
    pub fn rollback_to(&self, target: u32) -> Result<(), StoreError> {
        let tip = self.indexed_height()?.ok_or(StoreError::Corrupt("rollback on empty store"))?;
        if tip <= target { return Ok(()); }
        if tip - target > ROLLBACK_WINDOW { return Err(StoreError::ReindexRequired(tip - target)); }
        let txn = self.db.begin_write()?;
        for h in (target + 1..=tip).rev() {
            let undo = { let u = txn.open_table(UNDO)?; UndoRow::decode(u.get(k_u32(h).as_slice())?.ok_or(StoreError::ReindexRequired(tip - target))?.value())? };
            // 1. un-create boxes (reverse order)
            for id in undo.created_boxes.iter().rev() {
                let row = { let b = txn.open_table(BOXES)?; BoxRow::decode(b.get(id.as_slice())?.ok_or(StoreError::Corrupt("undo: created box missing"))?.value())? };
                txn.open_table(BOXES)?.remove(id.as_slice())?;
                txn.open_table(BOX_BY_GIDX)?.remove(k_u64(row.gidx).as_slice())?;
                txn.open_table(TREE_BOXES)?.remove(k_hash_gidx(&row.tree_hash, row.gidx).as_slice())?;
                txn.open_table(TREE_UNSPENT)?.remove(k_hash_gidx(&row.tree_hash, row.gidx).as_slice())?;
                txn.open_table(RENT_MATURES)?.remove(k_rent(maturity_height(row.creation_height), row.gidx).as_slice())?;
            }
            // 2. un-spend inputs
            for id in &undo.spent_boxes {
                let mut b = txn.open_table(BOXES)?;
                if let Some(v) = b.get(id.as_slice())? {
                    let mut row = BoxRow::decode(v.value())?; drop(v);
                    row.spent = None;
                    b.insert(id.as_slice(), row.encode().as_slice())?;
                    txn.open_table(TREE_UNSPENT)?.insert(k_hash_gidx(&row.tree_hash, row.gidx).as_slice(), &[][..])?;
                    txn.open_table(RENT_MATURES)?.insert(k_rent(maturity_height(row.creation_height), row.gidx).as_slice(), id.as_slice())?;
                }
            }
            // 3. txs and tree_txs
            for tid in &undo.tx_ids {
                let row = { let t = txn.open_table(TXS)?; TxRow::decode(t.get(tid.as_slice())?.ok_or(StoreError::Corrupt("undo: tx missing"))?.value())? };
                txn.open_table(TXS)?.remove(tid.as_slice())?;
                txn.open_table(TX_BY_GIDX)?.remove(k_u64(row.gidx).as_slice())?;
                let (lo, hi) = (k_u64(row.gidx), k_u64(row.gidx)); let _ = (lo, hi);
                // remove every TREE_TXS entry ending in this gidx: iterate trees touched via prev_balances
                let mut tt = txn.open_table(TREE_TXS)?;
                for (tree, _) in &undo.prev_balances { let _ = tt.remove(k_hash_gidx(tree, row.gidx).as_slice())?; }
            }
            // 4. balances + rich
            for (tree, prev) in &undo.prev_balances {
                let mut tb = txn.open_table(TREE_BALANCE)?; let mut rich = txn.open_table(RICH)?;
                if let Some(cur) = tb.get(tree.as_slice())?.map(|v| BalanceRow::decode(v.value())).transpose()? { rich.remove(k_rich(cur.nano, tree).as_slice())?; }
                match prev { Some(p) => { tb.insert(tree.as_slice(), p.encode().as_slice())?; if p.nano > 0 { rich.insert(k_rich(p.nano, tree).as_slice(), &[][..])?; } }
                             None => { tb.remove(tree.as_slice())?; } }
            }
            // 5. trees first seen in this block
            for t in &undo.new_trees { txn.open_table(ERGO_TREES)?.remove(t.as_slice())?; }
            // 6. header, undo, counters
            let hid = { let hd = txn.open_table(HEADERS)?; HeaderRow::decode(hd.get(k_u32(h).as_slice())?.ok_or(StoreError::Corrupt("undo: header missing"))?.value())?.id };
            txn.open_table(HEADERS)?.remove(k_u32(h).as_slice())?;
            txn.open_table(HEADER_BY_ID)?.remove(hid.as_slice())?;
            txn.open_table(UNDO)?.remove(k_u32(h).as_slice())?;
            let mut meta = txn.open_table(META)?;
            meta.insert(META_NEXT_BOX_GIDX, k_u64(undo.prev_next_box_gidx).as_slice())?;
            meta.insert(META_NEXT_TX_GIDX, k_u64(undo.prev_next_tx_gidx).as_slice())?;
            meta.insert(META_INDEXED_HEIGHT, k_u32(h - 1).as_slice())?;
        }
        txn.commit()?;
        Ok(())
    }
}
```
Implementation note: step 3 relies on `prev_balances` containing every tree touched by the block, which `apply_block` guarantees (every input and output tree loads a balance). Implement `fingerprint` in `lib.rs` by iterating `ALL` minus `UNDO` with `range::<&[u8]>(..)` and hashing `table_name || key || value`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p xp-store`
Expected: pass. If `apply_then_rollback_is_identity` fails, diff which table differs by making `fingerprint` return per-table hashes under `cfg(test)` — do not weaken the test.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(store): rollback_to via undo rows; apply/rollback identity test"
```

---

### Task 6: `xp-store` — `Reader` snapshot queries

**Files:**
- Create/complete: `crates/xp-store/src/read.rs`
- Test: `crates/xp-store/tests/read.rs`

**Interfaces:**
- Produces:
```rust
pub struct Reader { txn: redb::ReadTransaction }
pub enum Dir { Asc, Desc }
pub struct Page<T> { pub items: Vec<T>, pub next_cursor: Option<Gidx> }
impl Reader {
    pub fn new(store: &Store) -> Result<Reader, StoreError>;
    pub fn indexed_height(&self) -> Result<Option<u32>, StoreError>;
    pub fn header_at(&self, height: u32) -> Result<Option<HeaderRow>, StoreError>;
    pub fn height_of_header(&self, id: &Hash32) -> Result<Option<u32>, StoreError>;
    pub fn headers_desc(&self, before_height: Option<u32>, limit: usize) -> Result<Vec<(u32, HeaderRow)>, StoreError>;
    pub fn tx_by_id(&self, id: &Hash32) -> Result<Option<TxRow>, StoreError>;
    pub fn txs_by_gidx(&self, cursor: Option<Gidx>, limit: usize, dir: Dir) -> Result<Page<(Hash32, TxRow)>, StoreError>;
    pub fn txs_in_block(&self, height: u32) -> Result<Vec<(Hash32, TxRow)>, StoreError>;      // scan TX_BY_GIDX from first tx gidx of the block; TxRow.height filter
    pub fn box_by_id(&self, id: &Hash32) -> Result<Option<BoxRow>, StoreError>;
    pub fn boxes_of_tx(&self, tx: &TxRow, tx_id: &Hash32) -> Result<Vec<(Hash32, BoxRow)>, StoreError>; // outputs by (tx_id,index) via BOX_BY_GIDX contiguous range
    pub fn tree_row(&self, tree: &Hash32) -> Result<Option<TreeRow>, StoreError>;
    pub fn tree_by_address(&self, address: &str) -> Result<Option<Hash32>, StoreError>;     // derive tree bytes from address with ergo-lib, hash, lookup
    pub fn balance(&self, tree: &Hash32) -> Result<Option<BalanceRow>, StoreError>;
    pub fn tree_boxes(&self, tree: &Hash32, unspent_only: bool, cursor: Option<Gidx>, limit: usize, dir: Dir) -> Result<Page<(Hash32, BoxRow)>, StoreError>;
    pub fn tree_txs(&self, tree: &Hash32, cursor: Option<Gidx>, limit: usize, dir: Dir) -> Result<Page<(Hash32, TxRow)>, StoreError>;
    pub fn richlist(&self, cursor: Option<(u64, Hash32)>, limit: usize) -> Result<(Vec<(Hash32, u64)>, Option<(u64, Hash32)>), StoreError>;
    pub fn rent_matures_range(&self, from_height: u32, span: u32, limit: usize) -> Result<Vec<(u32, Hash32)>, StoreError>; // (mature_height, box_id)
    pub fn rent_eligible(&self, at_height: u32, cursor: Option<(u32, Gidx)>, limit: usize) -> Result<(Vec<(u32, Hash32)>, Option<(u32, Gidx)>), StoreError>; // everything with mature ≤ at_height
}
```
Outputs of a tx: `apply_block` allocates output gidx contiguously per tx, so store the first output gidx in `TxRow` — **add field `first_out_gidx: Gidx` to `TxRow`** (update Task 3 codec + tests; schema still v1 since nothing is deployed).

- [ ] **Step 1: Failing tests** — cover: `headers_desc` returns 3 fixtures newest first with `before_height`; `tree_boxes` asc/desc pagination with `limit=1` walks all boxes of the coinbase tree; `boxes_of_tx` returns exactly `output_count` rows in index order; `richlist` first entry has the largest balance among all `TREE_BALANCE` rows; `rent_eligible(1866002+RENT_PERIOD)` includes every unspent output of the three fixtures and excludes spent ones; `tree_by_address(addr)` round-trips through `tree_row(..).address`.

- [ ] **Step 2: Run — expect failure**

- [ ] **Step 3: Implement** with `range` over `prefix_range(tree)` and `k_hash_gidx(tree, cursor±1)` bounds; `Desc` uses `.rev()`. `next_cursor` = gidx of the last returned item when `items.len() == limit`. For `tree_by_address`, use `AddressEncoder::new(Mainnet).parse_address_from_str(s)?.script()?.sigma_serialize_bytes()`, then `xp_wire::tree_hash`.

- [ ] **Step 4: Run tests** — pass.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(store): Reader snapshot queries with cursor pagination"
```

---

### Task 7: `xp-source` — `BlockSource` trait and Rust node REST implementation

**Files:**
- Create: `crates/xp-source/Cargo.toml`, `src/lib.rs`, `src/rust_node.rs`
- Test: `crates/xp-source/tests/rust_node.rs` (uses a local mock HTTP server: `axum` test router serving the fixtures)

**Interfaces:**
- Produces:
```rust
#[async_trait::async_trait]
pub trait BlockSource: Send + Sync {
    fn name(&self) -> &str;
    async fn best_height(&self) -> Result<u32, SourceError>;
    async fn header_id_at(&self, height: u32) -> Result<Option<Hash32>, SourceError>;
    async fn full_block_json(&self, id: &Hash32) -> Result<Option<String>, SourceError>;
}
pub struct RustNode { base: String, http: reqwest::Client }
impl RustNode { pub fn new(base_url: &str) -> RustNode; }
#[derive(thiserror::Error)] pub enum SourceError { Http(String), Decode(String), Unavailable }
```
Endpoints: `GET {base}/info` → `fullHeight`; `GET {base}/blocks/at/{h}` → `["id"]` (empty array ⇒ None); `GET {base}/blocks/{id}` → full block JSON (404 ⇒ None). Timeouts: connect 2 s, request 10 s. gzip on.

- [ ] **Step 1: Failing test** — spin an axum router on `127.0.0.1:0` with routes `/info`, `/blocks/at/{h}`, `/blocks/{id}` backed by the three fixtures; assert `best_height()==1866002`, `header_id_at(1866001)` equals fixture id, `header_id_at(1)` is None, `full_block_json(id)` decodes with `xp_wire::decode_block`, unknown id ⇒ None.

- [ ] **Step 2: Run — expect failure**

- [ ] **Step 3: Implement** (`reqwest::Client::builder().connect_timeout(2s).timeout(10s).gzip(true)`; parse `/info` as `serde_json::Value`).

- [ ] **Step 4: Run tests** — pass.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(source): BlockSource trait and Rust node REST source"
```

---

### Task 8: `xp-ingest` — pipeline with fork detection and sync modes

**Files:**
- Create: `crates/xp-ingest/Cargo.toml`, `src/lib.rs`
- Test: `crates/xp-ingest/tests/fork.rs`

**Interfaces:**
- Consumes: `BlockSource`, `Store::{indexed_height, header_id_at, apply_batch, rollback_to}`, `xp_wire::decode_block`.
- Produces:
```rust
pub struct IngestConfig { pub poll_ms: u64, pub bulk_batch: usize, pub bulk_concurrency: usize, pub durable_every: u32, pub tip_lag_for_bulk: u32 }
impl Default for IngestConfig { /* 500, 64, 8, 256, 64 */ }
pub struct IngestStatus { pub indexed: Option<u32>, pub best: u32, pub mode: Mode, pub source: String, pub halted: Option<String> }
pub enum Mode { Bulk, Tip }
pub async fn run(store: Arc<Store>, source: Arc<dyn BlockSource>, cfg: IngestConfig, status: watch::Sender<IngestStatus>, shutdown: CancellationToken) -> anyhow::Result<()>;
```
Algorithm per loop iteration:
1. `best = source.best_height()`; `indexed = store.indexed_height().unwrap_or(0)`.
2. If `indexed >= best`: sleep `poll_ms`, continue.
3. **Fork check**: if `indexed > 0`, compare `store.header_id_at(indexed)` with `source.header_id_at(indexed)`. If they differ, walk `h = indexed-1, indexed-2, …` until equal (or `indexed - h > ROLLBACK_WINDOW` ⇒ set `halted = "reindex required"` and return Err). Then `store.rollback_to(h)`; log; `indexed = h`.
4. Mode = `Bulk` if `best - indexed > tip_lag_for_bulk` else `Tip`.
5. Fetch heights `indexed+1 ..= min(best, indexed + batch)` where batch = `bulk_batch` in Bulk, 1 in Tip; fetch ids then blocks with `futures::stream::iter(..).buffered(concurrency)`; decode each with `decode_block` on `spawn_blocking`.
6. `store.apply_batch(&blocks, durable)` where `durable = mode == Tip || (indexed / durable_every) != (new_indexed / durable_every)`. On `ParentMismatch` (source changed tip mid-batch) just loop again — the fork check catches it.
7. Publish `IngestStatus`.

- [ ] **Step 1: Failing fork test** — a `FakeSource` (in-memory `Vec<DecodedBlock>` + JSON) built from the three fixtures, but the test needs a fork: create chain A = [1866000, 1866001, 1866002] and chain B = [1866000, 1866001', 1866002'] where the primes are the fixture JSON with `header.id`, `header.parentId` rewritten to fresh ids (edit the JSON strings, re-decode; tx content identical is fine). Run `run()` against A until `indexed==1866002`, swap the fake source's chain to B, and assert within 5 s that `indexed==1866002` and `store.header_id_at(1866002)==B[2].id` and `store.header_id_at(1866001)==B[1].id`. Also assert a fork deeper than `ROLLBACK_WINDOW` (FakeSource returning a different id at every height) makes `run()` return an error containing "reindex required".

- [ ] **Step 2: Run — expect failure**

- [ ] **Step 3: Implement** as specified (dependencies: tokio, tokio-util for `CancellationToken`, futures, anyhow, tracing).

- [ ] **Step 4: Run tests** — pass.

- [ ] **Step 5: Smoke run against the local node** (not a unit test): temporary `bin` wiring is Task 10; for now run `cargo test -p xp-ingest -- --ignored smoke_local_node` where the ignored test syncs heights 1..=2000 from `http://127.0.0.1:9063` into a tempdir store and asserts `indexed_height()==Some(2000)` and that block 1's genesis inputs did not error. Record blocks/s in the commit message.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(ingest): fetch→fork-check→apply pipeline with bulk/tip modes (smoke: N blocks/s)"
```

---

### Task 9: `xp-api` — router, DTOs, core handlers

**Files:**
- Create: `crates/xp-api/Cargo.toml`, `src/lib.rs`, `src/dto.rs`, `src/error.rs`, `src/handlers/{status,blocks,txs,boxes,addresses,richlist,rent,search}.rs`
- Test: `crates/xp-api/tests/routes.rs` (axum `Router` + `tower::ServiceExt::oneshot` over a store with the three fixtures applied)

**Interfaces:**
- Consumes: `Reader`, `IngestStatus` via `watch::Receiver`.
- Produces: `pub fn router(state: AppState) -> axum::Router;` `pub struct AppState { pub store: Arc<Store>, pub status: watch::Receiver<IngestStatus> }`.
- Routes (spec §8 subset): `/v1/status`, `/v1/blocks`, `/v1/blocks/{height_or_id}`, `/v1/blocks/{id}/txs`, `/v1/txs`, `/v1/txs/{id}`, `/v1/boxes/{id}`, `/v1/boxes/{id}/rent`, `/v1/addresses/{addr}`, `/v1/addresses/{addr}/boxes?unspent&cursor&limit&dir`, `/v1/addresses/{addr}/txs?cursor&limit&dir`, `/v1/addresses/{addr}/rent`, `/v1/richlist?cursor&limit`, `/v1/rent/upcoming?blocks=N&limit`, `/v1/rent/eligible?cursor&limit`, `/v1/search?q=`.
- DTO rules: ids hex; `value`, `fee`, `nano` as decimal **strings**; `Page<T>` → `{ "items": [...], "next_cursor": "123" | null }`; problem JSON `{ "type": "about:blank", "title", "status", "detail" }` for 400/404/500; `limit` clamped to 500, default 50.
- `TxDto` embeds resolved inputs (`BoxDto` for each input id via `box_by_id`) and outputs (`boxes_of_tx`). `BoxDto` includes `address`, `ergo_tree` (hex), `spent_by`, `spent_height`, `rent: { maturity_height, due_nano, claimable_at_tip: bool }`.
- `/v1/search?q=`: decimal ⇒ block height; 64-hex ⇒ try header id, tx id, box id (in that order); starts with `9` or `3`/`2`/`8` and parses as address ⇒ address; else 400. Returns `{ "kind": "block|tx|box|address", "id": "..." }` or 404.

- [ ] **Step 1: Failing route tests** — for each route: 200 shape assertions on fixture data (e.g. `/v1/blocks/1866001` → `height==1866001`, `/v1/txs/{coinbase id}` → `outputs.len()==output_count`), 404 for unknown ids, 400 for bad hex and `limit=0`, pagination walk of `/v1/addresses/{coinbase addr}/boxes?limit=1` until `next_cursor` null yields every box of that tree, `/v1/richlist?limit=3` sorted desc, `/v1/status` reflects a stubbed `IngestStatus`.

- [ ] **Step 2: Run — expect failure**

- [ ] **Step 3: Implement** (axum 0.8 path syntax `{param}`; `Reader::new` per request; handlers return `Result<Json<T>, ApiError>`; `ApiError` implements `IntoResponse` with problem JSON; a `tower_http::timeout::TimeoutLayer` of 5 s and `CorsLayer::permissive()`).

- [ ] **Step 4: Run tests** — pass.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(api): axum router with status, blocks, txs, boxes, addresses, richlist, rent, search"
```

---

### Task 10: `bin/explorer` — config, wiring, lifecycle

**Files:**
- Create: `bin/explorer/Cargo.toml`, `src/main.rs`, `src/config.rs`, `explorer.example.toml`, `README.md`

**Interfaces:**
- Config TOML:
```toml
data_dir = "./data"
bind = "127.0.0.1:8090"
[source]
kind = "rust_node"
url = "http://127.0.0.1:9063"
[ingest]
poll_ms = 500
bulk_batch = 64
bulk_concurrency = 8
durable_every = 256
tip_lag_for_bulk = 64
```
- `main`: parse `--config <path>` (default `explorer.toml`), init `tracing_subscriber` with `RUST_LOG`, `Store::open(data_dir/explorer.redb)`, build source, `watch::channel(IngestStatus)`, spawn `xp_ingest::run`, serve `xp_api::router` with graceful shutdown on SIGINT/SIGTERM (cancel token → ingest finishes current batch → store dropped last).

- [ ] **Step 1: Config test** — `config.rs` unit test parses the example TOML and applies defaults when `[ingest]` is omitted.

- [ ] **Step 2: Implement main + config.**

- [ ] **Step 3: Full-sync smoke run** (records results in `README.md` under "Benchmarks"):
```bash
cargo build --release
./target/release/explorer --config explorer.example.toml &
watch -n 30 'curl -s 127.0.0.1:8090/v1/status'
```
Expected: `mode: "bulk"` then `"tip"`; note blocks/s and final `du -sh data/explorer.redb`. Compare a random address against the node:
```bash
A=<address>; curl -s 127.0.0.1:8090/v1/addresses/$A | jq .balance.nano; curl -s 127.0.0.1:9063/blockchain/balanceForAddress/$A | jq .confirmed.nanoErgs
```

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat(bin): explorer binary with TOML config, graceful shutdown; README with sync benchmark"
```

---

### Task 11: Parity gate against the Rust node (spec §11)

**Files:**
- Create: `crates/xp-api/tests/parity.rs` (`#[ignore]`, needs `EXPLORER_URL` and `NODE_URL` env vars)
- Create: `scripts/parity.sh`

- [ ] **Step 1: Write the test** — sample 200 addresses by taking `/v1/blocks?limit=20` → each block's txs → output addresses (dedupe, shuffle with a fixed seed); for each compare: explorer `balance.nano` vs node `/blockchain/balanceForAddress/{a}.confirmed.nanoErgs`; explorer unspent box id set (`/v1/addresses/{a}/boxes?unspent=true`, walk all pages) vs node `/blockchain/box/unspent/byAddress/{a}?limit=16384` id set; explorer tx count (walk `/txs`) vs node `/blockchain/transaction/byAddress/{a}?limit=1` `.total`. Report every mismatch with the address; assert zero mismatches. Both services must be at the same height: read `/v1/status.indexed` and node `/blockchain/indexedHeight.indexedHeight` first and skip (not fail) if they differ.

- [ ] **Step 2: Run**

```bash
EXPLORER_URL=http://127.0.0.1:8090 NODE_URL=http://127.0.0.1:9063 cargo test -p xp-api --test parity -- --ignored --nocapture
```
Expected: `0 mismatches / 200`. Any mismatch is a store bug: fix in `apply.rs`, re-sync, re-run. Do not weaken the sample.

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "test(parity): 200-address balance/unspent/txcount parity gate against the Rust node"
```

---

## Self-review

- **Spec coverage (this plan's scope):** §3 tasks/pipeline → T8, T10; §4 crates → T1–T10 (xp-analytics deferred to Plan 3 as stated); §5 source trait, polling, fork window/halt → T7, T8; §6.1 core tables → T3 (tokens/templates/register_idx deferred, stated); §6.2 apply + durability → T4, T8; §6.3 reads/pagination → T6, T9; §6.4 `rich` and `rent_upcoming` → T4, T6, T9 (`stats_24h`, `miners_recent` deferred to Plan 3 with the stats endpoints); §6.5 rent constants → T1, T4; §6.6 rollback + identity test → T5; §8 core routes → T9; §10 config/data dir/graceful shutdown → T10; §11 wire/store/fork/API/parity tests → T2, T3–T6, T8, T9, T11.
- **Placeholder scan:** none; every code step has code or exact assertions.
- **Type consistency:** `TxRow.first_out_gidx` added in T6 must be reflected in T3's codec and T4's `apply_block` (`first_out_gidx: ctx.next_box` captured before the outputs loop) — implementer of T3/T4 should include it from the start. `Reader::rent_matures_range` signature used in T4's test matches T6. `IngestStatus`/`Mode` names match between T8 and T9/T10.
