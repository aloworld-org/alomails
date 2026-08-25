#!/usr/bin/env python3
"""alo production monitor.

Runs on a timer. Checks the health of the whole stack and emails an alert
(via the server's own authenticated submission, so no third-party account)
when something is wrong. Alerts are de-duplicated: a given problem is sent
once when it appears and re-sent at most every RE_ALERT_HOURS while it
persists, plus a single "recovered" note when it clears.

No message bodies, credentials, or personal data are ever included in an
alert — only service names, counts, percentages and dates.
"""
import json
import os
import smtplib
import ssl
import subprocess
import sys
import time
from datetime import datetime, timezone
from email.message import EmailMessage
from pathlib import Path

STATE_DIR = Path("/var/lib/alo-monitor")
STATE_FILE = STATE_DIR / "state.json"
COMPOSE_DIR = "/opt/alo/deploy/production"
RE_ALERT_HOURS = 6


def load_env(path="/root/.config/alo/monitor.env"):
    env = {}
    with open(path) as fh:
        for line in fh:
            line = line.strip()
            if not line or line.startswith("#") or "=" not in line:
                continue
            k, v = line.split("=", 1)
            env[k.strip()] = v.strip()
    return env


ENV = load_env()


def run(cmd, timeout=30):
    """Run a command, return (rc, stdout, stderr)."""
    p = subprocess.run(
        cmd, capture_output=True, text=True, timeout=timeout
    )
    return p.returncode, p.stdout, p.stderr


# ---- individual checks: each returns a list of (key, message) problems ----

def check_services():
    problems = []
    # `--all`, because without it compose lists only *running* services: a
    # container that has stopped or crashed is simply absent from the output,
    # the loop below never sees it, and the monitor reports everything fine.
    # That is the most ordinary failure there is, and it was invisible here
    # until somebody stopped a container on purpose to check (2026-08-25).
    # Running-but-unhealthy was caught; dead was not.
    rc, out, err = run(
        ["docker", "compose", "ps", "--all", "--format", "json"], timeout=30
    )
    if rc != 0:
        return [("docker", "docker compose ps failed — cannot read service health")]
    # compose emits either a JSON array or one JSON object per line
    services = []
    out = out.strip()
    if out.startswith("["):
        services = json.loads(out)
    else:
        for line in out.splitlines():
            line = line.strip()
            if line:
                services.append(json.loads(line))
    if not services:
        return [("docker", "no services are running — the stack appears to be down")]
    for s in services:
        name = s.get("Service") or s.get("Name", "?")
        state = (s.get("State") or "").lower()
        health = (s.get("Health") or "").lower()
        if state != "running":
            problems.append((f"svc:{name}", f"service '{name}' is '{state}', expected 'running'"))
        elif health and health not in ("healthy", ""):
            problems.append((f"svc:{name}", f"service '{name}' health is '{health}'"))
    return problems


def check_disk():
    problems = []
    threshold = int(ENV.get("DISK_PCT", "80"))
    rc, out, err = run(["df", "-P", "/"], timeout=15)
    if rc != 0:
        return [("disk", "could not read disk usage")]
    lines = out.strip().splitlines()
    if len(lines) >= 2:
        pct = int(lines[1].split()[4].rstrip("%"))
        if pct >= threshold:
            problems.append(("disk", f"root disk is {pct}% full (threshold {threshold}%)"))
    return problems


def check_memory():
    problems = []
    try:
        meminfo = {}
        for line in Path("/proc/meminfo").read_text().splitlines():
            k, _, v = line.partition(":")
            meminfo[k] = int(v.strip().split()[0])  # kB
        total = meminfo["MemTotal"]
        avail = meminfo["MemAvailable"]
        used_pct = round((total - avail) / total * 100)
        if used_pct >= 92:
            problems.append(("memory", f"memory is {used_pct}% used"))
    except Exception as exc:  # noqa: BLE001 - monitor must never crash
        problems.append(("memory", f"could not read memory: {exc}"))
    return problems


def check_cert():
    problems = []
    days = int(ENV.get("CERT_DAYS", "14"))
    domain = ENV.get("ALERT_HOST", "")
    rc, out, err = run(
        ["docker", "volume", "inspect", "ficina_certs", "-f", "{{.Mountpoint}}"],
        timeout=15,
    )
    if rc != 0:
        return [("cert", "could not locate the TLS certificate volume")]
    cert = Path(out.strip()) / "live" / domain / "cert.pem"
    if not cert.exists():
        return [("cert", f"TLS certificate not found at {cert}")]
    rc, out, err = run(
        ["openssl", "x509", "-enddate", "-noout", "-in", str(cert)], timeout=15
    )
    if rc != 0:
        return [("cert", "could not read the TLS certificate expiry")]
    # notAfter=Jul 28 12:00:00 2026 GMT
    exp = out.strip().split("=", 1)[1]
    exp_dt = datetime.strptime(exp, "%b %d %H:%M:%S %Y %Z").replace(tzinfo=timezone.utc)
    remaining = (exp_dt - datetime.now(timezone.utc)).days
    if remaining <= days:
        problems.append(("cert", f"TLS certificate expires in {remaining} days ({exp})"))
    return problems


def check_backup():
    problems = []
    max_hours = int(ENV.get("BACKUP_MAX_HOURS", "26"))
    env = dict(os.environ)
    env["RESTIC_REPOSITORY"] = "/opt/alo/backups/restic"
    env["RESTIC_PASSWORD_FILE"] = "/root/.config/alo/restic-password"
    p = subprocess.run(
        ["restic", "snapshots", "--json", "--latest", "1", "--tag", "alo"],
        capture_output=True, text=True, env=env, timeout=60,
    )
    if p.returncode != 0:
        return [("backup", "could not read restic snapshots — backup repo may be unreachable")]
    try:
        snaps = json.loads(p.stdout or "[]")
    except json.JSONDecodeError:
        return [("backup", "restic returned unreadable snapshot data")]
    if not snaps:
        return [("backup", "no backup snapshots exist yet")]
    ts = snaps[-1]["time"][:19]
    snap_dt = datetime.strptime(ts, "%Y-%m-%dT%H:%M:%S").replace(tzinfo=timezone.utc)
    age_h = (datetime.now(timezone.utc) - snap_dt).total_seconds() / 3600
    if age_h > max_hours:
        problems.append(("backup", f"latest backup is {round(age_h)}h old (expected under {max_hours}h)"))
    return problems


def check_failed_logins():
    problems = []
    threshold = int(ENV.get("FAILED_LOGIN_THRESHOLD", "15"))
    window = int(ENV.get("FAILED_LOGIN_WINDOW_MIN", "15"))
    patterns = ("authentication failed", "AUTHENTICATIONFAILED",
                "auth failed", "invalid credentials", "login failed")
    total = 0
    for svc in ("alo-smtp", "alo-imap", "alo-jmap"):
        rc, out, err = run(
            ["docker", "compose", "logs", "--since", f"{window}m", "--no-log-prefix", svc],
            timeout=30,
        )
        blob = (out + err).lower()
        total += sum(blob.count(p.lower()) for p in patterns)
    if total >= threshold:
        problems.append(("auth", f"{total} failed logins in the last {window} min (threshold {threshold}) — possible brute-force"))
    return problems


CHECKS = [
    check_services, check_disk, check_memory,
    check_cert, check_backup, check_failed_logins,
]


def gather_problems():
    problems = {}
    for check in CHECKS:
        try:
            for key, msg in check():
                problems[key] = msg
        except Exception as exc:  # noqa: BLE001 - a broken check must not silence the rest
            problems[f"monitor:{check.__name__}"] = f"monitor check '{check.__name__}' crashed: {exc}"
    return problems


def load_state():
    if STATE_FILE.exists():
        try:
            return json.loads(STATE_FILE.read_text())
        except json.JSONDecodeError:
            return {}
    return {}


def save_state(state):
    STATE_DIR.mkdir(parents=True, exist_ok=True)
    STATE_FILE.write_text(json.dumps(state, indent=2))


def send_email(subject, body):
    msg = EmailMessage()
    msg["From"] = ENV["ALERT_FROM"]
    msg["To"] = ENV["ALERT_TO"]
    msg["Subject"] = subject
    msg.set_content(body)
    ctx = ssl.create_default_context()
    with smtplib.SMTP_SSL(ENV["ALERT_HOST"], 465, context=ctx, timeout=30) as s:
        s.login(ENV["ALERT_USER"], ENV["ALERT_PASS"])
        s.send_message(msg)


def now_ts():
    return int(time.time())


def main():
    force_test = "--test" in sys.argv
    if force_test:
        send_email(
            "[alo] test alert — monitoring is working",
            "This is a harmless test alert from your alo server monitor.\n\n"
            "If you are reading this in your inbox, alerting works: you will be\n"
            "emailed here if a service goes down, the disk fills up, the TLS\n"
            "certificate is about to expire, a backup is missed, or there is a\n"
            "burst of failed logins.\n\n"
            f"Sent {datetime.now(timezone.utc):%Y-%m-%d %H:%M UTC} from "
            f"{ENV.get('ALERT_HOST')}.\n",
        )
        print("test alert sent")
        return 0

    problems = gather_problems()
    state = load_state()
    known = state.get("problems", {})
    now = now_ts()
    to_send = []

    # new or due-for-re-alert problems
    for key, msg in problems.items():
        prev = known.get(key)
        if prev is None:
            to_send.append(("NEW", msg))
            known[key] = {"msg": msg, "first": now, "last_alert": now}
        else:
            if now - prev.get("last_alert", 0) >= RE_ALERT_HOURS * 3600:
                to_send.append(("STILL", msg))
                prev["last_alert"] = now
            prev["msg"] = msg

    # recovered problems
    recovered = [known[k]["msg"] for k in list(known) if k not in problems]
    for k in list(known):
        if k not in problems:
            del known[k]

    state["problems"] = known
    state["last_run"] = now
    save_state(state)

    if to_send or recovered:
        lines = []
        if to_send:
            lines.append("PROBLEMS DETECTED on your alo server:\n")
            for tag, msg in to_send:
                lines.append(f"  [{tag}] {msg}")
        if recovered:
            lines.append("\nRECOVERED (no longer a problem):\n")
            for msg in recovered:
                lines.append(f"  [OK] {msg}")
        lines.append(f"\nChecked {datetime.now(timezone.utc):%Y-%m-%d %H:%M UTC} on {ENV.get('ALERT_HOST')}.")
        n = len(to_send)
        subject = (
            f"[alo] {n} problem{'s' if n != 1 else ''} detected"
            if to_send else "[alo] server recovered"
        )
        try:
            send_email(subject, "\n".join(lines))
            print(f"alert sent: {n} problem(s), {len(recovered)} recovered")
        except Exception as exc:  # noqa: BLE001
            print(f"FAILED to send alert: {exc}", file=sys.stderr)
            return 1
    else:
        print("all checks passed, no alert")
    return 0


if __name__ == "__main__":
    sys.exit(main())
