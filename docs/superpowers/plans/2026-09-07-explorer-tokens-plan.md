# Explorer Plan 2 — Tokens, Templates, Holders, Register Search — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Index Ergo tokens (EIP‑4 metadata, supply, burns, holders), script templates and register values, serve them over `/v1`, and add token, template and register-search pages to the frontend — completing spec §6.1 and §8 for the deferred rows/routes.

**Architecture:** Schema v2 of the redb store: new tables written in the same `apply_batch` transaction (and reversed by `rollback_to` via extended `UndoRow`), `Reader` range queries over composite keys, axum routes with the existing DTO conventions, SvelteKit pages built from the existing components. A store from schema v1 is refused at open (resync required; Nuremberg is resynced once at the end).

**Tech Stack:** unchanged (Rust: redb 2.6, axum 0.8, blake2; frontend: SvelteKit 2 / Svelte 5, Vitest, Playwright).

**Spec:** `docs/superpowers/specs/2026-09-05-ergo-explorer-design.md` §6.1 (rows `templates … register_idx`), §6.2 step 4/5, §8 (token/template/register routes, search), §11 (parity). Frontend spec `docs/superpowers/specs/2026-09-06-explorer-frontend-design.md` §10 phase 2.

## Global Constraints

- All redb keys big-endian fixed width; composite keys `(hash32, gidx u64 BE)`, `(token_id, amount u64 BE, tree_hash)`, `(count u64 BE, token_id)`, `(reg u8, blake2b256(value) 32, gidx u64 BE)` (spec §6.1).
- Row codecs: fixed ints BE, `Vec`/`String` u32-length-prefixed, `Option` 1-byte tag; `decode` returns `StoreError::Corrupt`, never panics (Plan 1 Task 3 rules).
- Undo must capture everything needed to reverse a block exactly; `apply(b); rollback(b)` byte-identical store (spec §6.6) — the existing fingerprint identity tests must stay green and be extended.
- `SCHEMA_VERSION` becomes `2`; `Store::open` refuses other versions with `Corrupt("schema version mismatch")`; README upgrade note; existing stores are resynced.
- Token mint rule (Ergo consensus): a token whose id equals the id of the transaction's FIRST input box is minted in that transaction; the minted amount is the total of that token in the outputs. EIP‑4 metadata is read from the FIRST output carrying the token: R4 name, R5 description, R6 decimals (ASCII digits), R7 type — all `Coll[Byte]` constants (`0e` + VLQ + bytes); absent/invalid ⇒ empty/`None`.
- Burn rule: per transaction and token, `burned += max(0, in − out)` (mint tx: in = 0 so nothing burns).
- Amounts on the wire are decimal strings; ids hex; cursor pagination `limit ≤ 500`; every commit `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` green; frontend `npm test && npm run check && npm run lint && npm run build` green, e2e `--workers=4`.
- Commit trailer: `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>` and `Claude-Session: https://claude.ai/code/session_01Jegbu3j98VqxmXFrcFCFdS`.

---

## File structure

```
crates/xp-wire/src/registers.rs        # coll_byte(hex) -> Option<Vec<u8>>, eip4::parse(registers) -> Eip4
crates/xp-store/src/tables.rs          # + TEMPLATES TEMPLATE_BOXES TEMPLATE_UNSPENT TOKENS TOKENS_BY_GIDX TOKENS_BY_HOLDERS
                                       #   TOKEN_BOXES TOKEN_UNSPENT TOKEN_HOLDERS TOKEN_HOLDER_AMT REGISTER_IDX; SCHEMA_VERSION=2
crates/xp-store/src/keys.rs            # + k_token_holder, k_by_count, k_register
crates/xp-store/src/rows.rs            # + TemplateRow, TokenRow; BalanceRow.tx_count; UndoRow v2 fields
crates/xp-store/src/apply.rs           # templates/register/tx_count wiring; calls tokens.rs
crates/xp-store/src/tokens.rs          # token mint/transfer/burn/holder bookkeeping (apply side)
crates/xp-store/src/rollback.rs        # reversal of all v2 tables
crates/xp-store/src/read.rs            # token/template/register queries
crates/xp-api/src/dto.rs               # TokenInfoDto, TokenHolderDto, TemplateDto, AddressDto.tx_count, BoxDto.tokens enrichment
crates/xp-api/src/handlers/{tokens,templates,registers}.rs ; search.rs (token/template kinds)
crates/xp-api/tests/{routes.rs,parity.rs}
frontend/src/lib/api/{types,endpoints}.ts ; src/lib/search/classify.ts
frontend/src/routes/tokens/+page.{ts,svelte} ; token/[id]/+page.{ts,svelte} ; template/[hash]/+page.{ts,svelte}
frontend/src/routes/search/+page.svelte (register search form) ; +layout.svelte (nav) ; +page.svelte (Top tokens card)
frontend/tests/e2e/{tokens,template,registers}.spec.ts ; tests/e2e/mock/{fixtures,handlers}.ts
deploy: scripts/deploy_frontend.sh unchanged; Nuremberg resync
```

---

### Task 1: `xp-wire` register helpers and EIP‑4 parsing

**Files:** create `crates/xp-wire/src/registers.rs`; modify `crates/xp-wire/src/lib.rs` (`pub mod registers;`); test `crates/xp-wire/tests/registers.rs`.

**Interfaces produced:**
```rust
pub fn coll_byte(hex: &str) -> Option<Vec<u8>>;       // "0e" + VLQ len + bytes, exact consumption, else None
pub fn coll_byte_utf8(hex: &str) -> Option<String>;   // coll_byte + String::from_utf8 (lossy replaced by None)
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Eip4 { pub name: String, pub description: String, pub decimals: Option<u8>, pub token_type: Option<String> }
pub fn parse_eip4(registers_json: &str) -> Eip4;      // registers_json is the node's {"R4": "<hex>", ...} object
pub fn minted_token_of(first_input: &Hash32, outputs: &[DecodedBox]) -> Option<(Hash32, u64)>; // (token id == first input id, total amount)
```

- [ ] **Step 1: Failing tests**
```rust
use xp_wire::registers::{coll_byte, coll_byte_utf8, parse_eip4, Eip4};
#[test] fn coll_byte_decodes() { assert_eq!(coll_byte("0e0568656c6c6f"), Some(b"hello".to_vec())); assert_eq!(coll_byte("0e05ab"), None); assert_eq!(coll_byte("0500"), None); assert_eq!(coll_byte("0e0568656c6c6f00"), None); }
#[test] fn eip4_from_registers() {
    let regs = r#"{"R4":"0e0568656c6c6f","R5":"0e0577686174","R6":"0e0132","R7":"0e020101"}"#;
    assert_eq!(parse_eip4(regs), Eip4 { name: "hello".into(), description: "what".into(), decimals: Some(2), token_type: Some("0101".into()) });
    assert_eq!(parse_eip4("{}"), Eip4::default());
    assert_eq!(parse_eip4(r#"{"R6":"0e02ffff"}"#).decimals, None);
}
```
(`token_type` is the raw hex of the R7 bytes; the frontend maps `0101` → NFT picture, `0102` → NFT audio, `0103` → NFT video, `0201` → membership; everything else "other".) Add a `minted_token_of` test on fixture block 1866000 if it contains a mint; if none of the three fixtures mints a token, fetch `tests/fixtures/blocks/<h>.json` for a known mint tx (find one via `/blockchain/token/byId/<any token id>` → `boxId` → tx → height) and commit it.

- [ ] **Step 2: Run** `cargo test -p xp-wire` → fail. **Step 3: Implement** (reuse the VLQ reader from `boxser.rs` via `pub(crate)`). **Step 4: Green.** **Step 5: Commit** `feat(wire): register Coll[Byte] decoding and EIP-4 token metadata parsing`.

---

### Task 2: Schema v2 tables, keys, rows, undo

**Files:** `crates/xp-store/src/{tables,keys,rows}.rs`; tests in the same files (proptest round trips + truncation).

**Interfaces produced:**
```rust
// tables.rs
pub const TEMPLATES: Tbl = TableDefinition::new("templates");           // template_hash -> TemplateRow
pub const TEMPLATE_BOXES: Tbl = ...("template_boxes");                  // (template_hash, gidx) -> ()
pub const TEMPLATE_UNSPENT: Tbl = ...("template_unspent");
pub const TOKENS: Tbl = ...("tokens");                                  // token_id -> TokenRow
pub const TOKENS_BY_GIDX: Tbl = ...("tokens_by_gidx");                  // mint box gidx u64 -> token_id (newest-first listing)
pub const TOKENS_BY_HOLDERS: Tbl = ...("tokens_by_holders");            // (holder_count u64 BE, token_id) -> ()
pub const TOKEN_BOXES: Tbl = ...("token_boxes");                        // (token_id, gidx) -> ()
pub const TOKEN_UNSPENT: Tbl = ...("token_unspent");
pub const TOKEN_HOLDERS: Tbl = ...("token_holders");                    // (token_id, amount BE, tree) -> ()
pub const TOKEN_HOLDER_AMT: Tbl = ...("token_holder_amt");              // (token_id, tree) -> u64 BE
pub const REGISTER_IDX: Tbl = ...("register_idx");                      // (reg u8, blake2b256(raw value bytes), gidx) -> ()
pub const SCHEMA_VERSION: u32 = 2;  ALL extended.
// keys.rs
pub fn k_token_holder(token: &Hash32, amount: u64, tree: &Hash32) -> [u8; 72];
pub fn k_token_tree(token: &Hash32, tree: &Hash32) -> [u8; 64];
pub fn k_by_count(count: u64, id: &Hash32) -> [u8; 40];
pub fn k_register(reg: u8, value_hash: &Hash32, g: Gidx) -> [u8; 41];
// rows.rs
pub struct TemplateRow { pub box_count: u64, pub unspent_count: u64, pub first_seen: u32, pub example_tree: Hash32 }
pub struct TokenRow { pub mint_tx: Hash32, pub mint_box: Hash32, pub mint_height: u32, pub mint_gidx: Gidx, pub name: String, pub description: String, pub decimals: Option<u8>, pub token_type: Option<String>, pub emission: u64, pub burned: u64, pub holder_count: u64, pub box_count: u64 }
pub struct BalanceRow { …existing…, pub tx_count: u64 }   // appended last in encoding
pub struct UndoRow { …existing…, pub new_templates: Vec<Hash32>, pub prev_templates: Vec<(Hash32, TemplateRow)>,
    pub new_tokens: Vec<Hash32>, pub prev_tokens: Vec<(Hash32, TokenRow)>, pub prev_holder_amts: Vec<(Hash32, Hash32, Option<u64>)>,
    pub register_keys: Vec<(u8, Hash32, Gidx)> }
```
- [ ] Tests: proptest round trips for `TemplateRow`, `TokenRow`, extended `BalanceRow`/`UndoRow`; truncation ⇒ `Corrupt`; key ordering tests (`k_token_holder` orders by amount within a token; `k_by_count` by count). Commit `feat(store): schema v2 tables, keys and row codecs for tokens/templates/registers`.

---

### Task 3: Apply — templates, register index, tx_count

**Files:** `crates/xp-store/src/apply.rs`, `crates/xp-store/src/lib.rs` (`Store::open` schema 2), tests `crates/xp-store/tests/apply.rs`.

- In `insert_output` (or right after it in `apply_block`): `TEMPLATE_BOXES`/`TEMPLATE_UNSPENT` insert keyed by `TreeRow.template_hash` (from `upsert_tree`'s row), `TemplateRow` upsert (`box_count += 1`, `unspent_count += 1`, `first_seen` on create, record `new_templates` or `prev_templates` in undo); `REGISTER_IDX` insert for each present register R4..R9: key `(reg_index 4..9 as u8, blake2b256(raw bytes from registers_json hex), gidx)`, pushed onto `undo.register_keys`.
- On spend: `TEMPLATE_UNSPENT` remove and `TemplateRow.unspent_count -= 1` (prev row recorded once per block per template).
- `tx_count`: for every distinct tree touched by a tx, `BalanceRow.tx_count += 1` (the same set used for `TREE_TXS`).
- Tests: fixture apply asserts `TemplateRow` for the P2PK template counts equal the number of P2PK outputs; register index contains the (R4, hash, gidx) key for a fixture box with R4; `tx_count` for the coinbase tree equals the number of txs touching it; identity test (apply+rollback) still passes AFTER Task 4 (rollback lands there) — for this task, add the apply assertions only and mark the identity test `#[ignore]` temporarily with a note, re-enabled in Task 4.
- Commit `feat(store): index script templates, register values and per-address tx counts`.

---

### Task 4: Apply — tokens; rollback for all v2 tables

**Files:** create `crates/xp-store/src/tokens.rs`; modify `apply.rs`, `rollback.rs`; tests `crates/xp-store/tests/{tokens,rollback}.rs`.

**Interfaces produced:**
```rust
// tokens.rs (pub(crate))
pub(crate) struct TokenDelta { pub in_: u64, pub out: u64 }
pub(crate) fn token_deltas(tx: &DecodedTx, input_boxes: &[BoxRow]) -> HashMap<Hash32, TokenDelta>;
pub(crate) fn apply_tx_tokens(ctx: &mut Ctx, tx: &DecodedTx, input_boxes: &[BoxRow], out_gidx_start: Gidx) -> Result<(), StoreError>;
```
Rules: for each output token `(id, amount)`: `TOKEN_BOXES`/`TOKEN_UNSPENT` insert, holder amount `TOKEN_HOLDER_AMT[(id,tree)] += amount` (record prev), `TOKEN_HOLDERS` re-keyed (remove old `(id, prev_amt, tree)`, insert new), `TokenRow.box_count += 1`, holder_count adjusted when a holder goes 0→>0 or >0→0 (and `TOKENS_BY_HOLDERS` re-keyed). For each input box token: `TOKEN_UNSPENT` remove, holder amount −=. Mint: `minted_token_of(first_input_id, outputs)` ⇒ new `TokenRow` with `Eip4` from the first output carrying the token, `emission` = minted amount, `TOKENS_BY_GIDX[mint box gidx] = id`, undo `new_tokens`. Burn: `burned += in − out` when positive (prev `TokenRow` recorded). Partial stores: unknown input boxes are skipped (no token deltas).
Rollback: reverse every v2 write using the undo fields (delete register keys, restore prev holder amts and re-key holders, restore prev token/template rows or delete new ones, remove `TOKENS_BY_GIDX`/`TOKENS_BY_HOLDERS` entries accordingly, restore `tx_count` via `prev_balances` — already whole-row).
- Tests: `apply_then_rollback_is_identity` re-enabled and extended over the four fixture blocks (incl. 1702686) — fingerprint equality; token test on a fixture with a mint (from Task 1) asserting `TokenRow.name/emission`, holder rows, `TOKENS_BY_HOLDERS` key; a burn test using a synthetic block (tx with token in > out) asserting `burned`; holder count transitions 0→1→0.
- Commit `feat(store): token mint/transfer/burn/holder indexing and full v2 rollback`.

---

### Task 5: Reader queries

**Files:** `crates/xp-store/src/read.rs` (+ a `read_tokens.rs` if read.rs would exceed ~700 lines), tests `crates/xp-store/tests/read.rs`.

**Interfaces produced:**
```rust
pub fn token(&self, id: &Hash32) -> Result<Option<TokenRow>, StoreError>;
pub fn tokens_newest(&self, cursor: Option<Gidx>, limit: usize) -> Result<Page<(Hash32, TokenRow)>, StoreError>;      // via TOKENS_BY_GIDX desc
pub fn tokens_by_holders(&self, cursor: Option<(u64, Hash32)>, limit: usize) -> Result<(Vec<(Hash32, TokenRow)>, Option<(u64, Hash32)>), StoreError>;
pub fn token_holders(&self, id: &Hash32, cursor: Option<(u64, Hash32)>, limit: usize) -> Result<(Vec<(Hash32 tree, u64 amount)>, Option<(u64, Hash32)>), StoreError>; // desc by amount
pub fn token_boxes(&self, id: &Hash32, unspent_only: bool, cursor: Option<Gidx>, limit: usize, dir: Dir) -> Result<Page<(Hash32, BoxRow)>, StoreError>;
pub fn template(&self, hash: &Hash32) -> Result<Option<TemplateRow>, StoreError>;
pub fn template_boxes(&self, hash: &Hash32, unspent_only: bool, cursor: Option<Gidx>, limit: usize, dir: Dir) -> Result<Page<(Hash32, BoxRow)>, StoreError>;
pub fn boxes_by_register(&self, reg: u8, value_hash: &Hash32, cursor: Option<Gidx>, limit: usize, dir: Dir) -> Result<Page<(Hash32, BoxRow)>, StoreError>;
pub fn token_names(&self, ids: &[Hash32]) -> Result<Vec<(Hash32, String, Option<u8>)>, StoreError>;   // for BoxDto enrichment
```
Reuse `page_composite`. Tests over fixtures for each (holders order desc, cursor round trip, unspent filter, register lookup hits the fixture box).
- Commit `feat(store): Reader queries for tokens, templates and register search`.

---

### Task 6: API — tokens, templates, registers, search, enrichment

**Files:** `crates/xp-api/src/dto.rs`, `handlers/{tokens,templates,registers}.rs`, `handlers/search.rs`, `handlers/addresses.rs` (tx_count), `lib.rs` routes; tests `crates/xp-api/tests/routes.rs`.

Routes/DTOs (spec §8): `GET /v1/tokens?cursor&limit&sort=newest|holders` → `PageDto<TokenInfoDto>`; `GET /v1/tokens/{id}` → `TokenInfoDto { id, name, description, decimals, token_type, kind: "nft-picture"|"nft-audio"|"nft-video"|"membership"|"token", emission, burned, supply (= emission − burned), holder_count, box_count, mint_tx, mint_box, mint_height }`; `GET /v1/tokens/{id}/holders?cursor&limit` → `PageDto<TokenHolderDto { address, tree_hash, amount, share_pct (2 decimals of supply) }>` with cursor `"<amount>:<treehex>"`; `GET /v1/tokens/{id}/boxes?unspent&cursor&limit&dir`; `GET /v1/templates/{hash}` → `TemplateDto { hash, box_count, unspent_count, first_seen, example_address }`; `GET /v1/templates/{hash}/boxes?unspent&cursor&limit&dir`; `GET /v1/registers/{reg}/{valueHex}/boxes?cursor&limit&dir` (reg ∈ R4..R9; valueHex is the serialised constant hex; the handler hashes it); `/v1/search`: 64-hex now tries header → tx → box → token → template; `AddressDto.tx_count`; `BoxDto.tokens[]` gains `name: string|null, decimals: number|null` (one `token_names` batch per response). Tests for each incl. 404s, cursor round trips, search resolving a token id and a template hash, `tx_count` matches a walk on a fixture address.
- Commit `feat(api): token, template and register-search endpoints; search kinds; tx_count; token names on boxes`.

---

### Task 7: Parity gate v2

**Files:** `crates/xp-api/tests/parity.rs`.
- Use `AddressDto.tx_count` instead of walking `/txs` (drop the page-cap skip for tx counts); add token checks: for 20 token ids taken from sampled boxes, compare `/v1/tokens/{id}` `emission`/`name` with the node's `/blockchain/token/byId/{id}` (`emissionAmount`, `name`) and the unspent box count with `/blockchain/box/unspent/byTokenId/{id}?limit=16384` length (skip above cap). Update `scripts/parity.sh` docs. Run against the local store only after the resync (Task 12) — for this task, verify compile + skip paths.
- Commit `test(parity): tx_count from the API and token emission/holders checks`.

---

### Task 8: Frontend API layer + classifier

**Files:** `frontend/src/lib/api/{types,endpoints}.ts`, `src/lib/search/classify.ts`, unit tests.
- Types: `TokenInfoDto`, `TokenHolderDto`, `TemplateDto`, `AddressDto.tx_count`, `TokenDto { id, amount, name: string|null, decimals: number|null }`, `SearchDto.kind` adds `'token' | 'template'`. Endpoints: `tokens(sort, cursor, limit)`, `token(id)`, `tokenHolders(id, cursor, limit)`, `tokenBoxes(id, unspent, cursor, limit, dir)`, `template(hash)`, `templateBoxes(...)`, `boxesByRegister(reg, valueHex, cursor, limit, dir)`. `routeFor` unchanged (hex32 still resolves via API); `/search/+page.ts` maps `token` → `/token/{id}`, `template` → `/template/{hash}`. `formatTokenAmount(amount: string, decimals: number|null)` (BigInt, inserts the decimal point) with tests (`"12345", 2` → `"123.45"`; `"5", 3` → `"0.005"`; `null` decimals → grouped integer).
- Commit `feat(frontend): token/template API types, endpoints, amount formatting, search kinds`.

---

### Task 9: Frontend token pages

**Files:** `src/routes/tokens/+page.{ts,svelte}`, `src/routes/token/[id]/+page.{ts,svelte}`, `src/lib/components/TokenBadge.svelte` (kind pill), nav item "Tokens" in the sidebar, `src/lib/components/Icon.svelte` (+ `token`, `image` icons).
- `/tokens`: header with a sort toggle (Newest | Most held); table (glass card) Name (link, fallback truncated id), Kind pill, Supply (formatted with decimals), Holders, Minted at (block link); infinite list.
- `/token/[id]`: page head (name, id copy, kind pill), facts (supply, emission, burned, decimals, holders, boxes, minted in tx/box/height, description), tabs Holders (rank, address link, amount, share bar) | Boxes (unspent/all toggle); 404 ⇒ "Token not found" page.
- Commit `feat(frontend): tokens list and token detail pages`.

---

### Task 10: Frontend templates, register search, enrichment, home card

**Files:** `src/routes/template/[hash]/+page.{ts,svelte}`; `src/routes/search/+page.svelte` (advanced: "Find boxes by register value" form: select R4..R9 + hex input → `/registers/…` results list in-page); `src/lib/components/BoxCard.svelte` + address/box pages: token names and decimal-formatted amounts, token links to `/token/{id}`; `src/routes/+page.svelte`: "Top tokens" card (most held, top 5) next to the rich list (adjust grid to three cards: rent, tokens, holders); box page: template hash links to `/template/{hash}`.
- Commit `feat(frontend): template page, register search, token enrichment, top-tokens card`.

---

### Task 11: Frontend e2e for the new routes

**Files:** `tests/e2e/mock/{fixtures,handlers}.ts` (tokens from fixture assets: build `TokenInfoDto`s for every token id seen, holders from unspent boxes, one synthetic EIP‑4 token with name/decimals; templates from tree hashes; register index from fixture registers), `tests/e2e/{tokens,template,registers}.spec.ts`, README coverage list.
- Specs: `/tokens` renders and sorts; `/token/{id}` facts + holders tab + boxes tab; search resolves a token id to `/token/…` and a template hash to `/template/…`; register search returns the fixture box; address page shows token names.
- Commit `test(frontend): mock tokens/templates/registers and e2e specs`.

---

### Task 12: Resync + deploy + docs

**Files:** `bin/explorer/README.md` (schema v2 upgrade note, new endpoints), `frontend/README.md` (routes), `CHANGELOG.md` (new, summarising Plans 1–2).
- Build release; deploy binary to Nuremberg (`/opt/explorer/explorer`), delete `data/explorer.redb`, restart service; deploy frontend (`scripts/deploy_frontend.sh`); verify `/v1/status` and `/tokens` render (empty until synced); document.
- Commit `docs: changelog and schema v2 upgrade notes`.

---

## Self-review
- Spec §6.1 rows templates/template_boxes/template_unspent/tokens/token_boxes/token_unspent/token_holders/token_holder_amt/register_idx → T2–T4 (plus TOKENS_BY_GIDX/TOKENS_BY_HOLDERS added for listing/sorting); §6.2 step 4 (template upsert, token mint) and step 5 (burns) → T3–T4; §6.6 rollback → T4; §8 token/template/register routes and search order → T6; §11 parity → T7; frontend phase 2 → T8–T11; resync/deploy → T12.
- Placeholders: none. Type consistency: `TokenRow` fields used in T5/T6 match T2; `Page`/`Dir` reuse Plan 1 types; cursor string formats mirror Plan 1's (`"<u64>:<hex>"`).
