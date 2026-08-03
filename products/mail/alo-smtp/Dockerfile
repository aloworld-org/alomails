# Build context is the repository root (the Cargo workspace);
# compose invokes this as: context ".." dockerfile "products/mail/alo-smtp/Dockerfile".
FROM rust:1.90-slim AS build
WORKDIR /src
# SQLX_OFFLINE + the checked-in .sqlx query cache let sqlx's compile-time
# query checks pass without a database at build time.
ENV SQLX_OFFLINE=true
COPY Cargo.toml Cargo.lock ./
COPY .sqlx ./.sqlx
COPY platform ./platform
COPY products ./products
COPY migrate ./migrate
RUN cargo build --release --package alo-smtp

FROM debian:bookworm-slim AS runtime
RUN groupadd --system --gid 10001 alo && useradd --system --no-create-home --uid 10001 --gid 10001 alo \
    && mkdir -p /var/spool/alo \
    && chown alo /var/spool/alo
COPY --from=build /src/target/release/alo-smtp /usr/local/bin/alo-smtp
USER alo
ENV ALO_SMTP_SPOOL_DIR=/var/spool/alo
# 2525 MX (dev), 587 submission/STARTTLS, 465 implicit TLS.
EXPOSE 2525 587 465
# Healthcheck is the binary itself (`--healthcheck` probes for the 220
# greeting), so the runtime image carries no shell tooling for it.
ENTRYPOINT ["/usr/local/bin/alo-smtp"]
