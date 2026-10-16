FROM rust:1.99.0-alpine3.23 AS build
RUN apk add --no-cache build-base pkgconf openssl-dev openssl-libs-static binaryen esbuild
ENV OPENSSL_STATIC=1
RUN rustup target add wasm32-unknown-unknown \
    && cargo install dioxus-cli --version 0.7.10 --locked \
    && cargo install wasm-bindgen-cli --version 0.2.129 --locked
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src src
COPY dashboard-ui dashboard-ui
COPY docs/dashboard-api.json docs/dashboard-api.json
RUN cargo build --release --locked --bin attosgameapi \
    && NO_DOWNLOADS=1 dx build --package attos-dashboard-ui --platform web --release

FROM alpine:3.23
RUN apk add --no-cache ca-certificates-bundle && adduser -D -u 10001 app
WORKDIR /app
COPY --from=build /build/target/release/attosgameapi /usr/local/bin/attosgameapi
COPY --from=build /build/target/dx/attos-dashboard-ui/release/web/public /app/public
ENV DASHBOARD_UI_DIR=/app/public
USER app
ENTRYPOINT ["/usr/local/bin/attosgameapi"]
CMD ["serve"]
