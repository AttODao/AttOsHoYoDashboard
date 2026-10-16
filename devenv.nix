{ pkgs, inputs, ... }:

let
  # Cargo.lock needs a newer CLI than the one bundled with the Dioxus package.
  wasmBindgen = inputs.wasm-tools.legacyPackages.${pkgs.stdenv.hostPlatform.system}.wasm-bindgen-cli;
  browsers = pkgs.playwright-driver.browsers.override {
    withFirefox = false;
    withWebkit = false;
  };
in
{
  imports = [ ./nix/test-server.nix ];

  # Never load account credentials or a production DATABASE_URL on shell entry.
  dotenv.enable = false;

  languages.rust = {
    enable = true;
    toolchainFile = ./rust-toolchain.toml;
  };

  packages = with pkgs; [
    git
    pkg-config
    openssl
    postgresql_18
    dioxus-cli
    binaryen
    wasmBindgen
    nodejs
    playwright-driver
  ];

  env = {
    PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
    ATTOS_TEST_PG_BIN = "${pkgs.postgresql_18}/bin";
    ATTOS_TEST_PLAYWRIGHT_MODULE = "${pkgs.playwright-driver}";
    PLAYWRIGHT_BROWSERS_PATH = "${browsers}";
    PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD = "1";
  };

  # Tests own temporary PostgreSQL clusters; no shared DB service is started.
  scripts.check-rust.exec = ''
    set -e
    cd "$DEVENV_ROOT"
    cargo fmt --all -- --check
    cargo clippy --locked --workspace --all-targets -- -D warnings
    cargo clippy --locked -p attos-dashboard-ui --target wasm32-unknown-unknown -- -D warnings
  '';
  scripts.test-rust.exec = ''
    cd "$DEVENV_ROOT"
    exec env -u DATABASE_URL cargo test --locked --workspace --all-targets -- --test-threads=2
  '';
  scripts.build-ui.exec = ''
    cd "$DEVENV_ROOT"
    exec dx build --package attos-dashboard-ui --platform web --release
  '';
  scripts.test-ui.exec = ''
    set -e
    cd "$DEVENV_ROOT"
    build-ui
    node dashboard-ui/tests/browser.mjs
    node dashboard-ui/tests/browser.mjs --normal-motion
  '';

  enterTest = ''
    set -e
    check-rust
    test-rust
    test-ui
  '';
}
