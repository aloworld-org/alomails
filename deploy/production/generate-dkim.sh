#!/usr/bin/env bash
# Generate the DKIM signing key and print the DNS record to publish.
# The key is 2048-bit RSA in PKCS#8 PEM (what alo-smtp expects), written
# to ./dkim/dkim.key with 0600 permissions (the server refuses a
# group/world-readable key). Re-running refuses to clobber an existing key.
#
# Usage: ./generate-dkim.sh   (reads ALO_SMTP_DKIM_SELECTOR and the mail
#                              domain from .env)
set -euo pipefail
cd "$(dirname "$0")"

# shellcheck disable=SC1091
[ -f .env ] && set -a && . ./.env && set +a

SELECTOR="${ALO_SMTP_DKIM_SELECTOR:-fic}"
DOMAIN="${ALO_SMTP_DKIM_DOMAIN:-${ALO_SMTP_LOCAL_DOMAINS%%,*}}"
KEY=./dkim/dkim.key

if [ -z "${DOMAIN}" ]; then
	echo "error: set ALO_SMTP_DKIM_DOMAIN (or ALO_SMTP_LOCAL_DOMAINS) in .env first" >&2
	exit 1
fi
if [ -f "${KEY}" ]; then
	echo "error: ${KEY} already exists — refusing to overwrite a live signing key" >&2
	exit 1
fi

mkdir -p ./dkim
umask 077
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out "${KEY}" 2>/dev/null
chmod 600 "${KEY}"

PUB=$(openssl rsa -in "${KEY}" -pubout -outform DER 2>/dev/null | openssl base64 -A)

echo "Wrote ${KEY} (0600)."
echo
echo "Publish this DNS TXT record, then set ALO_SMTP_DKIM_* in .env:"
echo
echo "  Host:  ${SELECTOR}._domainkey.${DOMAIN}"
echo "  Type:  TXT"
echo "  Value: v=DKIM1; k=rsa; p=${PUB}"
echo
echo "(Long TXT values may need splitting into 255-char chunks at your DNS host.)"
