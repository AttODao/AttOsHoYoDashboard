#!/usr/bin/env bash
# Launch API/Dashboard independently. The db mode only provisions local devenv PostgreSQL.
set -euo pipefail
mode=${1:?api, dashboard or db}
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
    api)
        : "${HOYOLAB_COOKIE_KEY:?Set HOYOLAB_COOKIE_KEY}"
        : "${DB_NAME:?Set DB_NAME}"
        : "${DB_USER:?Set DB_USER}"
        : "${DB_PASSWORD:?Set DB_PASSWORD}"
        export DB_HOST="${DB_HOST:-127.0.0.1}" DB_PORT="${DB_PORT:-5432}"
        unset DATABASE_URL HOYOLAB_COOKIE DB_ADMIN_PASSWORD
        "$binary" migrate
        users=$(PGHOST="$DB_HOST" PGPORT="$DB_PORT" PGUSER="$DB_USER" PGDATABASE="$DB_NAME" PGPASSWORD="$DB_PASSWORD" \
            psql -X -w -Atc 'SELECT count(*) FROM users')
        if [[ $users == 0 ]]; then "$binary" bootstrap-admin; fi
        unset INITIAL_ADMIN_USERNAME INITIAL_ADMIN_PASSWORD
        exec "$binary" serve-api
        ;;
    dashboard)
        unset DB_PASSWORD DATABASE_URL INITIAL_ADMIN_USERNAME INITIAL_ADMIN_PASSWORD HOYOLAB_COOKIE_KEY HOYOLAB_COOKIE DB_ADMIN_PASSWORD
        exec "$binary" serve-dashboard > >(while IFS= read -r line; do
            printf '%s\n' "$line"
            if [[ $line == *'dashboard listening'* ]]; then
                echo 'Dashboard: http://localhost:8080 (API: 127.0.0.1:8081)'
                echo 'For SSH, run on your LOCAL computer:'
                echo 'ssh -N -T -o ExitOnForwardFailure=yes -L 127.0.0.1:8080:127.0.0.1:8080 devcon'
                echo 'Ctrl+C stops processes started by this command.'
            fi
        done)
        ;;
    *) echo 'Usage: launch-server.sh api|dashboard|db BINARY' >&2; exit 1;;
esac
