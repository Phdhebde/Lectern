# syntax=docker/dockerfile:1.7
# Single image: Rust API server + built front-end. Runs as an unprivileged user.

FROM node:22-bookworm-slim AS web
WORKDIR /src/web
COPY web/package.json web/package-lock.json ./
RUN npm ci --ignore-scripts
COPY web/ ./
RUN npm run build

FROM rust:1-bookworm AS server
WORKDIR /src
COPY Cargo.toml Cargo.lock rustfmt.toml ./
COPY server/ server/
COPY config/ config/
COPY examples/ examples/
RUN cargo build --release --locked -p lectern-server

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home /app lectern \
    && mkdir -p /app/data /app/branding && chown -R lectern /app/data
WORKDIR /app
COPY --from=server /src/target/release/lectern-server /usr/local/bin/lectern
COPY --from=web /src/web/dist /app/web/dist
USER 10001
ENV LECTERN__SERVER__STATIC_DIR=/app/web/dist \
    LECTERN__SERVER__DATA_DIR=/app/data \
    LECTERN__SERVER__BRANDING_DIR=/app/branding \
    LECTERN_LOG_FORMAT=json
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/lectern"]
CMD ["serve"]
