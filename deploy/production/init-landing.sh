#!/usr/bin/env bash
# Bring the apex marketing site (alomails.com) online with its own Let's Encrypt
# certificate. Run ONCE from deploy/production/, AFTER the apex DNS points here:
#
#     A    alomails.com       -> this server's IP
#     A    www.alomails.com   -> this server's IP   (optional; redirects to apex)
#
# mail.alomails.com — the app — is NOT touched. The apex gets a SEPARATE
# certificate (a distinct cert, not a SAN on the mail cert), so the app's TLS is
# never at risk. Certbot's renew loop then keeps it fresh automatically.
#
# The ordering solves a chicken-and-egg: Caddy's apex TLS block can't load
# without a cert, but issuing the cert needs Caddy serving the HTTP-01 challenge.
# So we seed a throwaway self-signed cert, load Caddy (it answers the challenge),
# obtain the real cert, and reload.
set -euo pipefail
cd "$(dirname "$0")"

# shellcheck disable=SC1091
[ -f .env ] && set -a && . ./.env && set +a
: "${ACME_EMAIL:?set ACME_EMAIL in .env}"
APEX="alomails.com"

echo "==> Resolving ${APEX} (should be this server)…"
getent hosts "${APEX}" || echo "   (could not resolve locally; continuing)"

echo "==> 1/4 Seeding a throwaway self-signed cert so Caddy can load the apex TLS block"
docker compose run --rm --entrypoint sh certbot -c "\
  mkdir -p /etc/letsencrypt/live/${APEX} && \
  openssl req -x509 -newkey rsa:2048 -nodes -days 1 \
    -keyout /etc/letsencrypt/live/${APEX}/privkey.pem \
    -out    /etc/letsencrypt/live/${APEX}/fullchain.pem \
    -subj '/CN=${APEX}'"

echo "==> 2/4 (Re)loading Caddy with the apex blocks + landing mount"
docker compose up -d caddy
sleep 3

echo "==> 3/4 Obtaining the real certificate (webroot; Caddy serves the challenge)"
# Drop the seed so certbot owns the live dir cleanly. Caddy keeps serving the
# cached seed cert until the reload in step 4, so there is no gap.
docker compose run --rm --entrypoint sh certbot -c "\
  rm -rf /etc/letsencrypt/live/${APEX} /etc/letsencrypt/archive/${APEX} \
         /etc/letsencrypt/renewal/${APEX}.conf"
docker compose run --rm --entrypoint certbot certbot \
  certonly --webroot -w /var/www/certbot --non-interactive --agree-tos --no-eff-email \
  --email "${ACME_EMAIL}" -d "${APEX}" -d "www.${APEX}"

echo "==> 4/4 Restarting Caddy to pick up the real certificate"
# A graceful `caddy reload` does NOT re-read a file-based cert whose path is
# unchanged (it served the seed cert from step 1 straight through). A restart
# forces a fresh read of the now-real cert; the blip is ~1s.
docker compose restart caddy

echo
echo "Done — https://${APEX} is live (the app stays on https://mail.${APEX})."
