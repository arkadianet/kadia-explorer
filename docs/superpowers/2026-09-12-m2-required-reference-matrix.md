# M2 steps 3–4: required-reference inventory

Written before helper changes, 2026-09-12, baseline b2a83c0. Scope: ordinary
Readers, DTO expansion/enrichment, and apply/rollback absence handling. No schema,
encoding, transition-model or UNDO-retention changes. A site is one semantic
lookup/absence branch; shared callers are named together. Cursor termination,
empty collections, cache misses followed by loads, query defaults, and arithmetic
without a missing-row branch are not references.

Classes (each numbered row has exactly one): **U** unknown requested entity →
404 (existing empty search/list behavior where applicable); **R** required indexed
row absent → integrity error; **O** optional enrichment absent → remains optional;
**P** permitted gap on a declared partial store → incomplete. Write-side new-state
and same-block deletion absences use O: they are legitimate absence, not missing
chain history, and have no HTTP status. This extension of “enrichment” is explicit
because the four requested classes otherwise omit normal write initialization.

“Yes” means there is a local witness; “No” identifies an information limit, not
permission to label arbitrary damage as pre-seed history. Direct primary lookups
cannot prove deletion versus never indexed without a retained reference. We do
not add full-table scans to infer that distinction.

| ID | Site / absent reference | Class | Can distinguish? / evidence and policy |
|---|---|---|---|
| U1 | read::tx_by_id, box_by_id: requested id; txs/boxes handlers and search | U | No: primary absence alone cannot distinguish deleted from unknown; direct unknown remains 404. Expansion uses R7/R8 instead. |
| U2 | read::height_of_header: unknown hash | U | No: missing reverse index alone cannot prove prior membership. |
| U3 | read::header_at: height outside indexed interval; blocks/search; txs_in_block | U | Yes for outside interval; absence inside known interval is R1. |
| U4 | read::tree_by_address and addresses::tree_of | U | No: invalid address, never-seen tree and deleted tree collapse to None. Preserve 404. |
| U5 | read_tokens::token; tokens::token_of; search | U | No: no mint row can mean unknown, deleted, or pre-seed mint. Direct requests retain 404. |
| U6 | read_tokens::template; templates handlers | U | No: primary absence alone cannot prove deletion. |
| U7 | composite/rent/register/token-holder lists: no membership/cursor exhausted; search no matches | U | No: deleted membership with no remaining witness is indistinguishable from empty. Existing empty pages/search 404 retained. |
| R1 | read::header_at / headers_desc: missing height within retained interval | R | Yes: tip and partial seed bound define contiguous headers; genesis itself has no header. |
| R2 | read::height_of_header: reverse index points to absent header | R | Yes: HEADER_BY_ID promises a HEADERS row. |
| R3 | read::resolve_tx (global and address tx lists) | R | Yes: TX_BY_GIDX / TREE_TXS promises TXS. |
| R4 | read::txs_in_block and txs_by_gidx: missing TX_BY_GIDX in allocated range | R | Yes: first_tx_gidx + tx_count, or META_NEXT_TX_GIDX for the global list. Global-list hole detection identified before changing that pager. |
| R5 | read::boxes_of_tx: missing BOX_BY_GIDX or BOXES | R | Yes: transaction output range, including on partial stores. |
| R6 | read::BoxResolver / tx_by_gidx_lookup: tree/token/template/register/history indexes | R | Yes: membership promises local gidx and primary row. Never a permitted partial gap. |
| R7 | dto::box_dto/from_reader: missing ERGO_TREES | R | Yes: every retained box was indexed with a tree, even in partial mode. |
| R8 | dto::tx_dto: missing input BOXES on non-partial store | R | Yes: absence forbidden without explicit partial declaration. Unseeded is not partial permission. |
| R9 | dto::box_dto: invalid registers_json | R | Yes: malformed stored JSON is not an absent optional register; valid JSON null remains valid. |
| R10 | read_tokens::resolve_token: either token listing index → TOKENS | R | Yes: retained mint index promises row, even in partial mode. |
| R11 | read_tokens::token_names / dto::fill_token_names: referenced token row on non-partial store | R | Yes at enrichment caller: box/balance asset promises mint history. Name itself is O1. |
| R12 | addresses::get_one / dto::address_dto: known tree → TREE_BALANCE | R | Yes: balances persist at zero; no fabricated zero counts or amounts. |
| R13 | addresses::get_one canonical tree reload | R | Yes: same Reader already found tree. |
| R14 | richlist::list: indexed tree → ERGO_TREES / TREE_BALANCE | R | Yes: rich entry promises both; check balance amount agrees. |
| R15 | tokens::holders: indexed holder tree → ERGO_TREES / TOKEN_HOLDER_AMT | R | Yes: holder entry promises both; check stored amount agrees. |
| R16 | templates::get_one: example_tree → ERGO_TREES | R | Yes: retained template has retained example tree. |
| R17 | rent::items_of / read rent lists: RENT_MATURES → BOXES | R | Yes: maturity entry promises local live box. |
| R18 | supply: recognized genesis → emission metadata, box, balance; impossible total | R | Yes after recognition; recognition limitation is O5. |
| R19 | history: anchor header, creator tx, gidx/box and known-tree tip balance | R | Yes: existing history integrity checks retained. Unknown historical address may have zero balance. |
| R20 | apply_batch: indexed tip → header | R | Yes: indexed height promises header. Already strict. |
| R21 | apply_block: missing input on non-partial store | R | Yes: existing strict branch. |
| R22 | extras::template_of and rollback::template_of | R | Yes: retained box promises tree. Already strict. |
| R23 | rollback: created_boxes → BOXES; tx_ids → TXS; height → HEADERS | R | Yes: rollback journal names local rows. Already strict. |
| R24 | rollback: spent_boxes → BOXES, excluding same-block creations | R | Yes: created_boxes explicitly identifies intentional earlier deletion. Missing pre-seed inputs never enter spent_boxes. |
| R25 | rollback::token_row_now (new/previous tokens) | R | Yes: journal names local token rows. Already strict. |
| R26 | rollback: current TREE_BALANCE before restoring previous balance | R | Yes: touched balances are retained, including zero. |
| R27 | tokens::row_mut: referenced mint row on non-partial store | R | Yes: mint is inserted before output/spend counter updates; absent row cannot be optional name. |
| R28 | apply_batch / txs_by_gidx: initialized store → allocation counters | R | Yes: indexed tip requires both counters; genesis seeding requires box counter, but legitimately has no tx counter yet. Identified in final review before tightening these defaults. |
| O1 | TokenRow name/description/decimals/token_type; dto::fill_token_names / token_kind | O | Yes: existing row with empty name or absent EIP-4 fields is valid; retain empty/null/default kind. |
| O2 | dto::rent_dto / box spent fields | O | Yes: spent=None means unspent; absent tip means no claimable-at-tip assertion. |
| O3 | extras::register_hex/on_output: absent R4–R9 | O | No for malformed JSON at this substring helper: it cannot validate full JSON. Stored DTO parse is R9; valid absent register stays absent. |
| O4 | emission_tree_hash / box_kind on unseeded store | O | Yes: no genesis seeding means no emission identity; default box kind remains. Seeded missing metadata is R18 when supply recognized. |
| O5 | read::mainnet_genesis: absent/mismatching mainnet identities | O | No: no network metadata; another chain and deleted mainnet identity can look alike. Keep supply complete=false with null totals. |
| O6 | apply/genesis: initial counters and no indexed tip | O | No for missing tip: fresh/seed-only state and deleted tip can coincide. Counters on a witnessed initialized store are distinguishable and required (R28). No blanket error for a fresh store. |
| O7 | apply::load_balance / genesis balance creation | O | No in shared credit loader: first credit and deleted previous balance look alike. Debit underflow remains strict on full stores; known read balance is R12. |
| O8 | extras::row: initial template | O | No in shared output loader: new template versus deleted aggregate requires other index evidence. Existing debit checks retained. |
| O9 | tokens::holder_mut / finish; rollback::undo_holders current amount | O | No: holder rows legitimately disappear at zero; previous journal amount does not determine current membership. Existing full-store debit underflow checks retained. |
| O10 | rollback: previous balance/token/template/holder None | O | Yes: journal records first creation; absence means remove new state. |
| O11 | rollback: spent box also in created_boxes | O | Yes: intentionally removed in earlier un-create loop; no restore. |
| O12 | apply token-vector insertion, mint absent, no positive burn delta; genesis empty emission selection | O | Yes: normal new asset/no mint/no burn/no genesis candidate, not required row resolution. |
| P1 | dto::tx_dto / apply input lookup: absent input on declared partial store | P | No: input id contains no creation height, so pre-seed versus deleted in-range primary is indistinguishable here. Allow unresolved only with explicit META_PARTIAL_FROM; report incomplete. A dangling gidx is separately R6. |
| P2 | token_names / Tokens::row_mut: absent mint on declared partial store | P | No: token id gives no mint height; cannot prove pre-seed origin locally. Allow missing metadata/counters only on declared partial store, report incomplete. Token listing dangling indexes remain R10. |
| P3 | apply debit_balance/sub_tokens, Tokens::on_spend/finish, Extras::on_spend: partial saturation | P | No: existing partial bookkeeping lacks provenance to distinguish historical deficit from damage. Retain policy and report all partial-store amounts as incomplete, never certified totals. |
| P5 | apply::block_reward: missing first input tree on partial store | P | No: same missing-input provenance limit as P1; legacy reward fallback remains, response coverage header marks it incomplete. No reward algorithm changes. |
| O13 | apply::block_reward: no first transaction/input/output | O | Yes for structural absence in supplied block; empty block reward remains zero. |
| P4 | ordinary address/token/rich/rent lists and totals on partial/uninitialized store | P | Yes for lack of full coverage, not for cause of individual gap. Add snapshot-derived X-Explorer-Completeness: incomplete; complete means full history coverage, not a global corruption audit. Existing supply/history completeness remains route-specific. |

Counts: U **7**, R **28**, O **13**, P **5** = **53** semantic sites.
Uncertain classifications/local indistinguishability: U1, U2, U4–U7, O3,
O5–O9, P1–P3, P5. No class assignment is a claim that these can detect all corruption.
Missing secondary memberships with no surviving reference cannot be detected by a
bounded ordinary read. This task does not certify the production store.

Implementation/fixture evidence is recorded below after work, without changing this
pre-implementation inventory's numbering.

## Step 4 implementation and evidence

The response header `X-Explorer-Completeness` is `complete` when that response's
Reader sees genesis seeding and no partial marker, and `incomplete` otherwise.
Handlers without a Reader (status) omit it. It describes history coverage, not a
global integrity audit, pagination exhaustion or rent collectibility. Incomplete
amounts/counts describe only observed history and must not be interpreted as exact
chain totals, including when their numeric value is zero. Existing body fields,
nullability, arrays, and cursors are preserved. Existing rent `complete` still
means the indexed window fits within the requested limit; both signals apply.
A failed required reference returns only a 500 problem with `code=integrity_error`.
The underlying cause is logged, not exposed in the public detail.

Eight new `integrity_*` tests in `crates/xp-api/tests/routes.rs` use isolated
`tempfile` stores and the in-process router (no sockets). Mutation occurs only
after dropping the fixture's Store; raw redb closes before reopening the Store.

| Fixture | References / assertion |
|---|---|
| required_rows_fail_full_store_lists_details_and_totals | R1–R8, R10–R18, R28: remove allocation counter, header, transaction, tx gidx, output gidx, box, tree, input, token, balance, holder amount, emission metadata. Exercise block/global/address/token/template/register/rent routes and historical sums. Each damaged file also checks unrelated box/tx/token/template/block/address ids stay 404. R13 cannot be independently removed between same-snapshot lookups. |
| invalid_register_json_fails_expansion_but_valid_null_is_preserved | R9: malformed JSON fails list and detail expansion; valid stored JSON null retains its value. |
| partial_preseed_inputs_and_mints_are_explicitly_incomplete | P1/P2/P4: actually start after the mint/input creation, with no injected damage; input box and token name remain null, observed nonzero amounts remain, and tx/list/address/rich/rent responses explicitly report incomplete. |
| partial_in_range_dangling_indexes_still_fail | R1/R4/R5/R10 on declared partial stores, independently from the permitted pre-seed test. |
| full_store_optional_eip4_fields_and_unknown_ids_stay_valid | O1: full-store empty EIP-4 name and null decimals remain valid, with complete coverage; unknown ids still 404. |
| rollback_missing_spent_box_is_atomic_in_full_and_partial_stores | R24: missing previously indexed spent box aborts without changing existing fingerprint or tip in either mode. Existing same-block create/spend tests remain unchanged. |
| required_apply_and_rollback_references_abort_atomically | R20–R23/R25–R28: missing counter/input/tree/token/tip before apply, and created box/tree/tx/header/balance/token before rollback. Assert integrity error, unchanged fingerprint and tip. No new UNDO model or retention assertions. |
| rent_eligible_does_not_skip_a_missing_box | R17: fixture maturity keys placed within its tip, then remove a referenced box; eligible page fails. Upcoming failure is independently covered above. |

R19 is exercised by deleting a historical creator transaction or indexed box and
requesting both historical balance and box pages. This also exposed the preexisting
history `integrity` helper's untyped Internal error; it now emits the same additive
integrity code as ordinary reads.

Existing tests changed meaning: **none**. The store token-underflow unit fixture
now passes its already-intended full/partial mode into `Tokens::open`; its
assertions are unchanged. No existing lenient test was rewritten or weakened.
The original complete-window assertions on partial rent fixtures remain intact:
window truncation and chain-history coverage are separate signals.

The global tx pager now uses its allocated gidx interval instead of scanning only
surviving rows. Header paging similarly checks the retained height interval. Sparse
secondary memberships without any surviving witness remain undetectable, as
recorded in U7. Optional names, unknown primary ids and declared partial gaps were
not indiscriminately converted to errors.

Inventory review additions (before their corresponding helper changes): the global
allocated tx range was added to R4; structural/partial reward fallback sites were
added as O13/P5 (reward code unchanged); initialized allocation counters were split
from O6 into R28. The original 50-row inventory existed before implementation;
final inventory is 53 rows. These refinements do not imply a prior complete audit.

## Gate result

Candidate: `b2a83c0914cf980c04eb0775ea7589ed7069f4ce` plus this uncommitted diff.
Final command: `CARGO_TARGET_DIR=$PWD/target ./scripts/check.sh all`, **exit 1**.
Artifacts: [results](../../artifacts/check/all-q4clyPVj/results.log),
[revision/tool/diff manifest](../../artifacts/check/all-q4clyPVj/manifest.log),
[log hashes](../../artifacts/check/all-q4clyPVj/logs.sha256).

| Gate | Result |
|---|---|
| cargo fmt --all -- --check | PASS |
| cargo clippy --workspace --all-targets -- -D warnings | PASS |
| cargo test --workspace --no-fail-fast | All non-socket tests pass; NOT VERIFIED overall. `xp-source --test fallback` (6 cases) and `--test rust_node` (13 socket cases) are **not run: sandbox**, bind returns PermissionDenied/EPERM. One non-socket rust_node case passes. Three preexisting ignored tests remain unrun. |
| API route tests within G | 66 passed, 0 failed, 1 existing ignored; includes all eight new integrity tests |
| npm test && npm run check && npm run lint && npm run build | PASS |
| npx playwright test --workers=4 | **not run: sandbox**; mock server bind to 127.0.0.1:18099 returns EPERM |

The earlier gate at `artifacts/check/all-v8945BDe` also exited 1 for socket
refusals; the final run above supersedes it after R28's counter fix. The focused
command `CARGO_TARGET_DIR=$PWD/target cargo test -p xp-api --test routes integrity_`
passed all eight tests before the final G run. The initial attempt with Cargo's
configured home-cache target was refused as read-only; subsequent runs use only
repository `target/`, never a target directory under `/tmp`.

No `npm ci`, dependency addition, schema/version/row-encoding change, production
data operation, commit, or push. M2 steps 5–7 (generated transitions and UNDO
verification) remain unstarted. Gate evidence covers behavior of temporary
fixtures, not integrity of the existing production file.
