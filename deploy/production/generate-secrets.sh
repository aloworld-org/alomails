#!/usr/bin/env bash
# Fill the secret values in .env with fresh cryptographic randomness.
# Idempotent-ish: it will NOT overwrite a secret that is already set (so
# re-running is safe and never rotates a live password by accident).
#
# Usage: ./generate-secrets.sh   (run from deploy/production/)
set -euo pipefail

cd "$(dirname "$0")"

if [ ! -f .env ]; then
	cp .env.example .env
	echo "created .env from .env.example — edit DOMAIN / ALO_SMTP_LOCAL_DOMAINS / ACME_EMAIL before bringing the stack up"
fi

rand() { openssl rand -base64 32 | tr -d '/+=' | cut -c1-40; }

# set_secret KEY : fill KEY= in .env with a fresh secret only if it is empty.
set_secret() {
	local key="$1"
	local current
	current=$(grep -E "^${key}=" .env | head -1 | cut -d= -f2- || true)
	if [ -z "${current}" ]; then
		local value
		value=$(rand)
		# Portable in-place edit (BSD + GNU sed differ on -i).
		if sed --version >/dev/null 2>&1; then
			sed -i "s|^${key}=.*|${key}=${value}|" .env
		else
			sed -i '' "s|^${key}=.*|${key}=${value}|" .env
		fi
		echo "set ${key} (new secret)"
	else
		echo "kept ${key} (already set)"
	fi
}

set_secret POSTGRES_PASSWORD

echo
echo "Secrets are in .env (gitignored). Next: ./generate-dkim.sh, then bring the stack up (see README.md)."
