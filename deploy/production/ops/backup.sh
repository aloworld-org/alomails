#!/usr/bin/env bash
# alo encrypted backup: database + message blobs + TLS certs + config/DKIM.
#
# Uses restic (encrypted, deduplicated). The repository password lives in
# /root/.config/alo/restic-password (0600, NEVER in the repo). Installed to
# /opt/alo/backups/backup.sh and run daily by alo-backup.timer.
#
# Restore is documented in docs/operations-runbook.md ("Restore from backup").
set -euo pipefail
export RESTIC_REPOSITORY=/opt/alo/backups/restic
export RESTIC_PASSWORD_FILE=/root/.config/alo/restic-password
STAGING=/opt/alo/backups/staging
cd /opt/alo/deploy/production
mkdir -p "$STAGING"

# 1. Database (custom compressed dump).
#
# The role and database names are read from the deployment rather than written
# here. They are not the same everywhere — a developer machine calls the
# database `alo` and this deployment calls it `ficina` — and a backup script
# that hardcodes either one fails on the other. It failed silently for
# twenty-five days that way (2026-08-01 to 08-25): every night, `role "alo"
# does not exist`, and the alarm that should have said so was broken by the
# same migration.
# Asked of the running container rather than parsed out of .env: it is the
# same question with one fewer thing to get wrong, and the answer is what the
# database is actually using rather than what a file says it should be.
PGUSER=$(docker compose exec -T postgres printenv POSTGRES_USER | tr -d '\r\n')
PGDB=$(docker compose exec -T postgres printenv POSTGRES_DB | tr -d '\r\n')
docker compose exec -T postgres pg_dump -U "$PGUSER" -d "$PGDB" -Fc > "$STAGING/alo-db.dump"

# 2. Docker volume paths (message bodies + TLS certs).
BLOBS=$(docker volume inspect ficina_blobs -f "{{.Mountpoint}}")
CERTS=$(docker volume inspect ficina_certs -f "{{.Mountpoint}}")

# 3. Encrypted, deduplicated backup of everything that matters.
restic backup --tag alo \
  "$STAGING/alo-db.dump" \
  "$BLOBS" \
  "$CERTS" \
  /opt/alo/deploy/production

# 4. Retention: daily for a week, weekly for a month; prune the rest.
restic forget --tag alo --keep-daily 7 --keep-weekly 4 --prune

# 5. Off-site copy.
#
# THE REPOSITORY ABOVE IS ON THE SAME FILESYSTEM AS THE DATA IT PROTECTS. It
# defends against a bad migration, a wrong DELETE, a broken deploy — every
# failure that leaves the machine standing. It defends against nothing that
# takes the disk with it, because then the data and every copy of it go
# together. `docs/alo-product-description.md` promises customers replication
# across two EU locations; one filesystem is not two locations.
#
# Set OFFSITE_REPOSITORY (and OFFSITE_PASSWORD_FILE, if it differs) in
# /root/.config/alo/backup.env and this pushes there after every run. restic
# copy sends only what the destination lacks, so a nightly run costs the day's
# changes rather than the whole repository.
#
# Unset, it says so and exits clean rather than failing: a deployment that has
# not chosen a destination yet is not a broken backup, and a nightly red alert
# nobody can act on is how real alerts get ignored. The monitor reports the
# absence separately.
[ -f /root/.config/alo/backup.env ] && . /root/.config/alo/backup.env

if [ -n "${OFFSITE_REPOSITORY:-}" ]; then
	echo "backup: copying to the off-site repository"
	restic copy --tag alo \
		--repo2 "$OFFSITE_REPOSITORY" \
		--password-file2 "${OFFSITE_PASSWORD_FILE:-$RESTIC_PASSWORD_FILE}"
	echo "backup: off-site copy complete"
else
	echo "backup: no OFFSITE_REPOSITORY set — this backup exists only on the machine it protects"
fi
