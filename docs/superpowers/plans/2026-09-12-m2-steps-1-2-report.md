# M2 steps 1–2 implementation record

Scope: canonical selection and its source failure propagation only, plus the body id/height agreement check required by step 1. M2 steps 3–7 are not started. No commit or push.

## Operator-visible change

A primary returning HTTP 404, 405 or 501 for `/blocks/chainSlice` now produces `SourceError::Capability`. There is no `/blocks/at` request or first-id selection, even if that endpoint lists only one id. The old downgrade warning and process-wide warning guard are removed.

Ingest logs the error and retries at its existing retry cadence; it does not terminate for this capability error or select headers from the body fallback. After three consecutive source failures, `/v1/status.source_error` contains the capability error (prefixed with `block fetch` or `fork check`). Successful `/info` responses do not clear this streak. The error includes the URL/status and instructs the operator to upgrade the node or configure a primary supporting `/blocks/chainSlice`. Existing indexed data remains available while progress waits for a supported primary.

Malformed or contradictory canonical entries return `SourceError::Decode`; HTTP 503 remains `SourceError::Http`. Empty slices and valid slices containing only a different height return no canonical id. Body-only fallback remains supported. Decoded bodies must match the requested canonical id and height before apply; mismatches use the existing undecodable-block halt path. Existing store apply checks still enforce height continuity and parent linkage.

## Rollout precondition

Met according to the reviewer, as relayed in the implementation request on 2026-09-12: both production primary `http://127.0.0.1:9063` and configured public fallback `https://node.ergo.watch` served `/blocks/chainSlice` with valid header arrays for a live height range. This is reviewer-provided evidence, not a measurement performed in this sandbox. No production HTTP probe was attempted here.

## Evidence

New source integration cases cover missing `chainSlice` with a single id and orphan-first competing ids (also asserting zero `/blocks/at` calls), unsupported 405/501 statuses, and malformed/contradictory responses. New socket-free parser tests cover malformed entries, contradictions, empty slices and requested-height selection. Existing 503, height-mismatch and canonical-over-orphan tests are preserved. All six body-fallback tests and their assertions are preserved; their mock node now serves canonical headers via `chainSlice`.

New ingest regressions cover capability-error retries and publication after at least three failures on both empty-store block fetch and seeded-store fork check, and rejection of bodies with a mismatched selected id or height. Source-health production wiring required no change: `fetch_range` propagates `SourceError` to `source_failed!("block fetch", ...)`, and `fork_check` to `source_failed!("fork check", ...)`; the status handler copies `source_error` into the existing DTO. The existing API regression `status_repeated_body_errors_reports_failure_and_recovers` verifies the three-failure threshold through `/v1/status`.

## Gate results

Evidence below describes local, non-retained reviewer runs. CI has never executed;
these log paths identify historical local output, not durable artifacts.

- `./scripts/check.sh all`: exit 1; evidence `artifacts/check/all-WY98xn9q`. Formatting and frontend PASS (120 tests, Svelte check, lint, build/bundle budget). Playwright not run: sandbox (`listen EPERM`). Initial Rust checks not run: sandbox (configured shared Cargo target is read-only).
- `CARGO_TARGET_DIR="$PWD/target" ./scripts/check.sh rust`: exit 1; evidence `artifacts/check/rust-41KohHBU`. Formatting and Clippy PASS. New source parser tests, both new ingest regressions, and existing API health regression PASS. Source `fallback` and `rust_node` HTTP integration suites not run: sandbox (socket `PermissionDenied`). No attempt was made to fix socket restrictions.
- This Rust run also exposed a pre-existing arbitrary id in the short-chain parent-mismatch mock. Its advertised id now matches its body, preserving the intended parent mismatch and all original assertions. Final focused orphan-suite and Clippy verification is recorded below.
- No target directory under `/tmp`; no `npm ci`, new dependencies, core row encoding changes, DTO changes or wire contract changes.
- Final verification: `CARGO_TARGET_DIR="$PWD/target" cargo test -p xp-ingest --test orphan` PASS (3/3); `CARGO_TARGET_DIR="$PWD/target" cargo clippy --workspace --all-targets -- -D warnings` PASS; formatting and `git diff --check` PASS. The only remaining test targets blocked in the workspace run are the two socket-bound source suites; Playwright remains blocked as above. The aggregate gate remains nonzero, not a full PASS.
