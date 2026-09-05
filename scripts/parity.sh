#!/usr/bin/env bash
# Runs the parity gate (crates/xp-api/tests/parity.rs) against a live explorer and a live
# Rust node. Both default to the ports the project's own dev setup uses.
#
# Usage: scripts/parity.sh [explorer_url] [node_url] [-- extra test-binary args]
# Any args beyond the first two positional URLs are forwarded to the test binary after
# --nocapture (e.g. `scripts/parity.sh "" "" --exact parity_against_node`).
set -euo pipefail

EXPLORER_URL="${1:-${EXPLORER_URL:-http://127.0.0.1:18090}}"
NODE_URL="${2:-${NODE_URL:-http://127.0.0.1:9063}}"
if [ "$#" -gt 0 ]; then shift; fi
if [ "$#" -gt 0 ]; then shift; fi

cd "$(dirname "${BASH_SOURCE[0]}")/.."

EXPLORER_URL="$EXPLORER_URL" NODE_URL="$NODE_URL" \
    cargo test -p xp-api --test parity -- --ignored --nocapture "$@"
