FROM rust:1.99-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
RUN rustup target add wasm32-unknown-unknown \
    && cargo install dioxus-cli --version 0.7.10 --locked \
    && cargo install wasm-bindgen-cli --version 0.2.129 --locked
RUN curl -fsSL "https://github.com/WebAssembly/binaryen/releases/download/version_132/binaryen-version_132-$(uname -m)-linux.tar.gz" \
    | tar -xz --strip-components=1 -C /usr/local && ldconfig
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src src
COPY dashboard-ui dashboard-ui
COPY docs/dashboard-api.json docs/dashboard-api.json
RUN cargo build --release --locked --bin attosgameapi \
    && dx build --package attos-dashboard-ui --platform web --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends bash ca-certificates libssl3 postgresql-client \
    && rm -rf /var/lib/apt/lists/* && useradd --create-home --uid 10001 app
WORKDIR /app
COPY --from=build /build/target/release/attosgameapi /usr/local/bin/attosgameapi
COPY --from=build /build/target/dx/attos-dashboard-ui/release/web/public /app/public
COPY scripts/launch-server.sh /app/scripts/launch-server.sh
ENV DASHBOARD_UI_DIR=/app/public
USER app
ENTRYPOINT ["bash", "/app/scripts/launch-server.sh"]
CMD ["api", "/usr/local/bin/attosgameapi"]
