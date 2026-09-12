# Rent history Phase 2a

`scripts/rent_history.py` is a standalone Python 3.11+ standard-library classifier,
node evidence fetcher and exact-set reconciler. It never opens an explorer store.
No reporting redb, API route, dependency, schema or forward-ingest change is included.

## Run and resume

```sh
python3 scripts/rent_history.py classify \
  --config /path/to/active-explorer.toml \
  --candidates /path/to/phase1-candidates.jsonl \
  --rules /path/to/audited-rules.json \
  --work /path/to/rent-phase2a-work \
  --from-height 1846000 --to-height 1864008 \
  --concurrency 12 > rent-census-window.jsonl

python3 scripts/rent_history.py reconcile \
  --candidates /path/to/phase1-candidates.jsonl \
  --classification rent-census-window.jsonl \
  --census /path/to/independent-census.jsonl > reconciliation.json
```

Omit height bounds for all Phase 1 heights. Selection comes solely from the
completed Phase 1 file's candidate spends/transactions; summary cardinalities and
coverage must agree. The measured input is 945,288 spends, 132,341 transactions,
86,637 heights. This implementation retains that candidate set in memory.

The configured `source.url` supplies canonical `chainSlice` headers and full block
bodies. `/blocks/at` does not establish canonicality. The Phase 1 tip anchor is
checked before and after work; every cached height is checked on resume.
External inputs are resolved through the configured node's confirmed
`/blockchain/box/byId/{id}` endpoint (including spent boxes), with spending txid,
inclusion height and serialized box ID checks. Same-block inputs resolve from
preceding outputs. No UTXO-only, mempool or explorer-store fallback is used.
A node lacking the confirmed box endpoint produces missing evidence, not guessed
claims. Missing box evidence is retried on the next invocation.

The owner's 94 blocks/s measurement covers block fetching, **not** these additional
box lookups. This runner has not been benchmarked. Twelve bounded block workers
also resolve their external inputs; node load and total runtime must be measured.
Each HTTP response is capped at 16 MiB, with a 30-second timeout. Family analysis
above 10,000 edges or 16 MiB combined block/box JSON marks family totals unresolved.
Those are per-work-item limits, not a process RSS limit. Evidence/cache disk use
is additional and has not been measured.

Progress goes to stderr at each batch and every five seconds while waiting.
Failures exit nonzero; no successful summary is emitted for an interrupted run.
Restart with the same arguments and work directory, redirecting stdout to a
**fresh file**, never appending. Completed per-height evidence/results are atomic
JSON checkpoints. Every selected height, including cached results, is emitted
again, yielding a complete JSONL stream. The work manifest binds code digest,
node URL, candidate set/snapshot and rules; changes require a new directory.
Reorg mismatches fail closed and also require fresh work. This is evidence
checkpointing, not reporting database storage.

JSONL has one `block` record per selected height, containing transaction audit
records and separate deduplicated families, followed by a completion `summary`.
All transactions in a fetched block pass through the classifier, including
non-candidates and potential children. NanoERG fields are decimal strings or null.
A block's evidence digest covers its raw JSON block plus resolved input objects;
raw evidence is retained next to its checkpoint. Source assurance is explicitly
`trusted_node_response`: body/proof commitments are **not independently validated**.

## Rules and predicate

Supply an independently audited historical manifest. An empty `{"rules":[]}` is
valid for audit-only operation and deliberately cannot certify positive branches.
There is no built-in assertion that today's fee factor applies historically.
Each non-overlapping rule interval has this shape (placeholders are not evidence):

```json
{
  "rules": [{
    "from_height": 1846000,
    "to_height": 1864008,
    "period": 1051200,
    "storage_fee_factor": 1250000,
    "arithmetic": "wrapping_i32",
    "pointer_types": ["Short"],
    "validator_revision": "REPLACE_WITH_AUDITED_REVISION",
    "parameter_source": "REPLACE_WITH_HISTORICAL_PARAMETER_EVIDENCE"
  }]
}
```

The caller must substantiate the interval, revision and parameter source. The
implementation checks the manifest structure, not the truth of those assertions.
`["Short", "Int"]` is a separately pinned rule, never an automatic union. The
inspected local Scala interpreter takes Short; the Rust implementation differs.
Neither local checkout alone supplies an audited historical schedule.

An input qualifies only if its authenticated box is at least 1,051,200 blocks
old, its spending proof is explicitly empty, variable 127 decodes under the
interval's accepted type to an existing output, and the pinned branch accepts:

- Charge is serialized box length × factor with signed i32 wrapping.
- If input value minus charge is positive, the pointed output must have the
  inclusion height, sufficient value, identical tree, ordered tokens and registers.
- Otherwise it is fully consumed. The pointer is still required, but is not a
  refund edge and need not preserve owner properties.

One qualifying input makes a claim; signed or unresolved additional inputs do not
erase it. Young inputs are excluded first. Explicit nonempty proofs and absent
selectors exclude the rent path even when the input box cannot be fetched; census
accounting remains unavailable in that case. Missing proof/box/history or unsupported
selector decoding is unresolved. Out-of-range pointers select ordinary fallback.
A pinned branch returning false on a purportedly confirmed input is an integrity
error, not a silently accepted claim. Positive consensus certification still needs
real accepted-branch fixtures/reference execution at the supplied historical rules.

## Amounts and families

Census delta is all input values minus each output returned to any input tree,
counted once. Verified-owner deltas separately subtract every output on each
verified owner tree once, including shared pointers and voluntary returns.
This version certifies transaction gross rent only when all spending inputs are
verified rent and owner deltas are nonnegative with no fee-owner collision. Mixed
funding and owner top-ups retain signed deltas but return null certified rent;
this is conservative and may leave independently separable flows unresolved.

Same-block output dependencies form undirected connected components from claims.
Edges from outputs returned to parent input-owner trees and from the **full**
standard fee tree are excluded. Descendants, additional parents and grandchildren
are included; later-block spends cannot be CPFP. Missing evidence or graph limits
make family analysis incomplete. No tree, including `0008d3`, identifies an actor.
Terminal P2PK keys/values are observations; other payout trees remain unknown.

Unique component transactions contribute their explicit fee outputs once to the
family record. Fee-collection spends neither add fees nor connect components.
Per-claim fee/net allocation is exact only for a complete, solely rent-funded,
one-claim family with exact owner flows and nonnegative net. Mixed external funding,
multiple claims, incomplete evidence and ambiguous owner flows yield null fee/net
allocations with reasons. Child boundary refunds to owner trees reduce net once.
Do not sum a family's fee separately for each member claim.

A claim with a parent fee or complete fee-paying component is `bot_fee_paying`.
A complete fee-free component is `miner_self_claim`, explicitly based on the owner's
structural rule. Incomplete family analysis leaves a no-fee parent `fee_free_parent`.
No miner/operator identity or CPFP intent is asserted.

## Validation and reconciliation

`PYTHONDONTWRITEBYTECODE=1 python3 scripts/test-rent-history.py` is included in
`./scripts/check.sh all` and its Rust subset. It runs the full classifier on existing
raw chain blocks 1,866,001 and 1,866,002, using earlier block outputs as authenticated
input evidence. Both have pinned emission and fee-collection transaction IDs; the
test requires empty proofs, matching control trees, ages 1–2, explicit age exclusion,
zero claims and zero unresolved classification predicates across the whole blocks.
Some non-control input boxes are absent, so their census amounts are unavailable;
nonempty proofs/absent selectors still establish rent-path exclusion. These blocks
are the available recent sample, not the owner's unavailable ten-block sample.

Synthetic tests exercise maturity boundaries, pointers, proof absence, both branches,
wrapping arithmetic/top-ups, shared pointers, CPFP fees, mixed/multiple parents,
generic absorbers, limits, resume, timeouts, reorg anchors and equal-count/different-ID
reconciliation. Their fixed arithmetic expectations test algorithms; they do not
certify historical consensus. Chain box IDs and negative expectations come from
existing raw node fixtures, never classifier-produced positive labels.

Independent census JSONL entries require `txid` and `height`; `rent_nano` enables
amount reconciliation. All entries must be in 1846000..1864008. Missing amounts
prevent acceptance. The report retains exact matched/only-ours/only-census ID lists
for both age candidates and verified claims, amount disagreements, and a sample
of input reasons with height. Missing classification evidence is reported as such,
not invented as a concrete chain explanation. Acceptance requires a completed stream
covering every Phase 1 candidate height in the census window, 5,116 independent IDs,
both exact set matches and amount agreement. Counts alone never pass.

The independent export, actual Phase 1 JSONL and audited historical rule manifest
were not present during implementation. Live sockets were denied by the sandbox.
Therefore the census remains **unreconciled**, and live positive-branch validation,
full backfill, throughput and the owner's ten-block control remain unrun.
