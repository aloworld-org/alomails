#!/usr/bin/env bash
# Bring the workspace umbrella (aloworkplace.com) online with its own Let's
# Encrypt certificate. Run ONCE from deploy/production/, AFTER:
#   1. aloworkplace.com DNS points at this server:
#        A    aloworkplace.com       -> this server's IP
#        A    www.aloworkplace.com   -> this server's IP   (redirects to apex)
#   2. the updated Caddyfile (with the aloworkplace.com blocks), the updated
#      docker-compose.yml (with the ./web-workplace:/srv-workplace mount), and
#      the workspace web build in ./web-workplace/ are all present here.
#
# mail.alomails.com and alomails.com are NOT touched — aloworkplace.com gets a
# SEPARATE certificate, so the mail app's TLS is never at risk. Certbot's renew
# loop then keeps it fresh automatically.
#
# Chicken-and-egg (same as init-landing.sh): Caddy's TLS block can't load
# without a cert, but issuing the cert needs Caddy serving the HTTP-01 challenge.
# So: seed a throwaway self-signed cert, load Caddy (it answers the challenge),
# obtain the real cert, restart.
set -euo pipefail
cd "$(dirname "$0")"

# shellcheck disable=SC1091
[ -f .env ] && set -a && . ./.env && set +a
: "${ACME_EMAIL:?set ACME_EMAIL in .env}"
HOST="aloworkplace.com"

echo "==> Checking prerequisites…"
[ -f web-workplace/index.html ] || { echo "   ERROR: web-workplace/index.html missing — stage the workspace build first."; exit 1; }
grep -q "aloworkplace.com" Caddyfile || { echo "   ERROR: Caddyfile has no aloworkplace.com blocks — sync the updated Caddyfile first."; exit 1; }
grep -q "web-workplace:/srv-workplace" docker-compose.yml || { echo "   ERROR: compose missing the /srv-workplace mount — sync the updated compose first."; exit 1; }

echo "==> Resolving ${HOST} (should be this server)…"
getent hosts "${HOST}" || echo "   (could not resolve locally; continuing)"

echo "==> 1/4 Seeding a throwaway self-signed cert so Caddy can load the ${HOST} TLS block"
docker compose run --rm --entrypoint sh certbot -c "\
  mkdir -p /etc/letsencrypt/live/${HOST} && \
  openssl req -x509 -newkey rsa:2048 -nodes -days 1 \
    -keyout /etc/letsencrypt/live/${HOST}/privkey.pem \
    -out    /etc/letsencrypt/live/${HOST}/fullchain.pem \
    -subj '/CN=${HOST}'"

echo "==> 2/4 (Re)creating Caddy with the workspace mount + blocks"
docker compose up -d caddy
sleep 3

echo "==> 3/4 Obtaining the real certificate (webroot; Caddy serves the challenge)"
docker compose run --rm --entrypoint sh certbot -c "\
  rm -rf /etc/letsencrypt/live/${HOST} /etc/letsencrypt/archive/${HOST} \
         /etc/letsencrypt/renewal/${HOST}.conf"
docker compose run --rm --entrypoint certbot certbot \
  certonly --webroot -w /var/www/certbot --non-interactive --agree-tos --no-eff-email \
  --email "${ACME_EMAIL}" -d "${HOST}" -d "www.${HOST}"

echo "==> 4/4 Restarting Caddy to pick up the real certificate"
docker compose restart caddy

echo
echo "Done — https://${HOST} is live (the full workspace: all products, one login)."
echo "Mail stays on https://mail.alomails.com, alomails.com stays the landing page."
