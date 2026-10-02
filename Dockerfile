# syntax=docker/dockerfile:1.7
# Single image: Rust API server + built front-end.
# Base images are pinned by digest (Dependabot proposes updates); the runtime is
# distroless: no shell, no package manager, non-root user.

FROM node:22-bookworm-slim@sha256:43ac6c60b8f89723f746e8a92ce91abd5017e627ce1ddfe4238355d3a30b772c AS web
WORKDIR /src/web
COPY web/package.json web/package-lock.json ./
RUN npm ci --ignore-scripts
COPY web/ ./
RUN npm run build

FROM rust:1-bookworm@sha256:59037199c44290f2befcdd58dcc540164763fc296950255aaefeef096a1866b0 AS server
WORKDIR /src
COPY Cargo.toml Cargo.lock rustfmt.toml ./
COPY server/ server/
COPY config/ config/
COPY examples/ examples/
RUN cargo build --release --locked -p lectern-server \
    && mkdir -p /out/data /out/branding

FROM gcr.io/distroless/cc-debian12:nonroot@sha256:9dac0a79194e45a7da0158a9c6da57b217585af0786db3845d1f0ec1a0dd182f
WORKDIR /app
COPY --from=server /src/target/release/lectern-server /usr/local/bin/lectern
COPY --from=web /src/web/dist /app/web/dist
COPY --from=server --chown=65532:65532 /out/data /app/data
COPY --from=server /out/branding /app/branding
USER 65532:65532
ENV LECTERN__SERVER__STATIC_DIR=/app/web/dist \
    LECTERN__SERVER__DATA_DIR=/app/data \
    LECTERN__SERVER__BRANDING_DIR=/app/branding \
    LECTERN_LOG_FORMAT=json
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/lectern"]
CMD ["serve"]
