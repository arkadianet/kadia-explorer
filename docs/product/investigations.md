# Explicit transaction and box investigations

`/investigate` follows indexed box membership across multiple transactions. It does
not infer ownership, counterparties, payment purpose, or allocation of particular
input value to particular outputs. Spending references and read-only data inputs
are distinct connection types.

Entry links use `?kind=tx&id=<64hex>` or `?kind=box&id=<64hex>`. Loading the page or
opening a shared plan performs no investigation request. The user explicitly loads
the selected node; each expansion fetches at most one referenced entity using the
existing transaction or box detail API. Selecting an already loaded node makes no
request. There is no recursive fetching or automatic refresh.

## Evidence and changing chain views

Every transaction retains its returned inclusion block/height, indexed tip when
provided, exact raw amounts, input coverage and local read time. A box detail read
has no supplied indexed-tip anchor; its creation height is never labelled as its
inclusion height. Separate reads are not represented as one atomic or common-branch
snapshot. Matching known relationships can detect contradictions but cannot prove
all loaded nodes remain canonical.

Linked transactions must contain the referenced box in the indicated input/output
position. Resolved boxes must agree on immutable identifiers, values, tokens,
creation height and script hash. Spending links also check the box's indexed
spender and spend height. Transaction inclusion changes, immutable disagreements,
changed box spend references, or disappearance of formerly loaded evidence clear
the loaded graph and require restarting without old pins. The UI does not call such
an observation a proven network reorganization. A transient refresh failure keeps
the older node visibly stale and blocks expansion from it until refreshed.

Missing input boxes retain their IDs and unknown coverage. They never imply zero
value. Initial missing or budget-limited detail reads leave the entity link available.

## Bounded sharing and export

An investigation admits at most 24 nodes, 48 edges, 1,000 references per transaction,
1 MiB of raw evidence per node, and 4 MiB of raw evidence overall. Only one request
is in flight at a time. Reset and navigation invalidate late responses. Reference
lists expand in explicit batches; box token previews show 20 raw quantities while
the full admitted data remains available in the JSON export.

Share URLs encode a versioned plan of entity IDs, typed edges and any observed
transaction inclusion pins, capped at 8,192 encoded characters. They contain no
fetched values and trigger no fan-out. Planned edges are visibly unverified until
loaded evidence supports them. A pinned transaction must still match its returned
inclusion before it can be used. A box ID pins immutable identity only, not spend
state or a historical snapshot.

The downloadable `kadia.investigation` version 1 JSON includes exact DTO quantities,
per-node load/error/stale states, read times, inclusion pins, typed connections and
whether each edge is observed in loaded evidence. Its declared scope is
`independent_indexed_reads`; its note carries the ownership/allocation and canonical
branch limitations. Formatted exports are capped at 8 MiB. Conflicting graphs cannot
be exported as loaded evidence. Share plans and exports are generated locally.

## Interaction and appearance

The scrollable graph uses ordinary focusable node buttons; the Evidence list offers
the same selection and inspector controls without requiring spatial navigation.
Connection evidence has a textual list and full IDs. Original is a compact graph
and inspector; Prism uses an integrated spatial workspace; Atelier places the
evidence inspector beside a ledger-like canvas; Aurora uses a full-width graph over
a spacious evidence section. All four preserve mobile scrolling, readable data,
visible focus, light/dark variables and reduced-motion preferences (no animation).
