# Explorer delivery status

Updated 2026-09-12. This is the delivery-status authority referenced by the
[September 10 history design](docs/superpowers/specs/2026-09-10-history-and-rent-reporting-design.md).
Historical design text is unchanged. Current scope is the accepted
[completion design](docs/superpowers/specs/2026-09-12-explorer-done-design.md) and
[M1–M6 plan](docs/superpowers/plans/2026-09-12-explorer-done-plan.md), subject to the
owner's scoped implementation requests below. This session implements **M3 steps 3–4 only**.
M2 is committed (`b2a83c0`, `cfb489e`, `43f5941`); the reviewer reports all five
gates passed outside the sandbox in 118 seconds. M3 steps 1, 2, 5 and 6 remain deferred.

States: `planned` = agreed work without implementation; `implemented` = code or
procedure exists; `verified` = specified acceptance evidence exists for the named
revision; `released` = deployment plus post-deployment smoke is evidenced.
An implemented item is not necessarily verified. Missing evidence never means pass.
CUT is a scope decision, not a delivery state. No release is certified here.

Working revision: `43f594188b45e139005c89d0c2507edb57c90337` plus the uncommitted
M3 steps 3–4 diff, branch `fix/explorer-exit-code-and-rent-truth`. No commit, push or production operation was
performed in this implementation session. Earlier baseline evidence below retains
its original provenance.

## Closed baseline correctness work

| Scope | State | Commit provenance | Current evidence |
|---|---|---|---|
| Unresolvable fork exits 3; report actual fork depth | implemented | `66eca9a745d3272b100b2982103eca0abd2542cc` | Reviewer G baseline below |
| Truncated address rent is a partial sample, not globally earliest maturity | implemented | `7c27018dee8e71a5957b46eee865738ae955c916` | Reviewer G baseline below |
| Every source operation contributes to truthful source health | implemented | `8a0e3dc76b7de4b5564d64f69700fc25682af65e` | Reviewer G baseline below |
| Runtime server failure exits nonzero, preserving ingest exit precedence | implemented | `8a0e3dc76b7de4b5564d64f69700fc25682af65e` | Reviewer G baseline below |

Height-based historical reads are implemented at baseline (including `fbf5ee0`);
the old design's “awaiting Rust execution gates” is historical, not current status.
The reviewer baseline includes those tests. Independent live verification remains
separate and unverified.

## Milestone progress

| Milestone | State | Evidence |
|---|---|---|
| M1 — make gates unavoidable (steps 1-3) | implemented | `scripts/check.sh all` exit 0, reviewer-run outside any sandbox on Rust 1.96.0 / Node v22.22.2 |
| M1 — steps 4-5 (enforcement, provisioning) | planned | Blocked on owner: needs a push and repo-admin rights. A committed workflow is not a gate until it runs and is required. |
| M2 steps 1-2 — fail closed on canonical selection | implemented | `scripts/check.sh all` exit 0; 12 new/adjusted source and ingest tests pass, reviewer-run |
| M2 steps 3-4 — required-reference matrix and fail-closed reads | implemented | Committed at `cfb489e`; [matrix and fixture evidence](docs/superpowers/2026-09-12-m2-required-reference-matrix.md). Final G: `artifacts/check/all-q4clyPVj`, exit 1 only for sandbox socket refusals (`fallback`, `rust_node`, Playwright); other gates pass. |
| M2 steps 5-7 — generated transition model, UNDO | implemented; deterministic tests verified | 256 × 32 histories, independent all-table oracle, retention boundaries and deliberate mutation pass. Current G evidence and sandbox limitations below. |
| M3 steps 3–4 — summary routes and legacy budgets | implemented; deterministic tests verified | Final G exit 1 for sandbox socket refusals only; evidence below. |
| M3 steps 1, 2, 5, 6; M4–M6 | planned | Deferred; frontend, telemetry and production measurements not started. |

### M2 steps 5–7 — independent transition and UNDO evidence

The model in `crates/xp-store/tests/support/model.rs` uses maps of fixture boxes,
transactions, token quantities and activity membership. It reconstructs secondary
indexes and counters from these maps. Rollback reconstructs the surviving fixture
chain from its starting state, intersecting UNDO keys with independently tracked
retention; it never consumes the store's undo values. Only the test driver calls
`Store::apply_batch` / `rollback_to`. Expectations use a separate schema-v2 byte
serializer, not production row encoders or index-key helpers. Production row structs
are data containers only. Schema version, core encoding, wire contracts and
production paths are unchanged; no dependencies or test hooks were added.

All 26 tables are compared after each command, including UNDO. The sole normalized
field is UNDO's `prev_balances` entry order, which production emits from a randomized
HashMap. Every entry and value remains significant; malformed/trailing bytes are
rejected before normalization. Existing rollback fingerprint assertions remain and
now also compare all logical tables. No physical redb file equality is asserted.

Coverage: seeds 0–255, 32 commands each (8,192 total), split evenly between synthetic
genesis and partial height-2000 starts. Each history includes 12 single applies,
8 two-block batches, 11 rollbacks and one close/reopen. Two transactions per block
exercise same-block spending; amounts, addresses, mint/full-or-partial burn choices,
rollback depths and branch IDs vary by seed. Each history asserts mint, burn,
same-block spend and reused-gidx coverage. Reapplied branches have new IDs.
Fixtures keep one live box and at most one live token species, with three addresses,
a fallback script template and an R9 register. Existing real-block fixtures retain
coverage of richer scripts, EIP-4 and multi-box transactions.

Runtime was reduced before reducing any requested coverage: four reopens became
one per history, batches became two blocks, and blocks became two transactions.
Four bounded worker threads run isolated histories. The earlier sequential candidate
was stopped after exceeding 60 seconds; the optimized candidate's standalone run
was 39.75 seconds, and the final focused run was **34.64 seconds** for all three new
tests (256 histories plus mutation/minimizer and retention). The full-gate run of
the same suite took **32.34 seconds**. Retention alone took 3.285 seconds in the
focused run. Temporary stores and Cargo target stay under the
repository; no target directory was placed under `/tmp`.

Retention cases apply 999, 1,000 and 1,001 blocks, plus 1,002 to force actual eviction.
The model expects up to W+1 retained rows, while the public rollback depth limit is W.
Tests check interval and independently serialized values, rollback at the depth
limit, just outside that limit, consumption of the extra retained row, and rejection
at a genuinely missing/pruned row. Rejected operations preserve fingerprints and
all raw logical bytes. Reapplication after pruning restores the indexed fingerprint
and the independently expected retained undo history.

Mutation proof: close Store, open the isolated database with redb, flip one byte in
a stored undo register-gidx, close it and reopen Store. The existing fingerprint
stays equal; the new oracle reports `table=undo`. Delta debugging reduces the
32-command injected failure to **one apply**, saves it, reloads it, and proves clean
replay passes while damaged replay fails. The saved proof is
[`seed-7.mutation-proof.txt`](artifacts/state-machine/seed-7.mutation-proof.txt).
Actual property failures save the seed, full command stream and table difference
before minimization, then a same-failure-signature, deletion-1-minimal replay.
Unexpected panics are captured too. Replay ordinary failure artifacts with:

```sh
CARGO_TARGET_DIR="$PWD/target" XP_STATE_REPLAY=artifacts/state-machine/seed-N.min.txt \
  cargo test -p xp-store --test state_machine seeded_histories -- --nocapture
```

The mutation-proof file deliberately passes without fault injection; use
`cargo test -p xp-store --test state_machine undo_mutation_and_minimized_replay -- --nocapture`
to replay its damage. This tests the minimizer and artifact round trip, not just its
existence. No real property failure remained in the final focused run.

Focused command (exit 101 only from denied socket fixtures):

```sh
CARGO_TARGET_DIR="$PWD/target" TMPDIR="$PWD/artifacts/m2-focused/tmp" \
  cargo test -p xp-source -p xp-store -p xp-api --no-fail-fast -- --nocapture
```

[Focused log](artifacts/m2-focused/tests.log): all store tests pass; API routes
66 pass / 1 pre-existing ignored, API unit tests 6 pass / 1 ignored. Source unit tests
and one non-socket canonical-selection test pass; 6 fallback and 13 rust-node HTTP
fixtures cannot bind sockets (`EPERM`): **not run: sandbox**. Live parity remains
ignored, not verified. No production-store integrity claim follows from these tests.

Full gate: `CARGO_TARGET_DIR="$PWD/target" ./scripts/check.sh all`.
[Gate artifacts](artifacts/check/all-JWBUXkHG/results.log), including manifest, tool
versions and per-command logs. **Exit 1, NOT VERIFIED:** only the `xp-source`
`fallback` and `rust_node` test targets and Playwright were blocked by socket
`EPERM` (**not run: sandbox**). Formatting, workspace Clippy, all other Rust tests,
and frontend tests/check/lint/build (including bundle budget) passed. Playwright's
mock server failed to bind `127.0.0.1:18099`; no browser tests ran. The current
candidate still needs G outside the sandbox; the owner's green prior baseline is
not substituted for that evidence.
A supplementary [source manifest](artifacts/m2-focused/source.sha256) includes the
new untracked test files, which `git diff --binary` alone cannot capture. The working
tree remains uncommitted and unpushed. No `npm ci` or production operation was run.

### M2 steps 3-4 — rollout preconditions and known limits (reviewer)

1. **Deploy gate.** This milestone converts latent silent damage into visible
   `500 integrity_error` responses. The production 89 GB store has never had an integrity
   audit (deferred to M6), so a read-only integrity sweep against a copy must run BEFORE this
   reaches production; otherwise pre-existing damage surfaces first as user-facing 500s.
   This mirrors the M2 steps 1-2 precondition, where `chainSlice` support was verified on both
   real sources before the legacy path was removed.
2. **Diagnostic limit.** `StoreError::Corrupt` is `&'static str`, so a fired check names the
   class ("rent entry missing box") but never the instance. An operator gets the code site, not
   the damaged row, and the log line is the only artifact. Widening the variant would touch the
   whole store crate and add allocation on read paths; instance-level identification belongs
   with M4's storage attribution. Recorded so it is not rediscovered during an incident.
3. **Detection is bounded.** Per the matrix: missing secondary memberships with no surviving
   reference cannot be detected by an ordinary read. This work makes corruption detectable
   where a witness exists; it does not certify the store.

Nothing above is `verified` in the release sense: CI has never executed, and live parity,
the restore drill and the capacity soak remain unrun.

## Rent backfill phase-1 pilot — measured 2026-09-12

Run against the local full-history store, quiesced with owner permission (its indexer stopped
with SIGTERM, exited cleanly in 4 s, restarted afterwards and caught back up to tip; ~11 minutes
of downtime). Summary record retained at `docs/operations/2026-09-12-rent-phase1-pilot.json`.

| Measure | Value |
|---|---|
| Rows scanned | 57,406,829 (14.3 GB) |
| Candidate spends | 945,288 |
| Distinct spending transactions | 132,341 |
| **Distinct heights (sizes phase 2)** | **86,637** |
| Candidate height range | 1,051,232 - 1,871,490 |
| Coverage | complete (not a partial store) |
| Scan wall time | 10 m 54 s, no network |

Phase-2 fetch throughput measured separately against the local node: 94 blocks/s over 120
randomly sampled candidate heights spread across the full range, at concurrency 12. An earlier
40-block sample suggested 283 blocks/s and was discarded as cache-warm and adjacent. At the
measured rate, fetching all 86,637 candidate heights is approximately 15 minutes.

This replaces the design's original full-rescan estimate of ~819,000 rent-era blocks at one to
five days. The reduction comes from generating candidates locally: `BoxRow` already stores
`creation_height` and `spent`, so the age predicate needs no block data. Classification work on
top of the fetch is not included in the 15 minutes and remains unmeasured.

## G baseline and reproducible invocation

The owner supplied the reviewer's run **outside the sandbox on this exact baseline
tree**. Accepted evidence, not a new measurement by this agent:

| Command (root unless specified) | Reviewer result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo test --workspace --no-fail-fast` | 31 test targets ok / 0 failures |
| frontend: `npm test` | 120 tests pass |
| frontend: `npm run check` | 0 errors |
| frontend: `npm run lint` | clean |
| frontend: `npm run build` | 58.97 KB / 120 KB budget |
| frontend: `npx playwright test --workers=4` | 76 passed |

The reviewer did not supply immutable artifact URLs, hashes or resolved tool
versions; those are not invented. Baseline Rust selection was `stable`. Locally
observed versions: `rustc 1.96.0 (ac68faa20 2026-05-25)`,
`cargo 1.96.0 (30a34c682 2026-05-25)`, Node `v22.22.2`, npm `10.9.7`,
Playwright `1.63.0`. These are local observations, not retroactive reviewer metadata.

**Reviewer re-verification on the pinned toolchain (2026-09-12).** The baseline above
was measured with `stable` = `rustc 1.95.0`, before this milestone pinned `1.96.0`.
Because the pin changes the compiler that every gate runs on — and clippy runs
`-D warnings`, so a lint change is a behavioural change — the reviewer re-ran the Rust
gates outside the sandbox on `1.96.0` after the pin took effect, on this exact tree:
`cargo fmt --all -- --check` clean, `cargo clippy --workspace --all-targets -- -D warnings`
clean, `cargo test --workspace --no-fail-fast` 31 test targets ok / 0 failures. The pin is
retained on that evidence. Installing `1.96.0` on a machine that lacks it is proven only
by the reviewer's own rustup download; a clean-runner install remains unproven until CI
executes.

Developer provisioning (network and writable tool caches required):

```sh
rustup show active-toolchain
cargo fetch --locked
cd frontend
# Select the version in .node-version with your Node version manager.
npm ci
npx playwright install --with-deps chromium
cd ..
scripts/check.sh
```

`scripts/check.sh` runs exactly G; `rust` and `frontend` select the same commands
for separate CI jobs. Both jobs are required for G. Build includes the existing
budget check. The default Playwright suite uses loopback mocks, not the live config.
Parity remains ignored; there is no `--ignored`, production secret or public Ergo
node in PR jobs. Installation needs package/browser servers; fixture execution does
not need a public service.

Logs, manifest, command exit statuses and SHA-256 hashes are written to unique
`artifacts/check/<suite>-*/` directories. Fixtures use a workspace TMPDIR. The
script does not set CARGO_TARGET_DIR or use /tmp; caller Cargo configuration remains
authoritative. Permission/quota/read-only errors produce a distinct environment
refusal diagnostic and nonzero exit; that heuristic does not exclude simultaneous
code failures. Other failures remain FAIL and require log inspection. A partial
suite or environment refusal never produces a successful G result.

## Local implementation evidence (sandbox)

All runs below are on the baseline SHA plus uncommitted orchestration changes.
They are local, ignored review artifacts, not immutable release artifacts. The
reviewer can read them in this workspace; owner must archive candidate CI evidence.

- [First runner console](artifacts/m1/check-console.log),
  [manifest and hashes](artifacts/check/all-fYmPG0Qj/manifest.log),
  [log checksums](artifacts/check/all-fYmPG0Qj/logs.sha256):
  `RUSTUP_TOOLCHAIN=stable scripts/check.sh`, exit 1. fmt passed;
  clippy/tests could not open the user-configured Cargo target lock on the read-only
  filesystem. Frontend 120 unit tests, check (0 errors/0 warnings), lint and build
  (58.97 KB / 120 KB) passed. Playwright could not start the mock server:
  `listen EPERM 127.0.0.1:18099` — **not run: sandbox**, not a code regression.
  This first script revision labelled read-only errors FAIL; diagnostic coverage was
  corrected before the final invocation.
- [Workspace-target run](artifacts/m1/check-workspace-console.log):
  `RUSTUP_TOOLCHAIN=stable CARGO_TARGET_DIR="$PWD/target" scripts/check.sh`, exit 1.
  fmt/clippy passed. Workspace tests completed with two refusing targets:
  `xp-source --test fallback` (6 fixture setup failures) and
  `xp-source --test rust_node` (10 fixture setup failures); each is
  `PermissionDenied` at loopback listener creation — **not run: sandbox**.
  Remaining targets passed; ignored operational tests stayed excluded.
- [Offline npm ci](artifacts/m1/npm-ci.log),
  [normal npm ci](artifacts/m1/npm-ci-network.log) and
  [DNS diagnostic log](artifacts/m1/npm-logs/2026-09-12T08_17_04_187Z-debug-0.log):
  locked install could not complete. Offline cache lacked `zimmerframe`; normal
  install encountered registry `EAI_AGAIN` and npm reported “Exit handler never
  called”. This is **not run: sandbox provisioning**. npm ci removed the previous
  node_modules before failing; subsequent frontend execution lacked Vitest and
  Playwright. Restore with `cd frontend && npm ci` outside the sandbox before the
  next local run. No package or lockfile was changed to evade that refusal.
  The final runner avoids npx auto-install when the locked Playwright binary is
  absent and labels missing prerequisites separately. Build hashes from the second
  run describe the earlier build, not a successful second build; final runner only
  hashes a build after its frontend gate passes.

- [Final runner console](artifacts/m1/check-final-console.log),
  [final manifest](artifacts/check/all-IquBhwDz/manifest.log),
  [final gate results](artifacts/check/all-IquBhwDz/results.log) and
  [final log hashes](artifacts/check/all-IquBhwDz/logs.sha256):
  `RUSTUP_TOOLCHAIN=stable CARGO_TARGET_DIR="$PWD/target" scripts/check.sh`
  completed with **exit 1** on the final runner/workflow bytes. fmt and clippy
  passed; Rust had 29 ok target summaries and the same two socket-refusing targets.
  Frontend was **not run: missing prerequisites after sandbox-blocked npm ci**;
  Playwright's first invocation above already captured the socket refusal. This is
  not a green candidate certification. The ledger text was finalized afterward;
  its untracked content is not part of the recorded tracked diff.

Final orchestration check: `bash -n scripts/check.sh`, parsed workflow structure
(PR/push/manual triggers, two shared-runner jobs, timeouts, full action SHAs,
always-upload and >=14-day retention), and `git diff --check` passed. No existing
application source/tests, package manifest, lockfile or historical document changed.
New code/config files are unstaged and the accepted planning documents are still
untracked. Local evidence directories are ignored.

## M1 implementation and tool choices

| Item | State | Evidence / blocker |
|---|---|---|
| Step 1: baseline | implemented | Reviewer baseline above; local evidence below; candidate-wide green run still required |
| Step 2: local runner and workflow | implemented | [runner](scripts/check.sh), [workflow](.github/workflows/ci.yml); actual GitHub execution unverified |
| Step 3: status ledger | implemented | This file; historical reference now resolves |
| Step 4: required-check enforcement | planned | Owner-only procedure below; not run, no GitHub MCP/admin configuration performed |
| Step 5: later evidence inputs | planned | Owner scheduling table below; availability and dates unknown |

Rust is pinned to locally exercised `1.96.0` in `rust-toolchain.toml` (rustfmt and
clippy). Its installed local name is `stable`; sandbox runs explicitly set
`RUSTUP_TOOLCHAIN=stable` to use that exact compiler without a forbidden rustup
installation outside the workspace. CI uses the numeric pin without that override.
Fresh numeric-toolchain installation is not proven locally.

Node is pinned to `22.22.2` in `frontend/.node-version`, consumed by setup-node.
The existing mock server runs `.ts` directly and documents Node >=22.18 for native
type stripping. This matches the local runtime without package upgrades. An
`engines` addition was considered; a separate version file avoids unnecessary
package/lock metadata churn. npm's resolved version is logged rather than separately
installed. Both lockfiles remain unchanged.

Action revisions reviewed against their upstream release/commit and action metadata
on 2026-09-12 (bounded provenance/interface review, not a full bundled-code audit):

| Action | Exact revision | Reason |
|---|---|---|
| [checkout v4.2.2](https://github.com/actions/checkout/commit/11bd71901bbe5b1630ceea73d27597364c9af683) | `11bd71901bbe5b1630ceea73d27597364c9af683` | Obtain event revision; credentials not persisted |
| [setup-node v4.4.0](https://github.com/actions/setup-node/commit/49933ea5288caeca8642d1e84afbd3f7d6820020) | `49933ea5288caeca8642d1e84afbd3f7d6820020` | Install exact Node runtime; no dependency cache action |
| [upload-artifact v4.6.2](https://github.com/actions/upload-artifact/commit/ea165f8d65b6e75b540449e92b4886f43607fa02) | `ea165f8d65b6e75b540449e92b4886f43607fa02` | Retain logs and Playwright traces for 14 days, including failure |

These are build orchestration, not application dependencies. Rust uses hosted
rustup and the repository pin, avoiding a fourth action. Ubuntu `24.04` is the
runner OS label (hosted image updates remain visible in GitHub setup logs). Job
limits are Rust 30 minutes and frontend 20 minutes. No path filters or
continue-on-error bypass the gates. Artifact upload runs with `always()`; a hard
runner termination can still prevent upload, so missing artifacts remain a blocker.

## Owner procedure: M1 step 4 — NOT EXECUTED

Prerequisites: reviewer commits the accepted files and M1 diff; owner chooses the
integration branch; GitHub Actions must be enabled, the three pinned actions
allowed, and account policy must permit required checks. The following commands
are **instructions for the owner**, not authorization or actions taken here.
Use a clean checkout of the reviewed candidate. Replace uppercase placeholders.

```sh
REPO=arkadianet/kadia-explorer
INTEGRATION=OWNER_SELECTED_INTEGRATION_BRANCH
CANDIDATE=REVIEWER_COMMITTED_CANDIDATE_SHA
PROBE=m1-enforcement-probe-20260912
EVIDENCE="$PWD/artifacts/owner-m1"
mkdir -p "$EVIDENCE"
git switch -c "$PROBE" "$CANDIDATE"
# Deliberately fail the real fmt gate on this disposable branch only.
printf '\nfn m1_gate_probe( ){ }\n' >> bin/explorer/src/main.rs
git add bin/explorer/src/main.rs
git commit -m 'test(ci): deliberate fmt failure for enforcement proof'
git push -u origin "$PROBE"
gh pr create --repo "$REPO" --base "$INTEGRATION" --head "$PROBE" \
  --title 'M1 enforcement probe — do not merge' \
  --body 'Disposable deliberate failure; retain red/green evidence and close without merging.'
gh run list --repo "$REPO" --branch "$PROBE" --workflow ci.yml
# Select the pull_request run (not an unrelated push run).
RED_RUN=ACTUAL_RED_RUN_ID
gh run watch "$RED_RUN" --repo "$REPO" --exit-status
# Expected nonzero; run the following even after that expected failure.
gh run view "$RED_RUN" --repo "$REPO" --json headSha,event,conclusion,jobs,url > "$EVIDENCE/red.json"
gh run view "$RED_RUN" --repo "$REPO" --log > "$EVIDENCE/red.log"
gh run download "$RED_RUN" --repo "$REPO" --dir "$EVIDENCE/red-artifacts"
```

In repository Settings → Branches → applicable classic branch protection rule,
set the **exact integration branch name**, Require a pull request before merging,
Require status checks to pass before merging, Require branches to be up to date
before merging, and select **Rust gates** and **Frontend gates**, with GitHub
Actions as their expected source. Enable **Do not allow bypassing the above
settings** (including administrators); leave force pushes and deletions disabled.
Preserve any stricter existing policy. Check rule precedence so this is the rule
that actually applies. Equivalent active rulesets must have these same required
checks and no bypass actors. Do not enable merge queue without adding/testing a
`merge_group` workflow trigger in a separately reviewed change.
See [GitHub's protection settings](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/managing-a-branch-protection-rule).

Capture the red PR's blocked-merge UI, check names/source, target branch, settings,
UTC time, actor and rule ID. Do not try to merge the probe. Export read-only policy:

```sh
# URL-encode INTEGRATION if its name contains '/' (for example release%2Fmain).
BRANCH_ENCODED=URL_ENCODED_INTEGRATION_BRANCH
gh api "repos/$REPO/branches/$BRANCH_ENCODED/protection" > "$EVIDENCE/protection.json"
gh api "repos/$REPO/rules/branches/$BRANCH_ENCODED" > "$EVIDENCE/effective-rules.json"
# Revert only the deliberate probe commit; leave the M1 candidate intact.
git revert --no-edit HEAD
git push origin "$PROBE"
gh run list --repo "$REPO" --branch "$PROBE" --workflow ci.yml
GREEN_RUN=ACTUAL_GREEN_PULL_REQUEST_RUN_ID
gh run watch "$GREEN_RUN" --repo "$REPO" --exit-status
gh run view "$GREEN_RUN" --repo "$REPO" --json headSha,event,conclusion,jobs,url > "$EVIDENCE/green.json"
gh run view "$GREEN_RUN" --repo "$REPO" --log > "$EVIDENCE/green.log"
gh run download "$GREEN_RUN" --repo "$REPO" --dir "$EVIDENCE/green-artifacts"
# Confirm the restored tree matches the candidate, then close without merging.
git diff --exit-code "$CANDIDATE" HEAD
gh pr close "$PROBE" --repo "$REPO"
find "$EVIDENCE" -type f ! -name SHA256SUMS -exec sha256sum {} + > "$EVIDENCE/SHA256SUMS"
```

Archive those files outside expiring Actions retention and link immutable URLs here.
Also run both checks on the actual candidate PR, recording its head and tested merge
SHA; the probe's restored tree is not a substitute for candidate-revision evidence.
If settings are unavailable, record the actual permission/policy error, actor and
needed admin/plan change. Until red → green, merge blocking, candidate G and policy
evidence exist, M1 is **not verified** and “unavoidable” enforcement is unproven.

## Owner procedure: M1 step 5 — NOT PROVISIONED

Owner must fill each row with named responsible person, resource identity,
availability confirmation, booking start/end in UTC and an evidence URL. No dates,
access, host capacity or procurement are assumed. Keep credentials out of this file.

| Input and precise action/evidence to supply | Availability / schedule | Blocks |
|---|---|---|
| Name primary mainnet source and independent reference URLs/versions; record `/info` and canonical `chainSlice` capability at a pinned height/hash using the source adapter's request shape; arrange stable reference access or independently captured equivalent coverage. Schedule `scripts/parity.sh "$EXPLORER_URL" "$NODE_URL"` on the final candidate; save exit code and complete JSON/log with anchor, sampled IDs and provenance. Require outcome pass, zero mismatches and design coverage floors. | unknown / not booked | M2 source rollout preflight; M6 independent parity |
| Reserve isolated deployment-class host; record CPU/RAM/OS/filesystem/storage, writable staging capacity and one-owner service configuration. Book nominal 10 req/s for 10 min, mixed concurrency 1/8/32/64 (>=1,000 requests each), then seven-day soak and seven daily growth intervals after M3 instrumentation. | unknown / not booked | M3 latency/drain evidence; M4 headroom; M6 capacity soak |
| Reserve independent backup destination plus separate full-size restore staging; record identities, free/allocated bytes and access confirmation. Book a maintenance window with operator, service stop/closed-owner confirmation, consistent copy/hash/manifest, restart, then isolated restore and catch-up. Approve actual service/path commands in M4's runbook once identities are known. Budget <=30 min capture downtime, <=4 h restore/catch-up, <=24 h backup age; measure, never assume. | unknown / not booked | M4 full-size restore drill; M6 recovery acceptance |
| Assign repository administrator and confirm Actions/required-check permission and allowed pinned actions; schedule the exact step 4 procedure above and capture read-only policy plus blocked PR evidence. | unknown / not booked | M1 enforcement and all candidate required-CI evidence |
| Assign external telemetry owner/destination; configure 15-second collection, >=14-day retention outside explorer, bounded disk/rotation and gap detection. Book fixture saturation + restart, then retrieve prior episode and new counter epoch; retain 80% register-cap alert and backup-age evidence when implemented. | unknown / not booked | M3 durable telemetry; M4 alerting; M6 seven-day continuity |

Once the owner supplies the source identities, capture their capability evidence
with the adapter's exact URL shape (read-only; not executed here):

```sh
PRIMARY_URL=OWNER_PRIMARY_URL
NODE_URL=OWNER_INDEPENDENT_REFERENCE_URL
HEIGHT=OWNER_SELECTED_COMMON_CANONICAL_HEIGHT
mkdir -p artifacts/owner-m1/sources
curl --fail-with-body --max-time 30 "$PRIMARY_URL/info" > artifacts/owner-m1/sources/primary-info.json
curl --fail-with-body --max-time 30 "$NODE_URL/info" > artifacts/owner-m1/sources/reference-info.json
curl --fail-with-body --max-time 30 "$PRIMARY_URL/blocks/chainSlice?fromHeight=$HEIGHT&toHeight=$HEIGHT" > artifacts/owner-m1/sources/primary-chain-slice.json
curl --fail-with-body --max-time 30 "$NODE_URL/blocks/chainSlice?fromHeight=$HEIGHT&toHeight=$HEIGHT" > artifacts/owner-m1/sources/reference-chain-slice.json
```

Inspect the height and canonical ID agreement; HTTP 200 alone is not capability
verification. Record URL identity, versions, time and exit codes without credentials.

Production paths, service identities and capacity are unknown; fabricating executable
stop/copy commands now would be unsafe and unauditable. Step 5 books those inputs;
M4 produces and reviews concrete recovery commands before the drill.

## Later milestone acceptance ledger

| Scope | State | Required artifact / current blocker |
|---|---|---|
| M2 integrity and transition model | implemented; current G not verified in sandbox | Matrix and deterministic store/API cases pass; 256 histories, UNDO retention and mutation evidence below. Source socket suites and Playwright require outside-sandbox G. |
| M3 bounded reads and telemetry | steps 3–4 implemented | Zero-lookup and boundary proofs below; frontend migration, latency/RSS/drain and durable restart evidence remain deferred. |
| M4 capacity and recovery | planned | Table attribution, growth/headroom, atomic cap tests, full-size checksummed restore/catch-up: absent |
| M5 continuation | implemented; sandbox-limited verification | Steps 1–4 committed in 2937523 / b437259; steps 5–6 uncommitted. See M5 entry below; full G remains unverified in sandbox. |
| M6 release verification | planned | Final candidate manifest, required G, independent parity, real-API smoke, seven-day soak, recovery and rollout evidence: absent |
| Parity against live reference node | implemented harness; unverified operational gate | No live run or captured independent equivalent in this session |
| Restore/resync drill | planned; unverified | No completed full-size restore. Production resync prohibited; isolated resync benchmark CUT, not a hidden passing gate |
| Capacity soak | planned; unverified | No seven-day growth/latency/RSS/headroom/telemetry dataset |

Populate artifact URLs, SHA/config/anchor and verdict in the relevant row when each
milestone executes. Absent artifacts intentionally have no invented links.

## CUT decisions and plan corrections

Accepted design §1.1 removes these from this release's obligations:

- `/v1/miners`, `/v1/stats/24h`, `/v1/stats/daily`, `/v1/rent/summary`,
  `/v1/rent/recent`, `/v1/rent/daily`; no replacement aggregate service.
- Reporting database/file, worker, backfill and rent-claims classifier; core input
  IDs cannot independently establish rent execution.
- DuckDB/general analytics, full-chain balance-event index, timestamp selector,
  historical holder rankings, new supply dashboards, fiat/price data, protocol
  expansion, token-name discovery, pool attribution and testnet.
- Mempool/WebSocket remain deferred; confirmed mainnet and polling stay in scope.
- Homepage transaction-kind badges requiring full expansion (removal belongs to M3,
  not yet implemented); detail facts stay.
- Legacy first-ID canonical guessing (removed in M2 steps 1–2);
  body-only fallback stays.
- Guaranteed 30–40 GB physical size, byte-identical redb restoration, restoring
  pruned historical UNDO, and isolated resync benchmarking. Measured attribution,
  retention-aware logical identity and full-size restore/catch-up remain required.

The original M1 estimate conflated code work with owner-controlled enforcement and
procurement. YAML alone cannot make gates unavoidable; steps 4–5 and a green hosted
candidate remain outstanding. “Pin the version that passes” must not turn a local
stable resolution into a claim about unrecorded reviewer versions. Full-size restore
is the recovery obligation; resync is not. No historical planning text was rewritten.


## M3 steps 3–4 — 2026-09-12 candidate

Only the two additive summary routes and the legacy transaction expansion safety
net are implemented in this round. No frontend edits, metrics package, external
dependency additions, schema-version bump, row-encoding change, commit or push.
The budget rationale, published block-size provenance, input-expansion envelope,
limitations and exact accounting are in
[transaction-budgets.md](docs/operations/transaction-budgets.md).
These are engineering judgments, not measured mainnet maxima or ordinary-load
containment. Summary routes perform zero enrichment; frontend adoption is deferred.
The proposed 10,000 box resolutions became 10,000 shared work units; successful JSON
and cumulative encoded-row admission each have a 2 MiB limit; the cooperative
worker deadline is four seconds. Valid large legacy responses may return 422.

Evidence from the final code candidate:

- New global and block summary walks match every fixture summary field and full DTO
  field/count, both directions, exclusive cursors, height/id paths and empty ranges.
  Per-store test counters stay `[0, 0, 0]` for box/tree/token enrichment; full DTO
  expansion increments all three as a positive control. Count 65,535 succeeds;
  65,536 returns `integrity_error` for each newly exposed input count.
- Exact/one-over work, decoded bytes, JSON bytes (including escaping), and deadline
  tests pass. Real token/register DTO expansion is tested at/over the work threshold.
  Oversized input/output transactions reject before enrichment. Oversized stored
  register/tree/token rows reject before owned decoding. A two-transaction block
  whose individual detail routes succeed fails cumulatively as a complete problem,
  never a successful prefix. Legacy fixture JSON matches the original DTO path.
- Stored decoding checks inside vectors and 128-byte string-copy chunks; a test
  interrupts midway through input, token and byte vectors. Register parsing, hex
  conversion and serialization also check cooperatively. Existing cancellation
  tests retain worker-owned permits; the ordinary cancellation test now explicitly
  asserts retention before unblocking. No existing test was weakened.
- Focused API tests: 26 unit tests and 72 route tests pass, one preexisting ignored
  route test. Store cooperative-decoder test passes. Final workspace gate also
  passes the non-socket suites, including the M2 generated state model and existing
  history limits. Live parity remains unrun as before.

Final command (target override is needed because the configured Cargo cache is
read-only here; this uses the existing repository `target`, not `/tmp`):

```sh
CARGO_TARGET_DIR="$PWD/target" ./scripts/check.sh all
```

Final evidence: [results](artifacts/check/all-pr2BIDoc/results.log),
[revision/diff/tool manifest](artifacts/check/all-pr2BIDoc/manifest.log),
[candidate source hashes](artifacts/check/all-pr2BIDoc/candidate-source-sha256.json).
New source/document files are also copied under that artifact's `new-files/`, since
`git diff` alone does not capture untracked files. This ledger update follows the
run; the hashed code and budget document are unchanged from the tested candidate.

| Gate | Result |
|---|---|
| Formatting | PASS |
| Clippy, workspace/all targets with warnings denied | PASS |
| Rust workspace tests | NOT VERIFIED overall: **not run: sandbox** for `xp-source` socket fixtures (`fallback`: 6 bind refusals; `rust_node`: 13 bind refusals). Other suites pass. |
| Frontend tests/check/lint/build | PASS; 120 tests in 16 files. No `npm ci` was run. |
| Playwright | **not run: sandbox** — mock-server listen at `127.0.0.1:18099` rejected with `EPERM`. |
| Overall | Exit **1**, expected environment restriction; not a green G claim. |

The earlier `all-HKuJGS7S` run predates the decoder deadline correction and is
superseded by `all-pr2BIDoc`. No production database or performance/load measurement
was used. M3 steps 1, 2, 5 and 6 remain separate tasks; M3 as a whole is incomplete.

## Rent history Phase 2a — 2026-09-12, uncommitted

Implemented the standalone standard-library classifier/fetch/reconcile tool in
`scripts/rent_history.py`, its 14 automated tests, and
[operator instructions](docs/rent-classifier.md). `scripts/check.sh all` now runs
these tests. No core schema, reporting redb, API route, external dependency,
commit or push. The live `data/explorer.redb` was not opened.

The predicate requires authenticated mature input bytes, explicitly empty proof,
a typed in-range output pointer and the accepted recreated/fully-consumed branch
under an explicitly supplied historical rule interval. Unsupported history and
missing evidence remain unresolved. Signed i32 storage-charge arithmetic is
separate from owner-tree withdrawal accounting. Generic absorber trees do not
identify claimants. Explicit family fees are counted once; multiple claims,
external funding, owner-flow ambiguity and incomplete family analysis have null
allocations with reasons. Miner/bot mode is explicit and fee-free unresolved
parents are not labeled miners. The implementation conservatively leaves all
mixed-input gross amounts null, even where further analysis might separate flows.

Confirmed negative controls run the entire classifier over repository raw blocks
1,866,001 and 1,866,002. Both contain pinned emission and full-standard-fee
collection transactions with empty proofs and ages 1–2. Both yield zero claims,
zero unresolved classification predicates, and explicit age exclusions for every
control input. Missing non-control input boxes limit census accounting but their
proof/selector evidence excludes the rent path. This is not the owner's ten-block
sample. Synthetic tests are algorithm checks, not positive consensus certification.

The runner uses only completed Phase 1 candidate heights and the configured node,
checks the Phase 1 canonical tip and cached block anchors, resolves confirmed spent
boxes from the node, retains per-height JSON evidence/checkpoints, and emits a fresh
complete JSONL stream on resume. Twelve bounded workers report progress. The owner's
94 blocks/s measurement does not include this implementation's spent-box lookups;
no new runtime/throughput claim is made. Evidence assurance is trusted node response,
not independent proof/body commitment validation.

Reconciliation compares exact candidate and verified txid sets, reports matched,
only-ours, only-census, amount differences and available input reasons. It requires
all candidate heights in the census window, the independent 5,116 IDs and supplied
census amounts to pass. **Not reconciled:** the actual candidate JSONL, independent
census export and audited historical parameter/rule schedule were not available.
The configured example node socket probe was rejected by sandbox policy. Live
backfill, real positive-branch oracle validation, the owner's ten-block control and
performance measurement were not run. No fabricated census match or 5,116-count
claim is made.

Final gate: `CARGO_TARGET_DIR="$PWD/target" ./scripts/check.sh all`.
Evidence: [results](artifacts/check/all-dui5p9Qy/results.log),
[manifest](artifacts/check/all-dui5p9Qy/manifest.log),
[source hashes](artifacts/check/all-dui5p9Qy/candidate-source-sha256.json).
New files are copied under that run's `candidate-files/` because untracked files
are absent from `git diff`. This ledger update follows the tested code.

| Gate | Result |
|---|---|
| Rent classifier | PASS, 14 tests including both confirmed-block negative controls |
| Capacity collector, formatting, workspace/all-target Clippy | PASS |
| Rust workspace tests | Non-socket suites pass. Socket suites **not run: sandbox**: 6 fallback and 13 rust_node bind refusals; overall NOT VERIFIED |
| Frontend tests/check/lint/build | PASS; 124 tests in 17 files |
| Playwright | **not run: sandbox**; mock-server bind at 127.0.0.1:18099 rejected with EPERM |
| Overall | Exit 1; not a green all-gates claim |

No `npm ci` ran, and Cargo used the repository target directory, not `/tmp`.
The earlier `all-dGGymv7R` run is superseded by `all-dui5p9Qy`.

## M5 steps 5–6 — strict frontend continuation and compatibility (2026-09-12)

Candidate: uncommitted changes on `fix/explorer-exit-code-and-rent-truth`, following
2937523 / b437259. No commit, push, dependency installation or schema change.

All ordinary first-party pagers send strict cursor/snapshot pairs. HTTP 409 clears
rows and both continuation fields, then latches further loads until the user
clicks Restart. The message is “The chain changed. Restart to load the updated list.”
A generation guard discards in-flight results after filter resets; list rendering
and filter-driven pager replacement remain unchanged. The homepage block-window
walk also carries the pair and discards its result on error. Historical routes and
capped samples keep their separate contracts. The expanded homepage transaction
card (limit 12) and bounded expanded block transaction table remain exempt;
their pinned tests are unchanged.

Evidence tests:
- `summaryLists.test.ts`: disjoint pre/post-409 IDs; repeated load attempts make
  no request until explicit restart; final accumulated rows contain only the new
  chain; cursor/snapshot pairs asserted.
- `pager.test.ts`: late responses after filter reset cannot append stale rows.
- `txs.spec.ts`: browser Restart message/button, empty rows after 409, only new
  rows after restart. **not run: sandbox**.
- `m5_pre_m5_client_parses_legacy_pages_and_strict_amounts_and_ids`: real router,
  unchanged legacy cursor requests deserialized into pre-M5 structs, then strict
  responses deserialized into the same structs.
- `m5_strict_expanded_wire_preserves_decimal_amounts_and_hex_ids`: transaction,
  box and token IDs stay lowercase hex strings; fee, value and token amount stay
  decimal strings. Both new Rust tests passed.
- Five existing client URL assertions and the summary URL assertions now require
  strict parameters and paired tokens; their response assertions were retained.
  These encoded pre-strict frontend requests and were updated for the opt-in.

The deliberate bare-cursor best-effort exception is prominent at the start of
`bin/explorer/README.md` §API, with strict usage and 400/409 behavior.

Frontend verification: `npm test` (126 tests / 17 files), `npm run check`
(zero errors/warnings), `npm run lint`, `npm run build` passed.
Bundle: **59.27 KB gzipped / 120 KB** in the final gate (earlier build: 59.28 KB).
Cargo and fixture scratch use repository target/artifact directories, never /tmp.

Final gate: `CARGO_TARGET_DIR="$PWD/target" ./scripts/check.sh all`, exit 1.
[Results](artifacts/check/all-leqV8asm/results.log) and
[revision/dirty-diff manifest](artifacts/check/all-leqV8asm/manifest.log).
Supersedes the intermediate `all-NEHTJExV` run. This results-only ledger update
follows the tested candidate.

| Gate | Result |
|---|---|
| Rent classifier / capacity collector / fmt / Clippy | PASS |
| Rust workspace tests | Non-socket suites PASS, including both new compatibility tests; fallback (6) and rust_node (13) socket tests **not run: sandbox**, bind PermissionDenied |
| npm test / check / lint / build | PASS; 126 tests, zero type errors/warnings, 59.27 KB gzipped |
| Playwright | **not run: sandbox**; mock-server bind to 127.0.0.1:18099 rejected with EPERM |
| Overall | Exit 1; full G NOT VERIFIED in sandbox |
