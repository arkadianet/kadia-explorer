#!/usr/bin/env bash
# Release parity for an explicitly supplied full-history explorer and reference node.
# Exit 0 = pass, 1 = fail, 2 = inconclusive. No implicit network targets.
set -euo pipefail
EXPLORER_URL="${1:-${EXPLORER_URL:-}}"
NODE_URL="${2:-${NODE_URL:-}}"
if [[ -z "$EXPLORER_URL" || -z "$NODE_URL" ]]; then
    echo '{"outcome":"inconclusive","reason":"explicit explorer and reference URLs are required"}'
    exit 2
fi
if [[ $# -gt 0 ]]; then shift; fi
if [[ $# -gt 0 ]]; then shift; fi
cd "$(dirname "${BASH_SOURCE[0]}")/.."
parity_log=$(mktemp)
trap 'rm -f "$parity_log"' EXIT
set +e
PARITY_COMMIT="$(git rev-parse HEAD)" EXPLORER_URL="$EXPLORER_URL" NODE_URL="$NODE_URL" \
    cargo test -p xp-api --test parity parity_against_node -- --ignored --nocapture "$@" > "$parity_log" 2>&1
parity_exit=$?
set -e
cat "$parity_log"
python3 - "$parity_log" "$parity_exit" <<'PY'
import json, sys
result = None
for line in open(sys.argv[1]):
    try:
        value = json.loads(line)
    except ValueError:
        continue
    if isinstance(value, dict) and value.get('outcome') in ('pass', 'fail', 'inconclusive'):
        result = value
if result is None:
    result = {'outcome': 'inconclusive', 'reason': 'no completed parity evidence artifact'}
if result['outcome'] == 'pass' and int(sys.argv[2]) != 0:
    result = {'outcome': 'inconclusive', 'reason': 'test process did not complete successfully'}
print(json.dumps(result))
sys.exit({'pass': 0, 'fail': 1, 'inconclusive': 2}[result['outcome']])
PY
