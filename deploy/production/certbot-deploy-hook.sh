#!/bin/sh
# certbot deploy hook: runs inside the certbot container after every
# successful issuance/renewal. The alo services (SMTP/IMAP/Caddy) run as a
# non-root user, so they cannot read certbot's default root-only cert store.
# This grants the services' group (uid/gid 999 by default) read access —
# WITHOUT making the private key world-readable: the live/archive directories
# become group-traversable (0750) and the private key group-readable (0640),
# owned root:<gid>, so only root and the service group can read it.
set -e
GID="${ALO_GID:-10001}"
for d in /etc/letsencrypt/live /etc/letsencrypt/archive; do
	[ -d "$d" ] || continue
	chgrp -R "$GID" "$d"
	chmod 0750 "$d"
	find "$d" -mindepth 1 -type d -exec chmod 0750 {} +
	find "$d" -type f -name 'privkey*.pem' -exec chmod 0640 {} +
done
echo "certbot deploy hook: cert made readable by service group ${GID}"
