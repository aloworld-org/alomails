# deploy/ — the composed engine set

Engines run as version-pinned, unmodified upstream containers behind
alo's APIs (ADR 0003). The image tags in `docker-compose.yml` are
the single source of truth for versions; `scripts/fetch-engines.sh`
derives its source checkouts from them.

## Phase 0 working set

```
cp .env.example .env        # then edit values
docker compose up -d --wait postgres garage rspamd alo-smtp
```

These four reach `healthy` with no further steps. Verify SMTP:

```
printf 'EHLO test\r\nQUIT\r\n' | nc localhost 2525
```

## Engines that need bootstrap before they reach healthy

These are boot-ready but not yet wired to the product — that wiring is
tracked in ROADMAP.md Phase 2, which is the issue of record until the
public tracker exists (nothing here is silently skipped):

- **Synapse** — needs a one-time config generation into its volume
  before first start (upstream flow):
  `docker compose run --rm synapse generate`
  Then `docker compose up -d synapse`. Real deployment (postgres
  backend instead of the generated sqlite default, one instance per
  tenant, OIDC delegated to alo-identity) is ROADMAP Phase 2
  "Chat & Meet".
- **LiveKit** — boots with the dev placeholder key in
  `livekit/livekit.yaml`. Token minting from alo identities is
  ROADMAP Phase 2 "Chat & Meet".
- **Collabora** — boots standalone; it becomes useful when alo
  Drive serves the WOPI endpoints it calls (ROADMAP Phase 2
  "Drive & Docs"). `COLLABORA_ALIASGROUP1` must then name Drive's
  public host.

## Rules

- Never patch an engine image; a source patch requires an ADR first
  (CLAUDE.md standing rules).
- Version bumps: change the tag here, re-run
  `scripts/fetch-engines.sh`, note the bump in CHANGELOG.md — operators
  diff engine versions (release skill).
- Real secrets live in `.env` (gitignored), never in this directory.
