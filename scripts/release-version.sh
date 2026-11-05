#!/usr/bin/env bash
# Resolve which deliverable a release tag publishes and check its version
# against that deliverable's crate only (docs/versioning.md):
#   vX.Y.Z[-suffix]      app (Android APK)      -> dashboard-ui/Cargo.toml
#   web-vX.Y.Z[-suffix]  web (container image)  -> Cargo.toml
#
# Usage: release-version.sh <tag> [repository root]
# Prints `KEY=value` lines for $GITHUB_ENV:
#   COMPONENT     app or web; each workflow fails unless it is its own deliverable
#   VERSION       tag without `v`/`web-v` (asset and image tags, e.g. 1.8.0 or 1.8.0-test.1)
#   APP_VERSION   app only: X.Y.Z part, the APK versionName
#   VERSION_CODE  app only: Android versionCode X*10000+Y*100+Z (same rule as build-android)
#   PRERELEASE    true for `-<suffix>` test tags, false for plain releases
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
if [[ ! $tag =~ ^(web-)?v(($number\.$number\.$number)(-($suffix))?)$ ]]; then
  fail "Tag must be vX.Y.Z (app) or web-vX.Y.Z (web), optionally with -<suffix>: $tag"
fi
if [ -n "${BASH_REMATCH[1]}" ]; then
  component=web
  manifest=Cargo.toml
else
  component=app
  manifest=dashboard-ui/Cargo.toml
fi
version=${BASH_REMATCH[2]}
release=${BASH_REMATCH[3]}
major=${BASH_REMATCH[4]}
minor=${BASH_REMATCH[5]}
patch=${BASH_REMATCH[6]}
prerelease=false
if [ -n "${BASH_REMATCH[7]}" ]; then
  prerelease=true
fi
# The Android versionCode needs this; web tags follow the same number rules.
if [ "$minor" -ge 100 ] || [ "$patch" -ge 100 ]; then
  fail "Minor and patch must be below 100: $tag"
fi

crate=$(crate_version "$root/$manifest")
if [ "$crate" != "$release" ]; then
  fail "Tag $tag must match $manifest ($crate)"
fi

echo "COMPONENT=$component"
echo "VERSION=$version"
if [ "$component" = app ]; then
  echo "APP_VERSION=$release"
  echo "VERSION_CODE=$((major * 10000 + minor * 100 + patch))"
fi
echo "PRERELEASE=$prerelease"
