# alomails

**Sovereign, self-hostable email.** SMTP, IMAP/POP3, and a native JMAP API
with a built-in OpenID Connect provider — the mail core of the
[alo](https://github.com/aloworld-us) workspace, released on its own as an
independent, auditable product.

Rust below the waterline; PostgreSQL as the system of record; pinned upstream
engines behind our own APIs. No OpenSSL, no C in the trust path — TLS and
crypto run on `rustls`/`ring`.

## What's here

| Layer | Crates |
|---|---|
| **platform** — the shared kernel | `alo-store` (tenant-scoped message store on Postgres), `alo-identity` (OIDC provider, credentials, 2FA), `alo-auth-mail` (SPF/DKIM/DMARC), `alo-sieve` (filters), `alo-ai` (optional inference + egress guard) |
| **products/mail** — the Mail product | `alo-smtp` (receive + authenticated submission + outbound), `alo-smtp-client`, `alo-imap` (IMAP4rev2 + POP3), `alo-jmap` (RFC 8620/8621 API + OIDC endpoints + signup) |

Runs the complete first-class mail path: **receive** mail, **read** it from
any mail app, **send**, and **log in** securely — with the trust stack
(SPF/DKIM/DMARC/MTA-STS) and spam scoring at the boundary.

## Run it

A single-server deployment (Postgres + rspamd + ClamAV + Caddy + the mail
services) lives in [`deploy/production/`](deploy/production/) — see its
[`README`](deploy/production/README.md). In short:

```sh
cd deploy/production
cp .env.example .env        # set DOMAIN, ALO_SMTP_LOCAL_DOMAINS, ACME_EMAIL
./generate-secrets.sh
./generate-dkim.sh          # prints the DNS record to publish
./init-certs.sh             # Let's Encrypt (once)
docker compose up -d --build
```

Then connect Thunderbird / Apple Mail / your phone over IMAP 993 + SMTP 465,
or point a JMAP client at the same origin. Mail-client autoconfiguration
(Thunderbird/Apple Mail/Outlook) is served out of the box.

### Webmail

A browser client ships in [`web/`](web/) — a JMAP web app (mail, contacts,
IMAP import, personal signup), served at the same origin as the API:

```sh
cd web && npm ci && npm run build      # build the SPA
# publish the built dist/ to the server (see deploy/production/deploy-web.sh)
```

It is the alo mail surface with the suite-only modules (Docs, Chat, Meet, and
the multi-tenant control plane) removed — Mail as a standalone product.

## Build

```sh
cargo build --workspace          # the mail services + identity + migrator
cargo test --workspace           # requires a local Postgres (see CI / deploy)
```

## Relationship to alo-workplace

alomails is the Mail **product**. The [alo](https://github.com/aloworld-us)
workspace is the **suite** that composes it with the other products (Docs,
Calendar, Chat, Meet) and the multi-tenant control plane. The shared
`platform/` kernel is developed in the workspace monorepo and mirrored here;
alomails depends on it, never forks it (see the workspace's ADR 0019).

## License

[AGPL-3.0-only](LICENSE). Hosting alomails as a service means publishing your
changes. Commercial licenses (AGPL-exit) are available from the alo team.
Outside contributions require a CLA granting relicensing rights.
