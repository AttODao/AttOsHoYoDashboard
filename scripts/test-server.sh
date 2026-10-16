#!/usr/bin/env bash
# Only called by the isolated devenv test-server process, from its private cwd.
set -euo pipefail
binary=$1
"$binary" migrate
if [[ $(psql -X "$DATABASE_URL" -Atc 'SELECT count(*) FROM users') == 0 ]]; then
    printf '%s\n' 'public-test-only-password' | "$binary" bootstrap-admin test-admin
fi
"$binary" serve | while IFS= read -r line; do
    printf '%s\n' "$line"
    if [[ $line == *'dashboard listening'* ]]; then
        echo 'Run on your LOCAL computer:'
        echo 'ssh -N -T -o ExitOnForwardFailure=yes -L 127.0.0.1:8080:127.0.0.1:8080 devcon'
        echo 'Browser: http://localhost:8080'
        echo 'TEST ONLY login: test-admin / public-test-only-password (first start; change required).'
        echo 'Ctrl+C here stops API and PostgreSQL. Test data stays under .devenv/.'
    fi
done
