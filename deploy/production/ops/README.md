# alo production ops layer (backups + monitoring)

These are the host-level operational pieces that sit **around** the Docker
stack: encrypted backups and health-and-alerting monitoring. They are plain
files (a couple of scripts + systemd units) so the whole setup is reproducible
and reviewable, not click-configured on one box.

Everything here is designed for the **single-server** deployment in the parent
directory. The day-to-day meaning of each piece — and every recovery
procedure — is in [`docs/operations-runbook.md`](../../../docs/operations-runbook.md).

## What's here

| File | Role |
|---|---|
| `backup.sh` | Nightly encrypted backup (DB + message blobs + TLS certs + config/DKIM) via restic. |
| `monitor.py` | Health checks (services, disk, memory, cert expiry, backup freshness, failed-login bursts) that email an alert when something is wrong. De-duplicates so it won't spam. |
| `send-alert.sh` | One-shot alert sender used by the backup-failure unit. |
| `monitor.env.example` | Template for the monitor config. Copy to `/root/.config/alo/monitor.env` (0600) and fill in real values. **Never commit the real file — it holds a password.** |
| `systemd/alo-backup.{service,timer}` | Runs `backup.sh` daily at 03:30 (catches up if the server was off). |
| `systemd/alo-backup-failed.service` | `OnFailure` hook that emails you if a backup fails. |
| `systemd/alo-monitor.{service,timer}` | Runs `monitor.py` every 10 minutes. |
| `systemd/alo-campaign-egress.service` | Rewrites the source address of campaign mail to the campaign IP (ADR 0044 §1). Only needed on a host with a second address; see below. |

## Install on the server (one time)

```sh
# 1. scripts
install -D -m700 backup.sh      /opt/alo/backups/backup.sh
install -D -m700 monitor.py     /opt/alo/monitoring/monitor.py
install -D -m700 send-alert.sh  /opt/alo/monitoring/send-alert.sh

# 2. config (fill in real values, keep 0600)
install -D -m600 monitor.env.example /root/.config/alo/monitor.env
$EDITOR /root/.config/alo/monitor.env

# 3. restic repo + password (password: generate once, store OFF the server too)
mkdir -p /opt/alo/backups
openssl rand -base64 24 > /root/.config/alo/restic-password
chmod 600 /root/.config/alo/restic-password
RESTIC_PASSWORD_FILE=/root/.config/alo/restic-password \
  restic -r /opt/alo/backups/restic init

# 4. systemd units
cp systemd/*.service systemd/*.timer /etc/systemd/system/
systemctl daemon-reload
systemctl enable --now alo-backup.timer alo-monitor.timer

# 5. prove alerting works — you should get an email
python3 /opt/alo/monitoring/monitor.py --test
```

## The campaign sending identity's egress (only with a second IP)

ADR 0044 §1: bulk mail leaves by a different address from transactional mail, so
a marketing reputation can never reach the domain carrying invoices and password
resets. A container cannot bind one of the host's public addresses — it has its
own network namespace — so `alo-smtp` binds a private address on the compose
`egress` network and the host rewrites the source on the way out.

```sh
# The public address campaign mail must appear to come from. It must already be
# held by the host (`ip -4 addr show`) and have forward-confirmed reverse DNS.
echo 'CAMPAIGN_IP=159.195.89.28' > /etc/default/alo-campaign-egress
chmod 600 /etc/default/alo-campaign-egress

cp systemd/alo-campaign-egress.service /etc/systemd/system/
systemctl daemon-reload
systemctl enable --now alo-campaign-egress.service

# Tell alo-smtp which sending domain uses it, then recreate the service.
#   ALO_SMTP_EGRESS_IPS=news.example.com=172.19.0.10   (in .env)
docker compose up -d alo-smtp
```

**Docker's own masquerade rule must be off for that bridge, and the compose file
turns it off.** When Docker creates a bridge network it inserts a MASQUERADE
rule at the *top* of nat `POSTROUTING`, above the SNAT rule this unit adds — so
the mail leaves by the primary address while every log line correctly says the
source was pinned. It cost an hour to find, and the only thing that named it was
the receiver's own refusal (`SPF fail … ip=152.53.179.142`). The bridge carries
`com.docker.network.bridge.enable_ip_masquerade: "false"` so there is no
competing rule to be ordered against. If you ever see the chain grow a
`-s 172.19.0.0/24 … -j MASQUERADE` line, that option has been lost.

**Prove it rather than assume it** — the whole point is an address a receiver
checks:

```sh
docker compose exec alo-smtp sh -c 'curl -s --interface 172.19.0.10 https://ifconfig.me; echo'   # the campaign IP
docker compose exec alo-smtp sh -c 'curl -s https://ifconfig.me; echo'                            # the primary IP
```

Then send one real message from the campaign domain and read
`Authentication-Results` at the far end. `spf=pass` is the only evidence that
the address, the SPF record and this rule agree; a rule that is present but
wrong looks identical from the server.

## Not included here (needs an external account — see the runbook)

- **Off-server backup copy** — a second restic destination (object storage /
  SFTP) so a total server loss can't destroy the backups. Wire it as a
  `restic copy` step appended to `backup.sh` once the destination exists.
- **External uptime check** — a monitor *outside* this server, so you're
  alerted even if the whole box is down (the internal monitor can't email if
  the server is off).
