#!/usr/bin/env bash
# Development/test launcher only; production Docker executes the Rust binary directly.
set -euo pipefail
mode=${1:?app or db}
binary=${2:?application binary}
case "$mode" in
    db)
        : "${DB_NAME:?Set DB_NAME}"
        : "${DB_USER:?Set DB_USER}"
        : "${DB_PASSWORD:?Set DB_PASSWORD}"
        [[ "$DB_HOST" == "$DEVENV_RUNTIME/postgres" ]] || { echo 'Refusing a non-devenv database socket.' >&2; exit 1; }
        case "$DB_NAME" in postgres|template0|template1) echo 'Use a dedicated database name.' >&2; exit 1;; esac
        sql="$(dirname "${BASH_SOURCE[0]}")/launch-db.sql"
        if ! PGHOST="$DB_HOST" PGPORT="$DB_PORT" PGUSER="$(id -un)" PGDATABASE=postgres PGPASSWORD='' \
            psql -X -w -q -f "$sql" >/dev/null 2>&1; then
            echo 'Database setup failed. Check DB_NAME/DB_USER/DB_PASSWORD and database ownership.' >&2
            exit 1
        fi
        ;;
    app)
        : "${HOYOLAB_COOKIE_KEY:?Set HOYOLAB_COOKIE_KEY}"
        : "${DB_NAME:?Set DB_NAME}"
        : "${DB_USER:?Set DB_USER}"
        : "${DB_PASSWORD:?Set DB_PASSWORD}"
        unset DATABASE_URL HOYOLAB_COOKIE DB_ADMIN_PASSWORD
        exec "$binary" serve > >(while IFS= read -r line; do
            printf '%s\n' "$line"
            if [[ $line == *'dashboard listening'* ]]; then
                echo 'Dashboard/API: http://localhost:8080'
                echo 'For SSH, run on your LOCAL computer:'
                echo 'ssh -N -T -o ExitOnForwardFailure=yes -L 127.0.0.1:8080:127.0.0.1:8080 devcon'
                echo 'Ctrl+C stops processes started by this command.'
            fi
        done)
        ;;
    *) echo 'Usage: launch-server.sh app|db BINARY' >&2; exit 1;;
esac
