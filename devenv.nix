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
  imports = [ ./nix/launch-server.nix ];

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

  # Android app (dashboard-ui `mobile` feature): `devenv --profile android shell`.
  # Kept out of the default shell because the SDK/NDK are several GB and unfree;
  # entering this profile accepts the Android SDK licenses.
  profiles.android.module = {
    android = {
      enable = true;
      platforms.version = [ "34" ];
      buildTools.version = [ "34.0.0" ];
      abis = [ "arm64-v8a" "x86_64" ];
      cmake.version = [ ];
      extras = [ ];
      emulator.enable = false;
      systemImages.enable = false;
      googleAPIs.enable = false;
      googleTVAddOns.enable = false;
    };
    languages.java.jdk.package = pkgs.jdk17;
    # dx looks up the NDK through ANDROID_NDK_HOME.
    enterShell = ''
      export ANDROID_NDK_HOME="$ANDROID_NDK_ROOT"
    '';
    scripts.build-android.exec = ''
      exec ${pkgs.bash}/bin/bash "$DEVENV_ROOT/scripts/build-android.sh" "$@"
    '';
    scripts.check-android.exec = ''
      set -e
      cd "$DEVENV_ROOT"
      ndk_bin="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin"
      export CC_aarch64_linux_android="$ndk_bin/aarch64-linux-android24-clang"
      export AR_aarch64_linux_android="$ndk_bin/llvm-ar"
      export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CC_aarch64_linux_android"
      exec cargo clippy --locked -p attos-dashboard-ui --no-default-features --features mobile --target aarch64-linux-android -- -D warnings
    '';
  };
  # Emulator and x86_64 system image for local testing (needs KVM):
  # `devenv --profile android --profile android-emulator shell`.
  profiles.android-emulator.module = {
    android = {
      emulator.enable = true;
      systemImages.enable = true;
      googleAPIs.enable = true;
      systemImageTypes = [ "google_apis" ];
      abis = [ "arm64-v8a" "x86_64" ];
    };
  };

  enterTest = ''
    set -e
    check-rust
    test-rust
    test-ui
  '';
}
