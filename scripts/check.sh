#!/usr/bin/env bash
# G entry point. Provision Rust, npm ci and Chromium first; CI uses the same subsets.
# Usage: scripts/check.sh [all|rust|frontend]
# Exit 0: all selected gates passed. Exit 1: failure or environment refusal.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
root=$PWD
suite=${1:-all}
if [[ $# -gt 1 || ! $suite =~ ^(all|rust|frontend)$ ]]; then
    echo 'Usage: scripts/check.sh [all|rust|frontend]' >&2
    exit 2
fi
mkdir -p "$root/artifacts/check"
run_dir=$(mktemp -d "$root/artifacts/check/${suite}-XXXXXXXX")
# Keep fixture/compiler temporary files off shared /tmp. Respect Cargo's target configuration.
mkdir "$run_dir/tmp"
export TMPDIR="$run_dir/tmp"
failed=0
frontend_passed=0
printf 'Evidence: %s\n' "$run_dir"
record_manifest() (
    set -e
    date -u +%FT%TZ
    git rev-parse HEAD
    git status --short
    printf 'suite=%s RUSTUP_TOOLCHAIN=%s CARGO_TARGET_DIR=%s\n' "$suite" "${RUSTUP_TOOLCHAIN:-default}" "${CARGO_TARGET_DIR:-cargo-config-default}"
    if [[ $suite != frontend ]]; then rustc --version; cargo --version; fi
    if [[ $suite != rust ]]; then
        node --version
        npm --version
        (cd frontend && ./node_modules/.bin/playwright --version) || echo 'Playwright version unavailable: provision npm ci'
    fi
    sha256sum Cargo.lock frontend/package-lock.json rust-toolchain.toml frontend/.node-version scripts/check.sh .github/workflows/ci.yml
    git diff --binary
)
set +e
record_manifest > "$run_dir/manifest.log" 2>&1
manifest_status=$?
set -e
if (( manifest_status != 0 )); then
    echo 'Manifest recording failed; inspect manifest.log' >&2
    failed=1
fi
run_gate() {
    local name=$1 directory=$2 command=$3
    local -a statuses
    printf '\n>>> %s: %s\n' "$name" "$command"
    if [[ $name == playwright && ! -x "$root/frontend/node_modules/.bin/playwright" ]]; then
        printf 'playwright: NOT RUN — missing locked dependency; run npm ci first (no npx auto-install attempted)\n' | tee "$run_dir/$name.log" | tee -a "$run_dir/results.log"
        failed=1
        return
    fi
    set +e
    (cd "$directory" && bash -o pipefail -c "$command") 2>&1 | tee "$run_dir/$name.log"
    statuses=("${PIPESTATUS[@]}")
    set -e
    if (( statuses[0] == 0 && statuses[1] == 0 )); then
        if [[ $name == frontend ]]; then frontend_passed=1; fi
        printf '%s: PASS (exit 0)\n' "$name" | tee -a "$run_dir/results.log"
    else
        failed=1
        # Diagnostic only: permission text cannot prove all failures share that cause.
        # Never convert a refusal (or mixed failure/refusal) to a successful check.
        if grep -Eq 'PermissionDenied|Permission denied|Operation not permitted|EPERM|EACCES|Read-only file system|EROFS|EAI_AGAIN|ENETUNREACH|ENOTCACHED|Disk quota exceeded|No space left on device' "$run_dir/$name.log"; then
            printf '%s: NOT VERIFIED — environment refusal detected (not run: sandbox when confirmed); inspect log for additional failures (command=%s log=%s)\n' "$name" "${statuses[0]}" "${statuses[1]}" | tee -a "$run_dir/results.log"
        elif (( statuses[0] == 127 )); then
            printf '%s: NOT RUN — missing prerequisite (exit 127); inspect log and provision tools/dependencies\n' "$name" | tee -a "$run_dir/results.log"
        else
            printf '%s: FAIL (command=%s log=%s); inspect log\n' "$name" "${statuses[0]}" "${statuses[1]}" | tee -a "$run_dir/results.log"
        fi
    fi
}
if [[ $suite != frontend ]]; then
    run_gate rent-classifier "$root" 'PYTHONDONTWRITEBYTECODE=1 python3 scripts/test-rent-history.py'
    run_gate capacity-collector "$root" 'PYTHONDONTWRITEBYTECODE=1 python3 scripts/test-register-capacity.py'
    run_gate fmt "$root" 'cargo fmt --all -- --check'
    run_gate clippy "$root" 'cargo clippy --workspace --all-targets -- -D warnings'
    run_gate rust-tests "$root" 'cargo test --workspace --no-fail-fast'
fi
if [[ $suite != rust ]]; then
    run_gate frontend "$root/frontend" 'npm test && npm run check && npm run lint && npm run build'
    run_gate playwright "$root/frontend" 'npx playwright test --workers=4'
fi
# Build hashes are supplementary; logs and manifest identify unsuccessful builds too.
if [[ $frontend_passed == 1 && -d frontend/build ]]; then
    find frontend/build -type f -exec sha256sum {} + > "$run_dir/frontend-build.sha256"
fi
sha256sum "$run_dir"/*.log > "$run_dir/logs.sha256"
printf '\nSelected suite: %s; exit %s; evidence: %s\n' "$suite" "$failed" "$run_dir"
exit "$failed"
