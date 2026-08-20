#!/usr/bin/env bash
# Build one alo web surface and publish it to the single-server deployment.
# Caddy serves the files directly from a mounted directory, so a publish is
# a static-file copy — no container restart, effective immediately.
#
# Usage:
#   DEPLOY_HOST=root@mail.example.com DEPLOY_KEY=~/.ssh/alo_deploy ./deploy-web.sh
#   ALO_PRODUCT=workplace DEPLOY_HOST=… ./deploy-web.sh
#
# **Two surfaces are served from this one host** (ADR 0019), and each has its
# own mount in `docker-compose.yml`:
#
#   mail      → ./web      → /srv (mail.alomails.com)
#   workplace → ./web-workplace → /srv-workplace (app.aloworkplace.com)
#
# The product decides the directory, so publishing one can never overwrite the
# other. The workspace build was staged by hand before this script grew the
# second case, which is how it went stale without anybody noticing: there is no
# such thing as "the" web deploy here, and a script that only knew one of them
# quietly meant the other was nobody's job.
#
# One-time, before the first publish, register the web app as an OIDC client:
#   docker compose exec alo-jmap identityctl register-client \
#     web "alo Web" https://<domain>/auth/callback
set -euo pipefail

HOST="${DEPLOY_HOST:?set DEPLOY_HOST=user@host}"
KEY="${DEPLOY_KEY:-}"

# A plain `npm run build` defaults to the full workplace surface, so the mail
# deployment must state its product; this script keeps that default rather than
# changing what an existing invocation does.
export ALO_PRODUCT="${ALO_PRODUCT:-mail}"

# Where each surface is mounted from. An unknown product stops here rather than
# publishing a workplace build over alomails.
case "$ALO_PRODUCT" in
  mail)      remote_name="web" ;;
  workplace) remote_name="web-workplace" ;;
  *) echo "ERROR: ALO_PRODUCT must be one of: mail, workplace (got '$ALO_PRODUCT')" >&2; exit 2 ;;
esac
REMOTE_DIR="/opt/alo/deploy/production/$remote_name"

ssh_cmd=(ssh)
scp_cmd=(scp)
if [ -n "$KEY" ]; then
  ssh_cmd=(ssh -i "$KEY")
  scp_cmd=(scp -i "$KEY")
fi

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root/web"

echo "==> building the web app (ALO_PRODUCT=$ALO_PRODUCT)"
npm ci
npm run build

echo "==> publishing to $HOST:$REMOTE_DIR"
"${ssh_cmd[@]}" "$HOST" "mkdir -p '$REMOTE_DIR' && rm -rf '$REMOTE_DIR'/*"
"${scp_cmd[@]}" -r dist/* "$HOST:$REMOTE_DIR/"

echo "==> done — served immediately by Caddy (static files, no restart)"
