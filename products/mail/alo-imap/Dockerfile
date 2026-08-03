# Build context is the repository root (the Cargo workspace);
# compose invokes this as: context ".." dockerfile "products/mail/alo-imap/Dockerfile".
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
RUN cargo build --release --package alo-imap

FROM debian:bookworm-slim AS runtime
RUN groupadd --system --gid 10001 alo && useradd --system --no-create-home --uid 10001 --gid 10001 alo \
    && mkdir -p /var/lib/alo/blobs \
    && chown -R alo /var/lib/alo
COPY --from=build /src/target/release/alo-imap /usr/local/bin/alo-imap
USER alo
ENV ALO_BLOB_DIR=/var/lib/alo/blobs
# 993 implicit-TLS IMAP, 143 STARTTLS IMAP, 995 implicit-TLS POP3.
EXPOSE 993 143 995
# The binary probes itself (`--healthcheck` TCP-connects a listener), so the
# runtime image needs no extra tooling.
ENTRYPOINT ["/usr/local/bin/alo-imap"]
