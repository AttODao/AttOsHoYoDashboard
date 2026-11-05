#!/usr/bin/env bash
# Show which deliverable a set of changes affects, to decide what to release
# (rules: docs/versioning.md 2.4). It only prints; the decision stays with you.
#
# Usage: release-scope.sh [<app base> [<web base>]]
#   app base  default: newest plain app tag vX.Y.Z
#   web base  default: newest plain web tag web-vX.Y.Z, or v1.8.0 (the last
#             release that published both) while no web tag exists
# Each deliverable is compared from its own base to HEAD. Paths are classified as
#   app     only in the Android app          -> bump dashboard-ui/Cargo.toml, tag vX.Y.Z
#   web     only in the server image         -> bump Cargo.toml, tag web-vX.Y.Z
#   shared  shared UI crate / toolchain      -> read the diff (cfg, platform::APP) and decide
#   none    tests, docs, dev tooling         -> no release
#   other   not covered by the rules         -> classify by hand
set -euo pipefail

last_shared_tag=v1.8.0

classify() {
  case $1 in
    tests/* | dashboard-ui/tests/* | README.md | CHANGELOG.md | AGENTS.md | PRODUCT.md | DESIGN.md | LICENSE \
      | devenv.* | nix/* | .agents/* | .pi/* | .impeccable/* | .gitignore | .dockerignore \
      | scripts/*.test.* | scripts/release-version.sh | scripts/release-scope.sh \
      | scripts/launch-* | scripts/test-android.sh | scripts/subset-fonts.sh)
      echo none ;;
    docs/dashboard-api.json | src/* | Dockerfile | Cargo.toml | .forgejo/workflows/image.yml)
      echo web ;;
    docs/*)
      echo none ;;
    dashboard-ui/android/* | dashboard-ui/src/mobile/* | dashboard-ui/assets/mobile.css \
      | dashboard-ui/src/app/device_notifications.rs | dashboard-ui/src/app/home_widgets.rs \
      | dashboard-ui/src/app/updates.rs \
      | dashboard-ui/src/pages/app_update.rs | dashboard-ui/src/pages/device_notifications.rs \
      | scripts/build-android.sh | scripts/publish-release.sh | .forgejo/workflows/android.yml)
      echo app ;;
    dashboard-ui/* | Cargo.lock | rust-toolchain.toml)
      echo shared ;;
    *)
      echo other ;;
  esac
}

latest_tag() {
  # Newest plain release tag with the given prefix (test tags have a suffix).
  local prefix=$1 tag
  while IFS= read -r tag; do
    if [[ $tag =~ ^${prefix}(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
      echo "$tag"
      return
    fi
  done < <(git tag --list "${prefix}*" --sort=-version:refname)
}

app_base=${1:-$(latest_tag v)}
web_base=${2:-$(latest_tag web-v)}
web_base=${web_base:-$last_shared_tag}
if [ -z "$app_base" ]; then
  echo "No app tag vX.Y.Z found; pass the base explicitly." >&2
  exit 1
fi

summaries=()
report() {
  local deliverable=$1 base=$2 path class own=0 ignored=0
  local -a shared=() other=()
  git rev-parse --quiet --verify "$base^{commit}" > /dev/null || {
    echo "Unknown base for $deliverable: $base" >&2
    exit 1
  }
  echo "$deliverable: changes since $base"
  while IFS= read -r path; do
    class=$(classify "$path")
    case $class in
      "$deliverable") own=$((own + 1)) ;;
      shared) shared+=("$path") ;;
      other) other+=("$path") ;;
      *) ignored=$((ignored + 1)); continue ;;
    esac
    printf '  %-7s %s\n' "$class" "$path"
  done < <(git diff --name-only "$base" HEAD)
  echo "  ($ignored path(s) outside the $deliverable ignored)"

  local summary
  if [ "$own" -gt 0 ]; then
    summary="bump ($own $deliverable path(s))"
  elif [ "${#shared[@]}" -gt 0 ] || [ "${#other[@]}" -gt 0 ]; then
    summary="decide"
  else
    summary="no bump needed"
  fi
  if [ "${#shared[@]}" -gt 0 ]; then
    summary="$summary; read the shared diff: git diff $base HEAD -- ${shared[*]}"
  fi
  if [ "${#other[@]}" -gt 0 ]; then
    summary="$summary; classify by hand: ${other[*]}"
  fi
  summaries+=("$deliverable: $summary")
}

report app "$app_base"
report web "$web_base"
echo
printf '%s\n' "${summaries[@]}"
