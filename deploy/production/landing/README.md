# landing/ — served here, authored elsewhere

Caddy serves this directory at **https://alomails.com** (see the apex block in
`../Caddyfile` and the one-time `../init-landing.sh`). The **content** — the
marketing site — is NOT in this repo; it lives in and deploys from:

    https://github.com/aloworld-org/alomails-website

That repo's `deploy.sh` copies the built files here. This directory is kept
(with this note) only so the read-only mount in `../docker-compose.yml`
(`./landing:/srv-landing:ro`) has something to bind. Do not add site content
here — add it to the website repo.
