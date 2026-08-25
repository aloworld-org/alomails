#!/usr/bin/env bash
# Pull the encrypted backup repository off the server, onto this machine.
#
# WHY A PULL. The server's own backup lands on the same filesystem as the data
# it protects, so it survives mistakes and not the loss of the disk. The proper
# answer is a second location the *server* pushes to — see OFFSITE_REPOSITORY
# in backup.env.example — but that needs storage somebody has paid for. Until
# then, an operator machine pulling nightly is a real second location and costs
# nothing.
#
# WHY PULL RATHER THAN PUSH. The server cannot reach an operator's machine: it
# is behind NAT with no inbound route, and giving a mail server credentials to
# reach a laptop would be the wrong direction of trust anyway. The machine that
# wants the copy is the one that asks for it.
#
# WHAT IT DOES NOT DO. It never holds the repository password. The bytes here
# are ciphertext and stay that way; the key belongs in a password manager. A
# key kept beside the ciphertext is not encryption, it is a filename.
set -euo pipefail

SERVER=${SERVER:-root@152.53.179.142}
SSH_KEY=${SSH_KEY:-$HOME/.ssh/ficina_deploy}
REMOTE_REPO=${REMOTE_REPO:-/opt/alo/backups/restic}
DEST=${DEST:-/c/alo-offsite}

STAGING="$DEST/.incoming"
CURRENT="$DEST/restic"
PREVIOUS="$DEST/.previous"

mkdir -p "$DEST"
rm -rf "$STAGING"
mkdir -p "$STAGING"

echo "pull-offsite: fetching $REMOTE_REPO from $SERVER"

# Streamed, so it is one connection and one pass rather than a file at a time.
ssh -i "$SSH_KEY" -o BatchMode=yes -o StrictHostKeyChecking=no "$SERVER" \
	"tar cf - -C $(dirname "$REMOTE_REPO") $(basename "$REMOTE_REPO")" \
	| tar xf - -C "$STAGING"

# A restic repository without these is not one, and a truncated transfer that
# still exits zero is exactly what a backup copy must never accept quietly.
for required in config keys data index; do
	if [ ! -e "$STAGING/restic/$required" ]; then
		echo "pull-offsite: incomplete transfer — '$required' is missing; keeping the previous copy" >&2
		rm -rf "$STAGING"
		exit 1
	fi
done

REMOTE_COUNT=$(ssh -i "$SSH_KEY" -o BatchMode=yes -o StrictHostKeyChecking=no "$SERVER" \
	"find $REMOTE_REPO -type f | wc -l")
LOCAL_COUNT=$(find "$STAGING/restic" -type f | wc -l)
if [ "$REMOTE_COUNT" -ne "$LOCAL_COUNT" ]; then
	echo "pull-offsite: got $LOCAL_COUNT files, server has $REMOTE_COUNT — keeping the previous copy" >&2
	rm -rf "$STAGING"
	exit 1
fi

# Swap in only once the new copy is known good, and keep the old one until the
# swap has succeeded: a failed pull must never be able to leave this machine
# with no usable copy at all.
rm -rf "$PREVIOUS"
[ -d "$CURRENT" ] && mv "$CURRENT" "$PREVIOUS"
mv "$STAGING/restic" "$CURRENT"
rm -rf "$STAGING" "$PREVIOUS"

SIZE=$(du -sh "$CURRENT" | cut -f1)
echo "pull-offsite: $LOCAL_COUNT files, $SIZE, verified against the server"
echo "pull-offsite: the password is NOT stored here — it lives in a password manager"
date -u '+pull-offsite: completed %Y-%m-%dT%H:%M:%SZ' > "$DEST/last-pull.txt"
