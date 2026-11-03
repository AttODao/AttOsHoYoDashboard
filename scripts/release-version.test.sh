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

manifests 1.8.0 1.8.0
accepts v1.8.0 $'VERSION=1.8.0\nAPP_VERSION=1.8.0\nVERSION_CODE=10800\nPRERELEASE=false'
accepts v1.8.0-test.1 $'VERSION=1.8.0-test.1\nAPP_VERSION=1.8.0\nVERSION_CODE=10800\nPRERELEASE=true'
accepts v1.8.0-rc-2 $'VERSION=1.8.0-rc-2\nAPP_VERSION=1.8.0\nVERSION_CODE=10800\nPRERELEASE=true'
rejects v1.8.1 'must match both package versions'
rejects 1.8.0 'Tag must be'
rejects v1.8 'Tag must be'
rejects v01.8.0 'Tag must be'
rejects v1.8.0- 'Tag must be'
rejects 'v1.8.0-te st' 'Tag must be'
rejects v1.8.0+build 'Tag must be'

manifests 1.8.0 1.7.0
rejects v1.8.0 'dashboard-ui/Cargo.toml 1.7.0'

manifests 1.100.0 1.100.0
rejects v1.100.0 'below 100'

manifests 12.3.45 12.3.45
accepts v12.3.45 $'VERSION=12.3.45\nAPP_VERSION=12.3.45\nVERSION_CODE=120345\nPRERELEASE=false'

if [ "$failures" -ne 0 ]; then
  echo "$failures release-version check(s) failed" >&2
  exit 1
fi
echo "release-version: all checks passed"
