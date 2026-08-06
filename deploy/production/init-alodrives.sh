#!/usr/bin/env bash
# Bring alodrives.com online: a landing page on the apex + the standalone Drive
# app on app.alodrives.com. Run ONCE from deploy/production/, AFTER:
#   1. DNS points at this server:
#        A    alodrives.com       -> this server's IP
#        A    www.alodrives.com   -> this server's IP
#        A    app.alodrives.com   -> this server's IP
#   2. the updated Caddyfile (alodrives blocks), the updated compose (with the
#      ./web-drive + ./web-drive-landing mounts), the landing in
#      ./web-drive-landing/, and the Drive-only build in ./web-drive/ are all here.
#
# alodrives.com gets its OWN certificate (alodrives.com + www + app as SANs), so
# nothing else is touched. Same chicken-and-egg dance as init-workplace.sh: seed
# a throwaway self-signed cert so Caddy can load the TLS blocks, load Caddy (it
# answers the HTTP-01 challenge on all three hosts), obtain the real cert,
# restart.
set -euo pipefail
cd "$(dirname "$0")"

# shellcheck disable=SC1091
[ -f .env ] && set -a && . ./.env && set +a
: "${ACME_EMAIL:?set ACME_EMAIL in .env}"
HOST="alodrives.com"

echo "==> Checking prerequisites…"
[ -f web-drive-landing/index.html ] || { echo "   ERROR: web-drive-landing/index.html missing."; exit 1; }
[ -f web-drive/index.html ] || { echo "   ERROR: web-drive/index.html missing (Drive-only build)."; exit 1; }
grep -q "app.alodrives.com" Caddyfile || { echo "   ERROR: Caddyfile has no alodrives blocks."; exit 1; }
grep -q "web-drive:/srv-drive" docker-compose.yml || { echo "   ERROR: compose missing the drive mounts."; exit 1; }

echo "==> Resolving app.alodrives.com (should be this server)…"
getent hosts app.alodrives.com || echo "   (could not resolve locally; continuing)"

echo "==> 1/4 Seeding a throwaway self-signed cert so Caddy can load the alodrives TLS blocks"
docker compose run --rm --entrypoint sh certbot -c "\
  mkdir -p /etc/letsencrypt/live/${HOST} && \
  openssl req -x509 -newkey rsa:2048 -nodes -days 1 \
    -keyout /etc/letsencrypt/live/${HOST}/privkey.pem \
    -out    /etc/letsencrypt/live/${HOST}/fullchain.pem \
    -subj '/CN=${HOST}'"

echo "==> 2/4 (Re)creating Caddy with the alodrives mounts + blocks (serves the challenge)"
docker compose up -d caddy
sleep 3

echo "==> 3/4 Obtaining the real certificate (alodrives.com + www + app)"
docker compose run --rm --entrypoint sh certbot -c "\
  rm -rf /etc/letsencrypt/live/${HOST} /etc/letsencrypt/archive/${HOST} \
         /etc/letsencrypt/renewal/${HOST}.conf"
docker compose run --rm --entrypoint certbot certbot \
  certonly --webroot -w /var/www/certbot --non-interactive --agree-tos --no-eff-email \
  --email "${ACME_EMAIL}" \
  -d "${HOST}" -d "www.${HOST}" -d "app.${HOST}"

echo "==> 4/4 Restarting Caddy to pick up the real certificate"
docker compose restart caddy

echo
echo "Done:"
echo "  https://alodrives.com      → the marketing landing page"
echo "  https://app.alodrives.com  → the standalone Drive app"
