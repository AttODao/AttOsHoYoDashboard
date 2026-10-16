{ pkgs, ... }:

let
  run = mode: ''
    work="$DEVENV_STATE/launch-${mode}"
    mkdir -p "$work"
    chmod 700 "$work"
    : > "$work/.env"
    cd "$work"
    exec env -i PATH="$PATH" HOME="$work" DEVENV_RUNTIME="$DEVENV_RUNTIME" \
      DB_HOST="$PGHOST" DB_PORT="$PGPORT" \
      API_BIND_ADDR=127.0.0.1:8081 DASHBOARD_BIND_ADDR=127.0.0.1:8080 \
      API_ORIGIN=http://127.0.0.1:8081 PUBLIC_ORIGIN=http://localhost:8080 RP_ID=localhost \
      DASHBOARD_UI_DIR="$DEVENV_ROOT/target/dx/attos-dashboard-ui/release/web/public" \
      node --env-file="$DEVENV_ROOT/.env.test" -e '
        const [shell, script, mode, binary] = process.argv.slice(1);
        process.execve(shell, [shell, script, mode, binary], process.env);
      ' "${pkgs.bash}/bin/bash" "$DEVENV_ROOT/scripts/launch-server.sh" ${mode} "$DEVENV_ROOT/target/debug/attosgameapi"
  '';
  start = targets: withUi: ''
    set -e
    cd "$DEVENV_ROOT"
    ${if withUi then "build-ui" else ""}
    cargo build --locked --bin attosgameapi
    exec ${pkgs.devenv}/bin/devenv --no-tui --profile launch-server up ${targets}
  '';
in
{
  scripts.launch-server.exec = start "" true;
  scripts.launch-api.exec = start "api" false;
  scripts.launch-dashboard.exec = start "dashboard" true;

  profiles.launch-server.module = {
    services.postgres = {
      enable = true;
      package = pkgs.postgresql_18;
      listen_addresses = "";
      createDatabase = false;
      settings.password_encryption = "scram-sha-256";
      hbaConf = ''
        local postgres,template1 all peer
        local all all scram-sha-256
      '';
    };
    tasks."launch:database" = {
      after = [ "devenv:processes:postgres@ready" ];
      before = [ "devenv:processes:api" ];
      exec = run "db";
    };
    processes.api.exec = run "api";
    # No API/DB dependency: the Dashboard can start even while the API is down.
    processes.dashboard.exec = run "dashboard";
  };
}
