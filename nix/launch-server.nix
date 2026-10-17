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
      BIND_ADDR=127.0.0.1:8080 PUBLIC_ORIGIN=http://localhost:8080 RP_ID=localhost \
      DASHBOARD_UI_DIR="$DEVENV_ROOT/target/dx/attos-dashboard-ui/release/web/public" \
      node --env-file="$DEVENV_ROOT/.env.test" -e '
        const [shell, script, mode, binary] = process.argv.slice(1);
        process.execve(shell, [shell, script, mode, binary], process.env);
      ' "${pkgs.bash}/bin/bash" "$DEVENV_ROOT/scripts/launch-server.sh" ${mode} "$DEVENV_ROOT/target/debug/attoshoyodashboard"
  '';
in
{
  scripts.launch-server.exec = ''
    set -e
    cd "$DEVENV_ROOT"
    build-ui
    cargo build --locked --bin attoshoyodashboard
    exec ${pkgs.devenv}/bin/devenv --no-tui --profile launch-server up
  '';

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
      before = [ "devenv:processes:app" ];
      exec = run "db";
    };
    processes.app.exec = run "app";
  };
}
