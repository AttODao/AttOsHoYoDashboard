#!/usr/bin/env bash
# Tests for release-scope.sh in a throwaway git repository.
set -euo pipefail

script="$(cd "$(dirname "$0")" && pwd)/release-scope.sh"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
failures=0

git_() {
  git -C "$work" -c user.name=test -c user.email=test@example.invalid -c commit.gpgsign=false -c tag.gpgsign=false "$@"
}

touch_commit() {
  local path
  for path in "$@"; do
    mkdir -p "$work/$(dirname "$path")"
    echo "$RANDOM" >> "$work/$path"
  done
  git_ add -A
  git_ commit -qm "change $*"
}

contains() {
  local name=$1 output=$2 expected=$3
  if [[ $output != *"$expected"* ]]; then
    printf 'FAIL: %s: expected %q in\n%s\n' "$name" "$expected" "$output"
    failures=$((failures + 1))
  fi
}

lacks() {
  local name=$1 output=$2 unexpected=$3
  if [[ $output == *"$unexpected"* ]]; then
    printf 'FAIL: %s: unexpected %q in\n%s\n' "$name" "$unexpected" "$output"
    failures=$((failures + 1))
  fi
}

scope() {
  (cd "$work" && bash "$script" "$@")
}

git_ init -q
touch_commit Cargo.toml dashboard-ui/Cargo.toml
git_ tag -a v1.8.0 -m 'Release 1.8.0'
git_ tag -a v1.8.1-test.1 -m test

# Nothing since the last shared release.
output=$(scope)
contains 'no changes' "$output" 'app: changes since v1.8.0'
contains 'no changes' "$output" 'web: changes since v1.8.0'
contains 'no changes' "$output" 'app: no bump needed'
contains 'no changes' "$output" 'web: no bump needed'

# Web-only paths: the app is not affected.
touch_commit src/main.rs Dockerfile docs/dashboard-api.json
output=$(scope)
contains 'web only' "$output" 'web: bump (3 web path(s))'
contains 'web only' "$output" 'app: no bump needed'
contains 'web only' "$output" '  web     src/main.rs'
contains 'web only' "$output" '(3 path(s) outside the app ignored)'

# A web release moves only the web base; the app still compares from v1.8.0.
git_ tag -a web-v1.8.1 -m 'Release web 1.8.1'
git_ tag -a web-v1.8.2-test.1 -m test
output=$(scope)
contains 'after web tag' "$output" 'web: changes since web-v1.8.1'
contains 'after web tag' "$output" 'web: no bump needed'
contains 'after web tag' "$output" 'app: changes since v1.8.0'
contains 'after web tag' "$output" 'app: no bump needed'

# App-only paths, docs and tests: the web is not affected.
touch_commit dashboard-ui/src/mobile/updates.rs dashboard-ui/android/MainActivity.kt \
  dashboard-ui/src/pages/app_update.rs docs/releases.md tests/cli.rs dashboard-ui/tests/browser.mjs
output=$(scope)
contains 'app only' "$output" 'app: bump (3 app path(s))'
contains 'app only' "$output" 'web: no bump needed'
lacks 'app only' "$output" 'none'

# Shared UI code and unknown paths ask for a decision on both sides.
touch_commit dashboard-ui/src/pages/about.rs Cargo.lock mystery.txt
output=$(scope)
contains 'shared' "$output" '  shared  dashboard-ui/src/pages/about.rs'
contains 'shared' "$output" 'web: decide; read the shared diff: git diff web-v1.8.1 HEAD -- Cargo.lock dashboard-ui/src/pages/about.rs; classify by hand: mystery.txt'
contains 'shared' "$output" 'app: bump (3 app path(s)); read the shared diff: git diff v1.8.0 HEAD -- Cargo.lock dashboard-ui/src/pages/about.rs'

# Newest plain tag wins (version order, not creation order); explicit bases override.
git_ tag -a v1.10.0 -m 'Release app 1.10.0' HEAD
git_ tag -a v1.9.0 -m 'Release app 1.9.0' HEAD~1
output=$(scope)
contains 'version order' "$output" 'app: changes since v1.10.0'
contains 'version order' "$output" 'app: no bump needed'
output=$(scope v1.9.0 v1.8.0)
contains 'explicit bases' "$output" 'app: changes since v1.9.0'
contains 'explicit bases' "$output" 'web: changes since v1.8.0'
contains 'explicit bases' "$output" 'web: bump (3 web path(s))'

if output=$(scope no-such-tag 2>&1); then
  echo "FAIL: unknown base was accepted: $output"
  failures=$((failures + 1))
else
  contains 'unknown base' "$output" 'Unknown base for app: no-such-tag'
fi

if [ "$failures" -ne 0 ]; then
  echo "$failures release-scope check(s) failed" >&2
  exit 1
fi
echo "release-scope: all checks passed"
