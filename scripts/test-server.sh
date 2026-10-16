#!/usr/bin/env bash
# Only called by the isolated devenv test-server process, from its private cwd.
set -euo pipefail
binary=$1
: "${HOYOLAB_COOKIE_KEY:?Set HOYOLAB_COOKIE_KEY in .env.test}"
"$binary" migrate
if [[ $(psql -X "$DATABASE_URL" -Atc 'SELECT count(*) FROM users') == 0 ]]; then
    "$binary" bootstrap-admin
fi
unset INITIAL_ADMIN_USERNAME INITIAL_ADMIN_PASSWORD
"$binary" serve | while IFS= read -r line; do
    printf '%s\n' "$line"
    if [[ $line == *'dashboard listening'* ]]; then
        echo 'Run on your LOCAL computer:'
        echo 'ssh -N -T -o ExitOnForwardFailure=yes -L 127.0.0.1:8080:127.0.0.1:8080 devcon'
        echo 'Browser: http://localhost:8080'
        echo 'Initial login credentials: see .env.test (existing users are not reset).'
        echo 'Ctrl+C here stops API and PostgreSQL. Test data stays under .devenv/.'
    fi
done
