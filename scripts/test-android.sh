#!/usr/bin/env bash
# JVM tests for the Android widget model; run inside the devenv android profile.
set -euo pipefail
cd "${DEVENV_ROOT:?run inside the devenv android profile}"
project=target/dx/attos-dashboard-ui/debug/android/app
kotlin_src="$project/app/src/main/kotlin/dev/dioxus/main"
test_src="$project/app/src/test/java/dev/dioxus/main"
if [ ! -d "$test_src" ]; then
  echo "Android JVM test project missing: run build-android first" >&2
  exit 1
fi
cp dashboard-ui/android/kotlin/*.kt "$kotlin_src/"
cp dashboard-ui/android/test/*.kt "$test_src/"
(cd "$project" && ./gradlew --quiet testDebugUnitTest)
