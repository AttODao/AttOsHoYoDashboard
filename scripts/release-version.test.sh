#!/usr/bin/env bash
# Tests for release-version.sh against synthetic crate manifests.
set -euo pipefail

script="$(cd "$(dirname "$0")" && pwd)/release-version.sh"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
failures=0

manifests() {
  mkdir -p "$work/dashboard-ui"
  printf '[package]\nname = "server"\nversion = "%s"\n\n[dependencies]\nversion = "9.9.9"\n' "$1" > "$work/Cargo.toml"
  printf '[package]\nname = "ui"\nversion = "%s"\n' "$2" > "$work/dashboard-ui/Cargo.toml"
}

accepts() {
  local tag=$1 expected=$2 output
  if ! output=$(bash "$script" "$tag" "$work" 2>&1); then
    echo "FAIL: $tag was rejected: $output"
    failures=$((failures + 1))
  elif [ "$output" != "$expected" ]; then
    printf 'FAIL: %s printed\n%s\nexpected\n%s\n' "$tag" "$output" "$expected"
    failures=$((failures + 1))
  fi
}

rejects() {
  local tag=$1 message=$2 output
  if output=$(bash "$script" "$tag" "$work" 2>&1); then
    echo "FAIL: $tag was accepted: $output"
    failures=$((failures + 1))
  elif [[ $output != *"$message"* ]]; then
    echo "FAIL: $tag: expected '$message' in: $output"
    failures=$((failures + 1))
  fi
}

# Arguments: server (web) version, dashboard-ui (app) version.
manifests 1.8.0 1.8.0
accepts v1.8.0 $'COMPONENT=app\nVERSION=1.8.0\nAPP_VERSION=1.8.0\nVERSION_CODE=10800\nPRERELEASE=false'
accepts v1.8.0-test.1 $'COMPONENT=app\nVERSION=1.8.0-test.1\nAPP_VERSION=1.8.0\nVERSION_CODE=10800\nPRERELEASE=true'
accepts v1.8.0-rc-2 $'COMPONENT=app\nVERSION=1.8.0-rc-2\nAPP_VERSION=1.8.0\nVERSION_CODE=10800\nPRERELEASE=true'
accepts web-v1.8.0 $'COMPONENT=web\nVERSION=1.8.0\nPRERELEASE=false'
rejects v1.8.1 'Tag v1.8.1 must match dashboard-ui/Cargo.toml (1.8.0)'
rejects web-v1.8.1 'Tag web-v1.8.1 must match Cargo.toml (1.8.0)'
rejects 1.8.0 'Tag must be'
rejects v1.8 'Tag must be'
rejects v01.8.0 'Tag must be'
rejects v1.8.0- 'Tag must be'
rejects 'v1.8.0-te st' 'Tag must be'
rejects v1.8.0+build 'Tag must be'
rejects app-v1.8.0 'Tag must be'
rejects webv1.8.0 'Tag must be'
rejects web-1.8.0 'Tag must be'
rejects Web-v1.8.0 'Tag must be'
rejects web-v01.8.0 'Tag must be'
rejects web-v1.8.0+build 'Tag must be'

# Independent versions: each tag reads only its own crate.
manifests 1.8.1 1.9.0
accepts v1.9.0 $'COMPONENT=app\nVERSION=1.9.0\nAPP_VERSION=1.9.0\nVERSION_CODE=10900\nPRERELEASE=false'
accepts web-v1.8.1 $'COMPONENT=web\nVERSION=1.8.1\nPRERELEASE=false'
accepts web-v1.8.1-test.1 $'COMPONENT=web\nVERSION=1.8.1-test.1\nPRERELEASE=true'
rejects v1.8.1 'Tag v1.8.1 must match dashboard-ui/Cargo.toml (1.9.0)'
rejects web-v1.9.0 'Tag web-v1.9.0 must match Cargo.toml (1.8.1)'

manifests 1.100.0 1.100.0
rejects v1.100.0 'below 100'
rejects web-v1.100.0 'below 100'

manifests 12.3.45 12.3.45
accepts v12.3.45 $'COMPONENT=app\nVERSION=12.3.45\nAPP_VERSION=12.3.45\nVERSION_CODE=120345\nPRERELEASE=false'

if [ "$failures" -ne 0 ]; then
  echo "$failures release-version check(s) failed" >&2
  exit 1
fi
echo "release-version: all checks passed"
