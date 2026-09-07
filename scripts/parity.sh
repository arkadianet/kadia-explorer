#!/usr/bin/env bash
# Runs the parity gate (crates/xp-api/tests/parity.rs) against a live explorer and a live
# Rust node. Both default to the ports the project's own dev setup uses.
#
# What it checks:
#   - a sample of addresses seen in recent explorer blocks: balance, unspent box-id set, and
#     tx count (read straight off `GET /v1/addresses/{addr}`'s `tx_count` field — no page
#     walk needed there, unlike the unspent box-id set, which is still walked and page-cap
#     skipped since the API has no direct count for it)
#   - a sample of 20 token ids collected off boxes seen during the address comparison:
#     emission and name from `GET /v1/tokens/{id}` vs. the node's `/blockchain/token/byId`,
#     and unspent box count via `/v1/tokens/{id}/boxes?unspent=true` vs. the node's
#     `/blockchain/box/unspent/byTokenId` (skipped above the node's 16384-box cap). A token
#     minted before the store's indexed range 404s on the explorer and is recorded as
#     skipped, not a mismatch.
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
