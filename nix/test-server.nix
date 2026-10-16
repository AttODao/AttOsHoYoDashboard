{ pkgs, ... }:

{
  scripts.test-server.exec = ''
    set -e
    : "''${HOYOLAB_COOKIE_KEY:?Set HOYOLAB_COOKIE_KEY to a test-only 64-character hexadecimal key}"
    cd "$DEVENV_ROOT"
    build-ui
    cargo build --locked --bin attosgameapi
    exec ${pkgs.devenv}/bin/devenv --no-tui --profile test-server up
  '';

  profiles.test-server.module = {
    services.postgres = {
      enable = true;
      package = pkgs.postgresql_18;
      listen_addresses = ""; # Private Unix socket, no TCP database listener.
      initialDatabases = [ { name = "attos_test"; } ];
    };
    tasks."devenv:processes:dashboard".after = [ "devenv:processes:postgres@ready" ];
    processes.dashboard.exec = ''
      work="$DEVENV_STATE/test-server"
      mkdir -p "$work"
      chmod 700 "$work"
      : > "$work/.env" # Stop dotenvy's parent-directory search here.
      cd "$work"
      exec env -i PATH="$PATH" HOME="$work" \
        DATABASE_URL="postgres://$(id -un)@localhost/attos_test?host=$PGHOST" \
        HOYOLAB_COOKIE_KEY="''${HOYOLAB_COOKIE_KEY:?Set HOYOLAB_COOKIE_KEY}" \
        BIND_ADDR=127.0.0.1:8080 PUBLIC_ORIGIN=http://localhost:8080 RP_ID=localhost \
        DASHBOARD_UI_DIR="$DEVENV_ROOT/target/dx/attos-dashboard-ui/release/web/public" \
        bash "$DEVENV_ROOT/scripts/test-server.sh" "$DEVENV_ROOT/target/debug/attosgameapi"
    '';
  };
}
