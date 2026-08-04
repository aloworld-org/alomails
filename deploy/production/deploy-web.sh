#!/usr/bin/env bash
# Build the alo web app and publish it to the single-server deployment.
# Caddy serves the files directly from a mounted directory, so a publish is
# a static-file copy — no container restart, effective immediately.
#
# Usage:
#   DEPLOY_HOST=root@mail.example.com DEPLOY_KEY=~/.ssh/alo_deploy ./deploy-web.sh
#
# One-time, before the first publish, register the web app as an OIDC client:
#   docker compose exec alo-jmap identityctl register-client \
#     web "alo Web" https://<domain>/auth/callback
set -euo pipefail

HOST="${DEPLOY_HOST:?set DEPLOY_HOST=user@host}"
KEY="${DEPLOY_KEY:-}"
REMOTE_DIR="/opt/alo/deploy/production/web"

ssh_cmd=(ssh)
scp_cmd=(scp)
if [ -n "$KEY" ]; then
  ssh_cmd=(ssh -i "$KEY")
  scp_cmd=(scp -i "$KEY")
fi

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root/web"

# This deployment IS alomails (mail.alomails.com): build the mail surface —
# Home + Mail + Agenda, no suite modules (Chat/Drive/Meet). A plain build
# defaults to the full workplace surface, so ALO_PRODUCT=mail is mandatory here.
export ALO_PRODUCT="${ALO_PRODUCT:-mail}"
echo "==> building the web app (ALO_PRODUCT=$ALO_PRODUCT)"
npm ci
npm run build

echo "==> publishing to $HOST:$REMOTE_DIR"
"${ssh_cmd[@]}" "$HOST" "mkdir -p '$REMOTE_DIR' && rm -rf '$REMOTE_DIR'/*"
"${scp_cmd[@]}" -r dist/* "$HOST:$REMOTE_DIR/"

echo "==> done — served immediately by Caddy (static files, no restart)"
