#!/usr/bin/env bash
# Prove the backup can actually be restored — automatically, on a schedule.
#
# WHY. `docs/alo-product-description.md` promises customers that "restores are
# rehearsed monthly by script — an untested backup is a hope, not a backup".
# Until 2026-08-25 the rehearsal was neither scripted nor monthly, and the
# nightly backup had been failing for twenty-five days without anybody knowing.
# A rehearsal that runs by itself is what turns that from a thing somebody
# remembers to check into a thing that reports itself.
#
# It is deliberately more than "did restic exit zero". It takes the newest
# snapshot, decrypts it, restores the dump into a throwaway database and counts
# what came back. A backup that runs nightly and cannot be restored is the
# failure this is built to catch, and only a real restore catches it.
#
# SAFETY. It never touches the live database. The throwaway is created under a
# name of its own, dropped at the end, and this script refuses to run at all if
# that name ever resolves to the database the product uses.
set -euo pipefail

COMPOSE_DIR=${COMPOSE_DIR:-/opt/alo/deploy/production}
export RESTIC_REPOSITORY=${RESTIC_REPOSITORY:-/opt/alo/backups/restic}
export RESTIC_PASSWORD_FILE=${RESTIC_PASSWORD_FILE:-/root/.config/alo/restic-password}
WORK=$(mktemp -d /tmp/alo-restore-rehearsal.XXXXXX)
REHEARSAL_DB=${REHEARSAL_DB:-alo_restore_rehearsal}

cd "$COMPOSE_DIR"

# The names the running stack uses, asked of the container rather than assumed:
# a developer machine calls the database `alo` and this deployment `ficina`.
PGUSER=$(docker compose exec -T postgres printenv POSTGRES_USER | tr -d '\r\n')
PGDB=$(docker compose exec -T postgres printenv POSTGRES_DB | tr -d '\r\n')

if [ "$REHEARSAL_DB" = "$PGDB" ]; then
	echo "restore-rehearsal: refusing — the rehearsal database is the live one" >&2
	exit 1
fi

cleanup() {
	rm -rf "$WORK"
	docker compose exec -T postgres psql -U "$PGUSER" -d postgres \
		-c "DROP DATABASE IF EXISTS $REHEARSAL_DB;" >/dev/null 2>&1 || true
	docker compose exec -T postgres rm -f /tmp/rehearsal.dump >/dev/null 2>&1 || true
}
trap cleanup EXIT

# 1. Is there a recent backup at all? A rehearsal against a stale snapshot
#    passes while the nightly job is silently broken — exactly what happened.
LATEST_EPOCH=$(restic snapshots --tag alo --json 2>/dev/null \
	| python3 -c 'import sys,json,datetime
s=json.load(sys.stdin)
if not s: print(0)
else: print(int(datetime.datetime.fromisoformat(sorted(s,key=lambda x:x["time"])[-1]["time"]).timestamp()))')
if [ "$LATEST_EPOCH" -eq 0 ]; then
	echo "restore-rehearsal: no snapshots in the repository" >&2
	exit 1
fi
AGE_HOURS=$((($(date +%s) - LATEST_EPOCH) / 3600))
if [ "$AGE_HOURS" -gt "${MAX_SNAPSHOT_AGE_HOURS:-48}" ]; then
	echo "restore-rehearsal: newest snapshot is ${AGE_HOURS}h old — the nightly backup is not running" >&2
	exit 1
fi

# 2. Decrypt and restore the dump out of the repository.
restic restore latest --target "$WORK" --include "/opt/alo/backups/staging/alo-db.dump" >/dev/null
DUMP="$WORK/opt/alo/backups/staging/alo-db.dump"
[ -s "$DUMP" ] || { echo "restore-rehearsal: no database dump in the snapshot" >&2; exit 1; }

# 3. Restore it into a throwaway database.
docker compose cp "$DUMP" postgres:/tmp/rehearsal.dump >/dev/null
docker compose exec -T postgres psql -U "$PGUSER" -d postgres \
	-c "DROP DATABASE IF EXISTS $REHEARSAL_DB;" >/dev/null
docker compose exec -T postgres psql -U "$PGUSER" -d postgres \
	-c "CREATE DATABASE $REHEARSAL_DB;" >/dev/null
docker compose exec -T postgres pg_restore -U "$PGUSER" -d "$REHEARSAL_DB" \
	--no-owner /tmp/rehearsal.dump >/dev/null 2>&1 || true

# 4. Count what came back. An empty restore "succeeds" at every earlier step,
#    so the numbers are the only honest evidence.
count() {
	docker compose exec -T postgres psql -U "$PGUSER" -d "$1" -tAc "select count(*) from $2;" 2>/dev/null | tr -d '\r\n '
}
for table in users tenants messages; do
	restored=$(count "$REHEARSAL_DB" "$table")
	live=$(count "$PGDB" "$table")
	if [ -z "$restored" ]; then
		echo "restore-rehearsal: table '$table' is missing from the restored copy" >&2
		exit 1
	fi
	if [ "$restored" -eq 0 ] && [ "${live:-0}" -gt 0 ]; then
		echo "restore-rehearsal: '$table' restored empty while live holds $live" >&2
		exit 1
	fi
	echo "restore-rehearsal: $table — restored $restored, live $live"
done

echo "restore-rehearsal: a ${AGE_HOURS}h-old backup restored and read back cleanly"
