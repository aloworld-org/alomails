#!/usr/bin/env bash
# Restart the TLS-terminating services when their certificate has been renewed.
#
# WHY THIS EXISTS. certbot renews inside its own container and has no access to
# the Docker socket — deliberately, since that socket is root on the host — so
# it cannot restart the services that read the certificate. Its only renewal
# hook fixes file permissions. Nothing reloads anything.
#
# The consequence is quiet and total: a certificate renews on disk, every
# service keeps presenting the old one, and on the day it expires the whole
# deployment stops at once — web, IMAP, submission and MAPI together. It was
# invisible here only because frequent deploys happened to restart things
# often enough (found 2026-08-25, when a reissued certificate reached 993 and
# 465 but not 443).
#
# WHY COMPARE TIMESTAMPS rather than keep a record of the last certificate
# seen. A state file is one more thing that can be wrong, deleted, or disagree
# with reality. The container's own start time and the certificate's mtime are
# both facts the system already maintains, and "the certificate is newer than
# the process reading it" is exactly the condition that needs fixing —
# self-correcting even if this script has never run before.
#
# WHY caddy is restarted, not reloaded. `caddy reload` re-reads the Caddyfile,
# not the certificate files it already holds; a reloaded Caddy went on serving
# the old certificate while IMAP and submission had picked up the new one.
set -euo pipefail

COMPOSE_DIR=${COMPOSE_DIR:-/opt/alo/deploy/production}
CERT_NAME=${CERT_NAME:-mail.alomails.com}
# The services that read the certificate at startup. Caddy terminates TLS for
# every web surface; the other two for IMAPS/POP3S and submission.
SERVICES=${SERVICES:-"caddy alo-imap alo-smtp"}

cd "$COMPOSE_DIR"

CERTS_VOLUME=$(docker volume inspect ficina_certs -f '{{.Mountpoint}}' 2>/dev/null || true)
if [ -z "$CERTS_VOLUME" ]; then
	echo "cert-reload: cannot find the certificate volume; nothing to do" >&2
	exit 1
fi

CERT="$CERTS_VOLUME/live/$CERT_NAME/fullchain.pem"
if [ ! -f "$CERT" ]; then
	echo "cert-reload: no certificate at $CERT" >&2
	exit 1
fi

# Follow the symlink: live/ points into archive/, and it is the real file whose
# mtime moves when a certificate is renewed.
CERT_MTIME=$(stat -Lc %Y "$CERT")

restarted=0
for svc in $SERVICES; do
	container=$(docker compose ps -q "$svc" 2>/dev/null || true)
	if [ -z "$container" ]; then
		echo "cert-reload: $svc is not running; skipping"
		continue
	fi

	started_at=$(docker inspect -f '{{.State.StartedAt}}' "$container")
	started_epoch=$(date -d "$started_at" +%s 2>/dev/null || echo 0)

	if [ "$CERT_MTIME" -gt "$started_epoch" ]; then
		echo "cert-reload: $CERT_NAME is newer than $svc; restarting it"
		# One at a time, so a failure to come back is visible before the next
		# service is taken down with it.
		docker compose restart "$svc"
		restarted=$((restarted + 1))
	fi
done

if [ "$restarted" -eq 0 ]; then
	echo "cert-reload: every service is already newer than the certificate"
else
	echo "cert-reload: restarted $restarted service(s) onto the renewed certificate"
fi
