# alo — single-server production deployment

This runs the **complete first-class mail path** on one server and one
database: **receive** mail, **read** it from any mail app, **send**, and
**log in** securely. It is a real deployment, not a demo.

**Honest scope:** the day-one inbox is a **mail app (Thunderbird, Apple
Mail, a phone)** over IMAP — there is **no browser webmail yet** (that is a
separate Phase-2 build). Everything needed for a mail app to work is here.

## What runs

| Service | Purpose | Ports |
|---|---|---|
| `alo-smtp` | receive (25) + authenticated send (587/465) + outbound + spam/trust stack | 25, 587, 465 |
| `alo-imap` | read mail from a mail app | 993 (IMAPS), 995 (POP3S) — 143 closed |
| `alo-jmap` | native API **and** the OpenID Connect login provider (behind Caddy) | internal 8080 |
| `caddy` | automatic Let's Encrypt HTTPS for the login/API origin | 80, 443 |
| `postgres` | system of record | internal |
| `rspamd` | spam scoring at receive time | internal |

Message bodies are stored on a shared on-disk volume all three alo
services mount (single-node; multi-node would swap in Garage/S3).

## Prerequisites

- A Linux server with Docker + the compose plugin, ports 25/80/443/465/587/
  993/995 reachable from the internet. (Port 143, cleartext-then-STARTTLS
  IMAP, is deliberately not published — 993 covers IMAP securely.)
- DNS for your domain (`DOMAIN`, e.g. `mail.example.com`):
  - `A`/`AAAA` `mail.example.com` → this server,
  - `MX` for the mail domain → `mail.example.com`,
  - `PTR` (reverse DNS) for the server IP → `mail.example.com`. **This is set
    at your hosting/IP provider, not in the domain's DNS zone** — many hosts
    (Hetzner, OVH, DigitalOcean) expose it as a "reverse DNS"/"rDNS" field on
    the server or floating IP. Gmail and Outlook reject or spam-file mail from
    an IP whose PTR doesn't match the sending host, so this is not optional in
    practice. Verify it after setup on the admin **Security & trust** page,
    which flags a missing or mismatched PTR.
  - `A` `mta-sts.mail.example.com` → this server (for the MTA-STS policy),
  - SPF/DMARC as you prefer; **DKIM is generated below**. The **Security &
    trust** page runs live SPF/DKIM/DMARC/MX/PTR/MTA-STS checks against the
    email domain so you can confirm each record end-to-end after deploy.

## Setup

```sh
cd deploy/production
cp .env.example .env
# Edit .env: DOMAIN, ALO_SMTP_LOCAL_DOMAINS, ACME_EMAIL.
./generate-secrets.sh          # fills POSTGRES_PASSWORD with fresh randomness
./generate-dkim.sh             # writes dkim/dkim.key and PRINTS the DNS record
# → add the printed TXT record at fic._domainkey.<your-domain>, then continue.

# DNS for DOMAIN and mta-sts.DOMAIN must already resolve to this server.
./init-certs.sh                # obtains the Let's Encrypt cert (once)
docker compose up -d --build   # build the images and start the stack
```

Watch it come up and become healthy:

```sh
docker compose ps              # every service should read "healthy"
docker compose logs -f caddy   # first boot: Caddy obtains the Let's Encrypt cert
```

Create your first admin mailbox (password read from the environment, never
the command line):

```sh
docker compose exec -e ALO_ADMIN_PASSWORD='a-strong-password' \
  alo-jmap identityctl bootstrap-admin your-org you@your-domain.com
```

## Connect a mail app (the day-one inbox)

In Thunderbird / Apple Mail / your phone:

- **Incoming (IMAP):** server `mail.example.com`, port **993**, SSL/TLS,
  username = your full email, password = the one you just set.
- **Outgoing (SMTP):** server `mail.example.com`, port **465** (SSL/TLS) or
  **587** (STARTTLS), same username/password.

Send yourself a message and reply to it to confirm the full loop.

### One-step setup (autoconfig) — optional but recommended

So users can add an account by typing only their email address (no server
names or ports), alo serves the standard discovery documents from `alo-jmap`:

- **Mozilla autoconfig** (Thunderbird, Apple Mail): `GET
  /.well-known/autoconfig/mail/config-v1.1.xml` and `/mail/config-v1.1.xml`.
- **Microsoft Autodiscover** (Outlook): `GET`/`POST
  /autodiscover/autodiscover.xml`.

Clients look for these under the **email domain** (the part after `@`), not
the server FQDN, so wire the email domain to this server:

1. **DNS** (at the email domain `example.com`, where mailboxes live):
   - `CNAME autoconfig.example.com → mail.example.com`
   - `CNAME autodiscover.example.com → mail.example.com`
   - (optional, Thunderbird's second probe) an `A`/`CNAME` for the bare
     `example.com` → this server, if it isn't already pointed elsewhere.
2. **Caddy**: add site blocks for those names that reverse-proxy to
   `alo-jmap:8080` (a one-line `reverse_proxy`, same as the `@backend` block
   for the main host). Each name needs a certificate; add it to the certbot
   list or let Caddy's on-demand TLS obtain it.

Verify directly against the server origin before wiring DNS:

```sh
curl "https://mail.example.com/.well-known/autoconfig/mail/config-v1.1.xml?emailaddress=you@example.com"
curl -X POST https://mail.example.com/autodiscover/autodiscover.xml \
  -d '<Autodiscover><Request><EMailAddress>you@example.com</EMailAddress></Request></Autodiscover>'
```

The documents advertise IMAPS `993` and SMTPS `465` on the server FQDN with
password auth inside TLS — the same settings as the manual steps above. They
expose no secrets and need no authentication (the client has none yet).

## The login provider (OIDC)

Once up, the OpenID Connect endpoints are live at `https://<DOMAIN>`:
`/.well-known/openid-configuration`, `/oauth/authorize`, `/oauth/token`,
`/oauth/userinfo`, `/oauth/jwks`. Register a first-party app (e.g. a future
webmail) with:

```sh
docker compose exec alo-jmap \
  identityctl register-client web "alo Web" https://<DOMAIN>/callback
```

## TLS certificates

A dedicated `certbot` service obtains and renews **one** Let's Encrypt
certificate (for `<DOMAIN>` and `mta-sts.<DOMAIN>`) into a shared volume at
the stable path `/certs/live/<DOMAIN>/`. **Every service reads that same
path** — Caddy for HTTPS on 443, and the SMTP/IMAP services for their own
TLS on 465/587/993/995. One certificate, one location, no coupling to any
proxy's internal storage.

- **First issuance** is `./init-certs.sh`, run once before the first `up`
  (it uses certbot standalone on port 80, so DNS must already resolve to the
  server and nothing else may hold port 80 at that moment).
- **Renewal is automatic:** the `certbot` service runs `certbot renew` on a
  12-hour loop and only touches port 80 when a cert is within 30 days of
  expiry. The mail services pick up a renewed cert on their next restart, so
  a monthly `docker compose restart alo-smtp alo-imap` (or a
  reload-on-change hook) keeps them current — a small operational note, not a
  blocker.

The certificate path can only be exercised for real against a **public
domain** (Let's Encrypt will not issue for a private/local name) — see the
local test mode below for laptops.

## DKIM key size and DNS providers

`generate-dkim.sh` makes a **2048-bit RSA** key — the right default, universally
verified. Two real-world constraints to know:

- **`ring` (our crypto) refuses RSA keys below 2048 bits**, so a 1024-bit RSA
  key will fail to sign (`signing key could not be parsed`). Don't shrink the
  RSA key to fit a DNS field.
- **Some DNS UIs (e.g. Namecheap) reject a TXT value over ~255 characters**,
  even split into quoted strings — and a 2048-bit RSA DKIM record is ~400
  chars. If your DNS host can't store it, either move DNS to a provider that
  handles long TXT records (Cloudflare does, for free), **or** use an
  **Ed25519** DKIM key: set `ALO_SMTP_DKIM_ALGORITHM=ed25519`, generate with
  `openssl genpkey -algorithm ed25519 -out dkim/dkim.key`, and publish
  `v=DKIM1; k=ed25519; p=<base64 of the 32-byte public key>` (only ~60 chars).
  Ed25519 DKIM (RFC 8463) is verified by Gmail/Outlook and passes DMARC; note
  that some older spam-scoring engines (e.g. SpamAssassin) don't yet award the
  "valid DKIM" bonus for it, which can cost a point on tools like mail-tester
  without affecting real inbox delivery. For a perfect score everywhere, use
  RSA-2048 on a long-TXT-capable DNS host.

## Local test mode (no public domain)

Let's Encrypt cannot issue for a private/local name, so for a laptop smoke
test skip certbot + Caddy and let the mail services self-sign. In `.env`:

```sh
ALO_SMTP_ALLOW_SELF_SIGNED=true
ALO_IMAP_ALLOW_SELF_SIGNED=true
ALO_SMTP_TLS_CERT=
ALO_SMTP_TLS_KEY=
ALO_IMAP_TLS_CERT=
ALO_IMAP_TLS_KEY=
```

Then bring up only the core services (no cert needed):

```sh
docker compose up -d --build postgres rspamd alo-smtp alo-imap alo-jmap
```

The mail services present self-signed certs (mail apps will warn) and the
JMAP/OIDC API is reachable on its internal port — fine for a smoke test,
never for production. Do not run `init-certs.sh` locally.

## How to notice problems, and how to turn it off

- **Notice:** `docker compose ps` shows per-service health; `docker compose
  logs -f <service>` streams structured logs (no secrets are ever logged).
  A detected token replay or a failed revoke logs a `warn` a monitor can
  alert on.
- **Turn off outbound sending** (kill-switch): set
  `ALO_SMTP_OUTBOUND_ENABLED=false` in `.env` and
  `docker compose up -d alo-smtp`.
- **Stop everything:** `docker compose down` (data volumes persist);
  `docker compose down -v` also deletes the data (irreversible).

## The web app

The alo web app (the browser workspace — Mail first) is served at the same
origin as the API: Caddy serves the built SPA for normal paths and reverse-
proxies the backend paths (`/oauth/*`, `/jmap/*`, `/.well-known/*`,
`/auth/token`) to `alo-jmap`. Same origin means no CORS and a first-party
OIDC login redirect. The files live in a mounted directory (`./web`, mapped to
`/srv` in the Caddy container), so publishing is a static-file copy — no
restart.

```sh
# one time: register the web app as a public OIDC client (PKCE, no secret)
docker compose exec alo-jmap identityctl register-client \
  web "alo Web" https://<DOMAIN>/auth/callback

# build + publish (from a machine with the repo + Node):
DEPLOY_HOST=root@<DOMAIN> DEPLOY_KEY=~/.ssh/<key> ./deploy-web.sh
```

Then open `https://<DOMAIN>/` and sign in with a mailbox's email + password
(the authentication-code field appears only if the account has 2FA). The app's
architecture is in [`docs/design/web-shell.md`](../../docs/design/web-shell.md).

## Operations: backups, monitoring, and the runbook

Production hardening (encrypted nightly backups, health-and-alert monitoring,
security hardening, log rotation) lives in [`ops/`](ops/) as plain scripts and
systemd units — reproducible, not click-configured. Install steps are in
[`ops/README.md`](ops/README.md).

The plain-language day-to-day guide — how to check health, what each alert
means, how to restore from backup, how to add a mailbox, cert renewal, the
security posture, and the remaining DNS/account items — is
[`docs/operations-runbook.md`](../../docs/operations-runbook.md).

## What is deliberately NOT here

Browser **webmail** (Phase 2), and the **chat/meet/docs** engines
(Synapse/LiveKit/Collabora — separate Phase-2 products, in the dev compose
one level up). The multi-tenant / self-service hardening items in
`docs/design/security-audit-followups.md` are also deferred; none affect a
single-owner mailbox.
