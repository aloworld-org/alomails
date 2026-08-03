# Build context is the repository root (the Cargo workspace);
# compose invokes this as: context ".." dockerfile "products/mail/alo-jmap/Dockerfile".
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
# Also build identityctl (the admin/provisioning CLI) so the running image
# can bootstrap the first admin via `docker compose exec`.
RUN cargo build --release --package alo-jmap --package alo-identity --bins

FROM debian:bookworm-slim AS runtime
RUN groupadd --system --gid 10001 alo && useradd --system --no-create-home --uid 10001 --gid 10001 alo \
    && mkdir -p /var/lib/alo/blobs \
    && chown -R alo /var/lib/alo
COPY --from=build /src/target/release/alo-jmap /usr/local/bin/alo-jmap
COPY --from=build /src/target/release/identityctl /usr/local/bin/identityctl
USER alo
ENV ALO_BLOB_DIR=/var/lib/alo/blobs \
    ALO_JMAP_ADDR=0.0.0.0:8080
# The JMAP API + OIDC provider listens on plain HTTP internally; the front
# proxy (Caddy) terminates TLS on 443 and forwards here.
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/alo-jmap"]
