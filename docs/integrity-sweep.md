# Pre-deploy integrity sweep

Build and run against an explicitly supplied, quiesced store:

```sh
CARGO_TARGET_DIR="$PWD/target" cargo build -p xp-store --release --example integrity-sweep
./target/release/examples/integrity-sweep /explicit/offline/store.redb > integrity.jsonl
status=$?
```

Stop the owner before running. The executable takes exactly one path; there is no
path default, scratch directory, repair mode, or writable mode. Never aim output
redirection at the source file. No production-store audit has been performed by
this change.

Exit status is **0** for a completed scan with no findings within the reported
scope, **1** for a completed scan with findings, and **2** for inability to complete
(including lock refusal, missing path/table, unsupported schema, malformed row,
I/O error, or output failure). Process termination/signals also produce nonzero
status. An interrupted stream cannot certify a pass: require both exit 0 and the
final `summary` with `scan_complete:true`. Output failure after writing a summary
still exits 2. Errors attempt an `incomplete` record and always diagnose to stderr.

JSONL starts with the indexed tip height/id, partial declaration, genesis coverage,
mainnet recognition, and all 28 matrix coverage statements. Each finding carries
an R ID, source table/witness, hexadecimal witness key, and target reference or
failed condition. Findings are emitted as encountered; the final summary repeats
scope/tip and provides checks and findings for every R ID, plus P1/P2 permitted-gap
counts. A single missing primary can violate multiple references and matrix rows;
counts are violated checks, not distinct missing entities. `checks:0` means no
applicable witness was visited, not independent proof of that invariant.

The order is fixed scan-stage order, ascending redb byte keys, ascending allocated
ranges, and encoded vector order within each row. No timestamps or durations enter
JSONL. Progress goes to stderr at each stage and about every five seconds during
traversal; output flushes on that cadence and at start/completion. One immutable
snapshot and the exclusive lock cover the whole scan.

Progress reports `stage=... completed=N/T rows=N/T checks=N findings=N
stage_eta_s=... overall_eta_s=unavailable(mixed_stage_costs)`. `completed`
counts fully checked witnesses in the current stage; `rows` retains its original
meaning (table rows entered). The row denominator sums `len()` for the 23 tables
actually walked, from the same immutable snapshot, before checking starts.
Each `len()` reads redb's stored entry count: 23 table opens/metadata lookups,
constant count work per table, no entry scan or row decoding. Lookup-only tables
are excluded. The retained-height interval and allocated-tx range have separate
exact stage totals from metadata; they are not table rows. Metadata and emission
checks each count as one completed group. All traversal phases are bounded;
checks within a row are variable and do not have a precomputed denominator.
Row percentages therefore describe traversal coverage, not elapsed-time fraction.

Stages, in order: `metadata`, `retained_height`, `allocated_tx`, `headers`,
`header_by_id`, `tx_by_gidx`, `tree_txs`, `txs`, `box_by_gidx`, `tree_boxes`,
`tree_unspent`, `template_boxes`, `template_unspent`, `token_boxes`,
`token_unspent`, `register_idx`, `rent_matures`, `boxes`, `ergo_trees`,
`tree_balance`, `tokens_by_gidx`, `tokens_by_holders`, `templates`, `rich`,
`token_holders`, `emission`, `undo`. Stage starts and ends are reported even
for empty stages, alongside the five-second updates during checks.

`stage_eta_s` estimates only the current stage from completed witnesses over a
recent window of at most 60 seconds, sampled about every five seconds, with
at least 15 seconds of warm-up. Rates reset at every stage. ETA explicitly says
`unavailable(warming_up)`, `unavailable(no_recent_progress)`, or
`unavailable(stale_window)` (an operation outlasted the sampling window);
a finished/empty stage reports zero. A row with many nested checks can continue
reporting without counting that row complete. This remains an estimate of recent
throughput, not a promise about unseen rows. Whole-run ETA is deliberately omitted
with an explicit reason because later stages have different access costs. The
sample history is bounded (at most 13 samples); JSONL and audit semantics are
unchanged.

A declared partial store can exit 0 **within retained-reference scope**. P1 absent
inputs and P2 absent mint rows are counted separately, never findings. P3/P4/P5
history/amount limitations remain: `coverage_complete:false` and
`amounts_complete:false` forbid interpreting observed amounts as exact totals.
An unseeded store also reports incomplete coverage, but receives no P1/P2 absence
permission unless it has `META_PARTIAL_FROM`. Optional names/EIP-4 fields, absent
registers in valid JSON (including `null`), and unknown mainnet identity are not
findings. `coverage_complete` describes history provenance, not an audit of
unwitnessed corruption.

## Matrix coverage

IDs refer to [the M2 required-reference matrix](superpowers/2026-09-12-m2-required-reference-matrix.md).

| R IDs | Bounded retained-state check |
|---|---|
| R1 | Every height in `[partial_from or 1, indexed_tip]` has a header. |
| R2 | Every reverse-header row resolves a header with the matching ID. |
| R3 | Every TX_BY_GIDX and TREE_TXS reference resolves TXS (through the global index for tree memberships). |
| R4 | Every global allocated tx slot and every header's `first_tx_gidx + tx_count` interval resolves; surviving rows agree on gidx, height and block index. |
| R5 | Every retained transaction's output range resolves BOX_BY_GIDX then BOXES, with matching creator transaction and gidx. |
| R6 | BOX_BY_GIDX and all tree/token/template/register/rent box memberships resolve local boxes. Tree transaction lookup is covered by R3. |
| R7, R22 | Every retained box resolves ERGO_TREES. Covers read expansion and apply/rollback template lookup witnesses. |
| R8, R21 | Every retained transaction input resolves BOXES unless explicitly partial (P1 counted). Future block inputs are not in the store and cannot be certified. |
| R9 | Every box's registers_json parses as JSON. |
| R10 | Both token listing indexes resolve TOKENS, even when partial. |
| R11, R27 | Every box/balance asset resolves TOKENS unless explicitly partial (P2 counted). Future counter-update references are not reachable until the block exists. Optional token fields are not inspected for presence. |
| R12 | Every retained tree resolves TREE_BALANCE, including zero balances. |
| R13 | **Not independently reachable:** canonical tree reload occurs in the same immutable Reader snapshot as successful lookup. No intervening deletion is possible; R7/R12/R14/R15/R16 cover retained tree promises. |
| R14 | Every rich entry resolves its tree and balance; indexed amount agrees. |
| R15 | Every token holder entry resolves its tree and TOKEN_HOLDER_AMT; amount agrees. |
| R16 | Every template resolves its example tree. |
| R17 | Every maturity entry resolves a local box that is still live. |
| R18 | On recognized full mainnet genesis, emission metadata matches the genesis emission box, balance exists, and remaining reserve does not exceed genesis total. Recognition uses the same three identities/values/zero creator/height as Reader; O5 means missing identities cannot distinguish damage from another network, so failed recognition is reported, not called clean mainnet supply. |
| R19 | All retained anchor heights, box gidx/primary identity, historical tree membership/tree identity, creator transactions, and known-tree tip balances. Known zero-creator genesis exception follows history on recognized mainnet; history is unavailable otherwise. No per-address historical sums are recomputed. |
| R20 | Indexed tip resolves a header. |
| R23 | Every retained UNDO row resolves its header, created boxes and tx IDs. |
| R24 | Every retained UNDO spent box resolves BOXES. Same-block creations still exist in a quiesced snapshot; rollback's intentional earlier un-create has not happened. No missing pre-seed inputs were journaled. |
| R25 | Every retained UNDO new/previous token reference resolves TOKENS. |
| R26 | Every retained UNDO touched balance resolves TREE_BALANCE, including previous-None entries that created a current balance. |
| R28 | Indexed tip requires both allocation counters; genesis-only seeding requires the box counter. |

R1–R12 and R14–R28 thus have retained-state witnesses checked; R13 is not an
independent stored-state invariant. Future transitions and journals already
pruned are not reachable. Missing memberships with no surviving witness, deleted
tip/genesis provenance, optional enrichment, full state recomputation, and UNDO
restoration correctness are outside this sweep. It does not replay or roll back.
These limits appear in JSON coverage as well as this document.

## Read-only proof and memory

Both offline examples use `examples/support/read_only.rs`. It calls `File::open`
(O_RDONLY), then `try_lock` (exclusive, nonblocking). The descriptor remains owned
by the backend throughout the snapshot. redb's small offset-zero header bookkeeping
is overlaid in memory only (at most 4096 bytes). All other writes and every resize
return PermissionDenied, sync is a no-op, and the repair callback aborts. The sweep
itself only calls `begin_read`; no Store writable opener is used in scan code.

Tests exercise a real separate process holding a Store lock, direct write failure
on the actual O_RDONLY descriptor, backend page-write/resize rejection, attempted
redb write-transaction commit failure, absent-file noncreation, and byte-for-byte
source equality after both attempted writes and repeated scans. File permissions
alone are not used as proof.

Memory is independent of store size and finding count: 8 MiB redb cache, a 4 KiB
header overlay, fixed 28-element check/finding arrays and two permitted-gap
counters, redb traversal state, and the current row/decoded vectors/JSON plus a
bounded number of point-lookups. The bound is **O(cache + largest row + tree
height)**, not a claim of an 8 MiB total RSS limit. UNDO and large token vectors are
decoded one row at a time; no set of findings, visited IDs, addresses, or journals
accumulates. No scratch files or external dependencies are needed.

Tests use isolated fixture stores; existing tests are unchanged in meaning. The
example has `test = true`, so workspace tests include its fixtures and shared
backend tests. Gate evidence is recorded after validation.

## Validation evidence

`CARGO_TARGET_DIR=$PWD/target ./scripts/check.sh all`: **exit 1**.
[Gate results](../artifacts/check/all-MY1prkEh/results.log) and
[Rust log](../artifacts/check/all-MY1prkEh/rust-tests.log).

- PASS: formatting, workspace/all-target Clippy, both Python suites, frontend unit
  tests/type checks/lint/build, and all non-socket workspace tests.
- Sweep: 10 passed (including the subprocess helper); rent-candidates: 6 passed.
- `xp-source` fallback (6) and rust_node socket cases (13): **not run: sandbox**,
  socket bind returns PermissionDenied/EPERM. Its non-socket case passed.
- Playwright: **not run: sandbox**, server bind to 127.0.0.1:18099 returns EPERM.
- Existing ignored tests remain ignored. No npm ci, production-store access,
  schema/encoding/dependency change, commit, or push. Targets and temporary files
  were under the repository, not /tmp.

[Source hashes](../artifacts/check/all-MY1prkEh/integrity-sweep-source.sha256)
include the new untracked files that `git diff` in the gate manifest does not embed.
