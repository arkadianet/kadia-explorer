# Register growth circuit breaker

Inspected the locally installed, Cargo.lock-pinned **redb 2.6.3** source:

| Statistic/API | Work | Category and units |
| --- | --- | --- |
| `ReadableTableMetadata::len()` / `is_empty()` | Root metadata; no row/page traversal once table is open | Logical entry count (`u64`), not bytes |
| `Table::stats()` / read-only table `stats()` | Recursively visits B-tree pages | `stored_bytes`: logical key + value bytes; `metadata_bytes`: branch keys and structural bytes; `fragmented_bytes`: unused bytes inside those pages; page counts and tree height are counts |
| `WriteTransaction::stats()` | Traverses user/system/freed trees and queries allocator accounting; not cheap | User payload bytes, internal metadata bytes, fragmentation including free pages; `allocated_pages` is redb pages, `page_size` bytes/page |
| Iterating tables to sum keys/values (including META and UNDO) | Full logical row scan | Logical serialized bytes; does not measure filesystem allocation |
| Filesystem metadata (`stat`) | Metadata operation | Apparent file length in bytes; on Linux `st_blocks * 512` gives filesystem-allocated bytes |

Source trail (relative to the redb package): `src/table.rs` implements `len` by
calling the tree; `src/tree_store/btree.rs` `Btree::len` returns `root.length`
(or zero). `BtreeMut::len` uses a read view of the transaction's current root.
`stats_helper` recursively visits all children. `src/transactions.rs`
`WriteTransaction::stats` aggregates user/system/freed trees, and includes
`count_free_pages() * page_size` in fragmentation. Stats getters themselves are
constant-time **after** the expensive stats object is built. Public allocation
statistics are bundled with that expensive call. Do not call it per block.
redb's page accounting is not filesystem allocation, and its fragmentation is
not a promise that compaction can reclaim that many filesystem bytes. No
allocator/file reconciliation or footprint measurements were performed here.

The breaker uses `REGISTER_IDX.len()` inside the batch's exclusive write
transaction plus a checked sum of the exact R4–R9 additions across every output
in the batch. It uses the same register-presence rules as Extras. Every valid
output gets a fresh gidx, so repeated values remain separate entries. There is
no historical scan, initialization counter, new metadata row, encoding change,
or schema bump. redb maintains length on insert/remove; rollback and process
restart therefore need no auxiliary count repair. Genesis uses the same check.

When a ceiling is configured, the total must be **strictly below** it. Equality or excess returns a
typed `StoreError::RegisterCapacity`; overflow returns `RegisterCountOverflow`.
Neither commits the transaction. redb's `WriteTransaction::drop` calls
`abort_inner` unless completed. Ingest's existing store-error path publishes a
local halt and returns the typed failure without advancing indexed height or
classifying it as a source failure. Restart after raising the setting resumes
from the last good tip. Startup refuses a setting below existing occupancy and
reports the minimum setting; equality is allowed at startup but new apply is
blocked until the cap is raised above occupancy plus the next batch.

The shipping default is **unset: enforcement is off and register growth is
unbounded**. Startup logs one warning saying no register ceiling is configured
and growth is unbounded, including current occupancy. An omitted setting never
rejects startup or a batch on capacity grounds. Opt in with the top-level TOML
`register_index_ceiling` integer after inventory supplies a measured number.
A set ceiling is enforced, including refusing to start if occupancy already
exceeds it. Startup logs occupancy and the configured ceiling. No existing
entries are skipped, pruned or reclaimed.

`GET /v1/register-capacity` exposes committed entries and configured ceiling as
decimal strings (preserving all u64 values), with `ceiling: null` when unset,
through the existing bounded blocking read admission. `scripts/check-register-capacity.py URL` is an external sampler
using only Python's standard library: it alerts at `entries * 5 >= ceiling * 4`,
returns 1 for an alert or unbounded growth, 2 for a missed/invalid sample, and emits
timestamped JSON. Unset yields `unbounded: true` and `alert_80_percent: null`
because no percentage threshold exists.
This endpoint and sampler cover register capacity only; they add no general metrics
surface, request counters, histograms, or ingest instrumentation. M3 remains deferred.
The operator's collector must schedule it and retain/route its output. Offline
arithmetic tests cover below/at/above 80% and u64 extremes. Live collector routing,
retention and alert receipt have not been verified; no external collector
configuration was available in this workspace.

Atomicity tests reuse `tests/support/logical.rs::snapshot` and compare the raw
maps for **every table, including UNDO**, before/after rejected applies. The
multi-block rejection starts with nonempty UNDO. They also cover genesis,
repeated register values, rollback, reopen, bad startup ceiling, and resume with
a raised setting. Overflow is exercised at the checked arithmetic boundary;
materializing u64::MAX rows is neither required nor practical.

Validation after the default correction (2026-09-12):
`CARGO_TARGET_DIR="$PWD/target" ./scripts/check.sh all` evidence is in
`artifacts/check/all-GbnUbBvy`. Collector tests, fmt, clippy, and frontend passed.
All executed Rust tests passed except the socket-dependent xp-source targets
(`fallback`, `rust_node`), whose socket setup was **not run: sandbox**.
Playwright was **not run: sandbox** (web-server socket binding denied).
The gate exits 1 and remains NOT VERIFIED here; the reviewer runs the full gate
outside the sandbox. No /tmp target or npm ci was used. New coverage includes
unset occupancy through u64::MAX, reopening and accepting a previously rejected
batch without a ceiling, null/configured endpoint ceilings, and the unbounded
sampler state. Configured boundary, atomicity, and overflow coverage is retained.
Live collector alert receipt, blocked inventory, and backup/restore remain unverified.
