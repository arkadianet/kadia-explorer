# Offline rent candidate scan (phase 1)

Run the first-class `xp-store` example on a **quiesced store or owner-prepared
snapshot**, with explicit source and existing scratch directory paths:

```sh
cargo run --release -p xp-store --example rent-candidates -- /path/to/snapshot.redb /path/to/scratch > candidates.jsonl
```

Do not point this at the live indexer's store, copy that store while running, or
redirect stdout onto the source. The example isolates this offline operation from
explorer's writable startup, configuration, ingestion and HTTP server. It does not
fetch network data, classify claims, write a reporting database or change schema.
No external sorting program or new external dependency is needed.

The source descriptor uses `File::open` (OS read-only), with an exclusive,
nonblocking file lock. redb 2.6 has no native read-only database opener and writes
bookkeeping on open: a private backend absorbs only header writes of at most
4 KiB in memory. It rejects other writes and all resizing; its sync is a no-op.
Repair is aborted. No source write syscall exists in this backend. The scanner
never calls `Store::open`. A fixture test deliberately attempts a write through
this opener and verifies byte identity after failure and after scanning.

One snapshot anchors the entire BOXES pass. Selection uses declared
`BoxRow.creation_height` and `spent` height, with checked subtraction and the
existing `xp_types::rent::RENT_PERIOD`. Every qualifying spend is emitted,
including multiple inputs of the same transaction. No profitability filter is
applied. Missing schema/provenance/tip headers and spends above tip are errors.
Completeness assumes a trusted, intact index; this is not a database integrity
or chain continuity audit.

Stdout is deterministic JSONL, in this order:

- `start`: indexed tip height/id, partial-from metadata, scan height range,
  maturity period; coverage is false until completion.
- `candidate_spend`: box ID, spending tx ID, spend height, ascending BOXES key
  order. These records retain input audit evidence.
- `candidate_transaction`: unique `(spending_tx_id, spend_height)` pairs,
  ascending height then tx ID.
- `summary`: `rows_scanned`, `row_bytes_scanned` (encoded BOXES key/value bytes,
  not physical I/O), `candidate_spends`, `distinct_spending_transactions`,
  **`distinct_heights`**, candidate height min/max, scan height range, indexed
  tip height/id, `partial_from`, `scan_complete`, and `coverage_complete`.

Empty ranges/tips are null and counts zero. A partial or unseeded store emits a
stderr warning and **always** has `coverage_complete:false`, even after a finished
scan. An empty, unseeded store therefore makes no historical coverage claim.
The scan range runs from partial-from (or genesis height 0) through the indexed tip.
Candidate range describes the actual observed candidate heights.

Stderr reports stage, rows, encoded row bytes, candidate spends and elapsed seconds
at startup, stage transitions, completion, and every five seconds while processing
rows or sorting. Timings stay off stdout so repeated runs compare byte-for-byte.
There is no real-store measurement or phase-2 cost estimate yet.

Candidate memory does not grow with candidate count: sorting holds at most 4,096
36-byte keys (144 KiB), an 8 KiB input buffer and two merge keys. The source cache
is configured to 8 MiB and the header overlay is at most 4 KiB. During the scan,
one encoded/decoded box is live. **Total memory additionally includes redb's
allocator metadata (scales with source page capacity), traversal pages and the
largest row; 8 MiB is a cache limit, not a process RSS limit.** No candidate sets
or run lists are retained in RAM. The source is released before sorting.

Temporary files hold fixed 36-byte tx/height records. Two-way external merges
bound memory and file handles; peak scratch space is at most three times the
36 bytes per candidate (plus filesystem overhead). Temporary files are anonymous
and closed on exit, including interrupted runs. This is scratch sorting, not a
persistent reporting file. Time is one BOXES pass plus two external sorts of
candidates; no runtime claim has been measured on a real store.

Restart from the beginning using a fresh/truncated stdout destination. Never
append a retry to an interrupted stream. Only a final summary and successful
process exit certify a completed scan; prior records are provisional. There are
no persisted checkpoints to reconcile or source mutations to undo.

`cargo test -p xp-store --example rent-candidates` runs fixture boundary,
declared-versus-inclusion, empty/partial, source-write rejection, deterministic
restart, broken-output and multi-run merge tests. The example has `test = true`,
so the workspace gate also runs them.
