#!/usr/bin/env bash
# Builds the explorer frontend and ships it to the production host.
#
#   scripts/deploy_frontend.sh [--caddy] [user@host] [site-url]
#
# --caddy  also installs deploy/caddy/explorer.kadia.io.Caddyfile as the host's
#          /etc/caddy/Caddyfile (backing the current one up, and only after `caddy validate`
#          accepts the new file on the host). Omitted by default: a plain run only ships the
#          built assets and reloads Caddy with the config it already has.
#
# The health check at the end hits $SITE_URL (2nd argument, or the SITE_URL environment
# variable, default https://explorer.kadia.io) so a deploy to another host checks that host's
# own domain rather than production's.
set -euo pipefail

CADDY=0
positional=()
for arg in "$@"; do
	case "$arg" in
		--caddy) CADDY=1 ;;
		-*) echo "unknown flag: $arg" >&2; exit 2 ;;
		*) positional+=("$arg") ;;
	esac
done

HOST="${positional[0]:-root@167.233.240.191}"
SITE_URL="${positional[1]:-${SITE_URL:-https://explorer.kadia.io}}"
KEY="${SSH_KEY:-$HOME/.ssh/hetzner_vps}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CADDYFILE="$ROOT/deploy/caddy/explorer.kadia.io.Caddyfile"
SSH=(ssh -i "$KEY" -o BatchMode=yes)

cd "$ROOT/frontend"
npm ci --silent && npm run build
rsync -az --delete -e "ssh -i $KEY -o BatchMode=yes" build/ "$HOST:/var/www/explorer/"

if [ "$CADDY" -eq 1 ]; then
	echo "installing $CADDYFILE on $HOST:/etc/caddy/Caddyfile"
	scp -i "$KEY" -o BatchMode=yes "$CADDYFILE" "$HOST:/tmp/Caddyfile.new"
	"${SSH[@]}" "$HOST" 'set -euo pipefail
		caddy validate --config /tmp/Caddyfile.new --adapter caddyfile
		cp -a /etc/caddy/Caddyfile "/etc/caddy/Caddyfile.bak.$(date +%Y%m%d-%H%M%S)"
		mv /tmp/Caddyfile.new /etc/caddy/Caddyfile'
fi

"${SSH[@]}" "$HOST" "systemctl reload caddy"
"${SSH[@]}" "$HOST" "curl -s -o /dev/null -w '%{http_code}\n' '$SITE_URL/'"
