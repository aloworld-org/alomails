#!/usr/bin/env bash
# Obtain the INITIAL Let's Encrypt certificate. Run this ONCE, before the
# first `docker compose up`, from deploy/production/.
#
# It runs certbot in standalone mode, which binds port 80 to answer the
# HTTP-01 challenge — so DNS for <domain> and mta-sts.<domain> must already
# point at this server, and nothing else may hold port 80 while it runs.
# The certificate lands in the shared `certs` volume at
# /certs/live/<domain>/, which Caddy, SMTP and IMAP all read. After this,
# the `certbot` service in the compose renews it automatically.
set -euo pipefail
cd "$(dirname "$0")"

# shellcheck disable=SC1091
[ -f .env ] && set -a && . ./.env && set +a
: "${DOMAIN:?set DOMAIN in .env}"
: "${ACME_EMAIL:?set ACME_EMAIL in .env}"

# Override the renewal-loop entrypoint back to the certbot binary for this
# one-off issuance, and publish port 80 for the challenge.
docker compose run --rm --service-ports --entrypoint certbot certbot \
	certonly --standalone --non-interactive --agree-tos --no-eff-email \
	--email "${ACME_EMAIL}" \
	-d "${DOMAIN}" -d "mta-sts.${DOMAIN}"

echo
echo "Certificate obtained for ${DOMAIN} (+ mta-sts.${DOMAIN}) in the shared volume."
echo "Next: docker compose up -d --build"
