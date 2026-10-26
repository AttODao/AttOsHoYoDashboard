#!/usr/bin/env bash
# Android APK for the dashboard UI (`mobile` feature); run inside `devenv --profile android shell`.
# dx generates and assembles the Gradle project; then the AHD launcher icon and label
# (dashboard-ui/android/res) replace Dioxus' defaults and Gradle assembles again.
set -euo pipefail
cd "${DEVENV_ROOT:?run inside the devenv android profile}"

: "${AHD_SERVER_URL:?AHD_SERVER_URL is required, e.g. AHD_SERVER_URL=https://ahd.example.com}"
target="${AHD_ANDROID_TARGET:-aarch64-linux-android}"
profile=debug
for argument in "$@"; do
  if [ "$argument" = --release ]; then
    profile=release
  fi
done

project="target/dx/attos-dashboard-ui/$profile/android/app"
res="$project/app/src/main/res"
overlay=dashboard-ui/android/res
# dx rewrites its own resources but keeps extra files: drop the previous overlay first,
# or its PNG icons would duplicate dx's WebP ones when dx runs Gradle.
if [ -d "$res" ]; then
  (cd "$overlay" && find . -type f) | while read -r file; do rm -f "$res/$file"; done
fi
# dx adds the library for this target but keeps other ABIs' from earlier builds, which
# could carry another AHD_SERVER_URL: the APK must hold this build only.
rm -rf "$project/app/src/main/jniLibs"

export AHD_SERVER_URL
dx build --package attos-dashboard-ui --platform android --target "$target" "$@"

test -d "$res"
rm -f "$res"/mipmap-*/ic_launcher.webp
cp -R "$overlay/." "$res/"
(cd "$project" && ./gradlew --quiet assembleDebug)

apk="$project/app/build/outputs/apk/debug/app-debug.apk"
test -f "$apk"
echo "APK: $DEVENV_ROOT/$apk"
