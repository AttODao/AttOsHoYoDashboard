#!/usr/bin/env bash
# Resolve the release version of a `v*` tag and check it against both crates.
#
# Usage: release-version.sh <tag> [repository root]
# Prints `KEY=value` lines for $GITHUB_ENV:
#   VERSION       tag without `v` (asset names use it, e.g. 1.8.0 or 1.8.0-test.1)
#   APP_VERSION   X.Y.Z part; must equal both Cargo.toml package versions
#   VERSION_CODE  Android versionCode X*10000+Y*100+Z (same rule as build-android)
#   PRERELEASE    true for `vX.Y.Z-<suffix>` test tags, false for plain releases
set -euo pipefail

tag=${1:?usage: release-version.sh <tag> [repository root]}
root=${2:-.}

fail() {
  echo "$*" >&2
  exit 1
}

crate_version() {
  # The nixos/nix CI image has no sed/awk: read the [package] table with bash only.
  local line in_package=false
  while IFS= read -r line || [ -n "$line" ]; do
    if [[ $line =~ ^\[([^]]*)\][[:space:]]*$ ]]; then
      in_package=false
      if [ "${BASH_REMATCH[1]}" = package ]; then
        in_package=true
      fi
    elif $in_package && [[ $line =~ ^version[[:space:]]*=[[:space:]]*\"([^\"]*)\" ]]; then
      echo "${BASH_REMATCH[1]}"
      return
    fi
  done < "$1"
}

number='(0|[1-9][0-9]*)'
suffix='[0-9A-Za-z]+([.-][0-9A-Za-z]+)*'
if [[ ! $tag =~ ^v($number\.$number\.$number)(-($suffix))?$ ]]; then
  fail "Tag must be vX.Y.Z or vX.Y.Z-<suffix>: $tag"
fi
app_version=${BASH_REMATCH[1]}
major=${BASH_REMATCH[2]}
minor=${BASH_REMATCH[3]}
patch=${BASH_REMATCH[4]}
prerelease=false
if [ -n "${BASH_REMATCH[5]}" ]; then
  prerelease=true
fi
if [ "$minor" -ge 100 ] || [ "$patch" -ge 100 ]; then
  fail "Minor and patch must be below 100 for the Android versionCode: $tag"
fi

server_version=$(crate_version "$root/Cargo.toml")
ui_version=$(crate_version "$root/dashboard-ui/Cargo.toml")
if [ "$server_version" != "$app_version" ] || [ "$ui_version" != "$app_version" ]; then
  fail "Tag $tag must match both package versions (Cargo.toml $server_version, dashboard-ui/Cargo.toml $ui_version)"
fi

echo "VERSION=${tag#v}"
echo "APP_VERSION=$app_version"
echo "VERSION_CODE=$((major * 10000 + minor * 100 + patch))"
echo "PRERELEASE=$prerelease"
