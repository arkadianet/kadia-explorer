# Explorer delivery status

Updated 2026-09-12. This is the delivery-status authority referenced by the
[September 10 history design](docs/superpowers/specs/2026-09-10-history-and-rent-reporting-design.md).
Historical design text is unchanged. Current scope is the accepted
[completion design](docs/superpowers/specs/2026-09-12-explorer-done-design.md) and
[M1–M6 plan](docs/superpowers/plans/2026-09-12-explorer-done-plan.md), subject to the
owner's M1 instructions: implement steps 1–3; document, but do not execute, steps 4–5.

States: `planned` = agreed work without implementation; `implemented` = code or
procedure exists; `verified` = specified acceptance evidence exists for the named
revision; `released` = deployment plus post-deployment smoke is evidenced.
An implemented item is not necessarily verified. Missing evidence never means pass.
CUT is a scope decision, not a delivery state. No release is certified here.

Working revision: `8a0e3dc76b7de4b5564d64f69700fc25682af65e` plus the uncommitted M1
diff, branch `fix/explorer-exit-code-and-rent-truth`. There is no M1 commit SHA yet.
The two accepted September 12 planning documents remain untracked for the reviewer
to commit with the implementation. No commit, push, PR, protection change or
production operation was performed in this implementation session.

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
| M2 integrity and transition model | planned | Source matrix, corruption/partial cases, seeded histories, UNDO mutation and G: absent |
| M3 bounded reads and telemetry | planned | Summary lookup counts, boundary tests, latency/RSS/drain and durable restart evidence: absent |
| M4 capacity and recovery | planned | Table attribution, growth/headroom, atomic cap tests, full-size checksummed restore/catch-up: absent |
| M5 continuation | planned | Route policy matrix, multi-page/409 compatibility, frontend reset tests and G: absent |
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
- Legacy first-ID canonical guessing (removal belongs to M2, not yet implemented);
  body-only fallback stays.
- Guaranteed 30–40 GB physical size, byte-identical redb restoration, restoring
  pruned historical UNDO, and isolated resync benchmarking. Measured attribution,
  retention-aware logical identity and full-size restore/catch-up remain required.

The original M1 estimate conflated code work with owner-controlled enforcement and
procurement. YAML alone cannot make gates unavoidable; steps 4–5 and a green hosted
candidate remain outstanding. “Pin the version that passes” must not turn a local
stable resolution into a claim about unrecorded reviewer versions. Full-size restore
is the recovery obligation; resync is not. No historical planning text was rewritten.
