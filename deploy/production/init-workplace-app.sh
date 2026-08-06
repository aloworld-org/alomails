#!/usr/bin/env bash
# Split aloworkplace.com into a landing page (apex) + the workspace app
# (app.aloworkplace.com). Run ONCE from deploy/production/, AFTER:
#   1. app.aloworkplace.com DNS points at this server:
#        A    app.aloworkplace.com   -> this server's IP
#   2. the updated Caddyfile (apex→landing, app→app), the updated compose (with
#      the ./web-workplace-landing mount), the landing files in
#      ./web-workplace-landing/, and the workspace build in ./web-workplace/ are
#      all present here.
#
# The apex cert already exists (init-workplace.sh issued it for aloworkplace.com
# + www). This EXPANDS that same cert to also cover app.aloworkplace.com — one
# cert, three names — so the app block can serve TLS. certbot --expand only
# replaces the cert on success, so a failure leaves the current cert intact.
set -euo pipefail
cd "$(dirname "$0")"

# shellcheck disable=SC1091
[ -f .env ] && set -a && . ./.env && set +a
: "${ACME_EMAIL:?set ACME_EMAIL in .env}"

echo "==> Checking prerequisites…"
[ -f web-workplace-landing/index.html ] || { echo "   ERROR: web-workplace-landing/index.html missing — stage the landing first."; exit 1; }
[ -f web-workplace/index.html ] || { echo "   ERROR: web-workplace/index.html missing — stage the workspace build first."; exit 1; }
grep -q "app.aloworkplace.com" Caddyfile || { echo "   ERROR: Caddyfile has no app.aloworkplace.com block — sync the updated Caddyfile first."; exit 1; }
grep -q "web-workplace-landing:/srv-workplace-landing" docker-compose.yml || { echo "   ERROR: compose missing the landing mount — sync the updated compose first."; exit 1; }

echo "==> Resolving app.aloworkplace.com (should be this server)…"
getent hosts app.aloworkplace.com || echo "   (could not resolve locally; continuing)"

# Order matters: Caddy must be serving the app.aloworkplace.com HTTP block
# (the ACME challenge) BEFORE certbot runs, or Let's Encrypt can't fetch the
# challenge for the new host. The app HTTPS block loads against the existing
# 2-name cert in the meantime (a brief SAN mismatch on app.aloworkplace.com,
# resolved by the expand + restart below).
echo "==> 1/3 (Re)creating Caddy with the landing mount + the app block"
docker compose up -d caddy
sleep 3

echo "==> 2/3 Expanding the aloworkplace.com certificate to cover app.aloworkplace.com"
docker compose run --rm --entrypoint certbot certbot \
  certonly --webroot -w /var/www/certbot --non-interactive --agree-tos --no-eff-email \
  --expand --cert-name aloworkplace.com \
  --email "${ACME_EMAIL}" \
  -d aloworkplace.com -d www.aloworkplace.com -d app.aloworkplace.com

echo "==> 3/3 Restarting Caddy to pick up the expanded certificate"
docker compose restart caddy

echo
echo "Done:"
echo "  https://aloworkplace.com      → the marketing landing page"
echo "  https://app.aloworkplace.com  → the workspace (all products, one login)"
