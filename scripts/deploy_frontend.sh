#!/usr/bin/env bash
set -euo pipefail
HOST="${1:-root@167.233.240.191}"; KEY="${SSH_KEY:-$HOME/.ssh/hetzner_vps}"
cd "$(dirname "$0")/../frontend"
npm ci --silent && npm run build
rsync -az --delete -e "ssh -i $KEY -o BatchMode=yes" build/ "$HOST:/var/www/explorer/"
ssh -i "$KEY" -o BatchMode=yes "$HOST" 'systemctl reload caddy && curl -s -o /dev/null -w "%{http_code}\n" https://explorer.kadia.io/'
