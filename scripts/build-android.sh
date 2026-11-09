#!/usr/bin/env bash
# Android APK for the dashboard UI (`mobile` feature); run inside `devenv --profile android shell`.
# dx generates and assembles the Gradle project; then the AHD launcher/notification resources,
# label, theme and network security config (dashboard-ui/android/res) replace Dioxus' defaults, the
# extra Kotlin sources (dashboard-ui/android/kotlin) join MainActivity.kt,
# the activity keeps itself across configuration changes, the manifest receivers, the inbox
# job service, UnifiedPush receiver/service and distributor queries, update receiver and
# browser sign-in callbacks are inserted. Home-screen widget Kotlin
# sources, resources and JVM tests are copied before Gradle assembles again.
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
kotlin_src="$project/app/src/main/kotlin/dev/dioxus/main"
test_src="$project/app/src/test/java/dev/dioxus/main"
# dx rewrites its own resources but keeps extra files: drop the previous overlay first,
# or its PNG icons would duplicate dx's WebP ones when dx runs Gradle. The same goes for the
# extra Kotlin sources and JVM tests: dx compiles MainActivity before the overlay exists.
if [ -d "$res" ]; then
  (cd "$overlay" && find . -type f) | while read -r file; do rm -f "$res/$file"; done
fi
for source in "$kotlin_src" "$test_src"; do
  if [ -d "$source" ]; then
    for file in dashboard-ui/android/kotlin/*.kt dashboard-ui/android/test/*.kt; do
      rm -f "$source/$(basename "$file")"
    done
  fi
done
# dx adds the library for this target but keeps other ABIs' from earlier builds, which
# could carry another AHD_SERVER_URL: the APK must hold this build only.
rm -rf "$project/app/src/main/jniLibs"

export AHD_SERVER_URL
dx build --package attos-dashboard-ui --platform android --target "$target" "$@"

test -d "$res"
rm -f "$res"/mipmap-*/ic_launcher.webp
cp -R "$overlay/." "$res/"
# Kotlin next to dx's MainActivity.kt (widgets and indicators, dashboard-ui/android/README.md).
test -d "$kotlin_src"
cp dashboard-ui/android/kotlin/*.kt "$kotlin_src/"
mkdir -p "$test_src"
cp dashboard-ui/android/test/*.kt "$test_src/"
# tao/wry abort the process when the activity is destroyed or a second instance starts: one
# instance (singleTask), not recreated for dark mode, font size, display size, locale or
# keyboard changes (dashboard-ui/android/MainActivity.kt).
manifest="$project/app/src/main/AndroidManifest.xml"
config_changes='orientation|screenLayout|screenSize|smallestScreenSize|keyboard|keyboardHidden|navigation|uiMode|density|fontScale|fontWeightAdjustment|locale|layoutDirection|mcc|mnc|touchscreen|colorMode'
sed -i "s/android:configChanges=\"[^\"]*\"/android:configChanges=\"$config_changes\"/" "$manifest"
grep -qF "android:configChanges=\"$config_changes\"" "$manifest"
grep -q 'android:launchMode=' "$manifest" \
  || sed -i 's/<activity android:configChanges=/<activity android:launchMode="singleTask" android:configChanges=/' "$manifest"
grep -qF 'android:launchMode="singleTask"' "$manifest"
# The session cookie files never go into backups or device transfers (res/xml/ahd_*_rules.xml).
grep -q 'android:dataExtractionRules=' "$manifest" \
  || sed -i 's/<application android:hasCode=/<application android:fullBackupContent="@xml\/ahd_backup_rules" android:dataExtractionRules="@xml\/ahd_data_extraction_rules" android:hasCode=/' "$manifest"
grep -qF 'android:fullBackupContent="@xml/ahd_backup_rules"' "$manifest"
grep -qF 'android:dataExtractionRules="@xml/ahd_data_extraction_rules"' "$manifest"
if ! grep -q AhdInboxJob "$manifest"; then
  awk '
    /<\/application>/ {
      while ((getline line < "dashboard-ui/android/AndroidManifest.application.xml") > 0) print line
      close("dashboard-ui/android/AndroidManifest.application.xml")
    }
    { print }
  ' "$manifest" > "$manifest.tmp"
  mv "$manifest.tmp" "$manifest"
fi
# UnifiedPush distributor visibility sits next to <application>, not inside it.
if ! grep -q 'org.unifiedpush.android.distributor.REGISTER' "$manifest"; then
  awk '
    /<application/ && !inserted {
      while ((getline line < "dashboard-ui/android/AndroidManifest.queries.xml") > 0) print line
      close("dashboard-ui/android/AndroidManifest.queries.xml")
      inserted = 1
    }
    { print }
  ' "$manifest" > "$manifest.tmp"
  mv "$manifest.tmp" "$manifest"
fi
grep -q 'org.unifiedpush.android.distributor.REGISTER' "$manifest"
grep -q AhdPushReceiver "$manifest"
grep -q AhdBootReceiver "$manifest"
grep -q AhdInboxJob "$manifest"
grep -q GenshinWidget "$manifest"
grep -q StarRailWidget "$manifest"
grep -q AhdWidgetJob "$manifest"
grep -q AhdUpdateReceiver "$manifest"
if ! grep -qF 'android:scheme="cc.attodao.ahd"' "$manifest"; then
  awk '
    /<\/activity>/ && !inserted {
      while ((getline line < "dashboard-ui/android/AndroidManifest.activity.xml") > 0) print line
      close("dashboard-ui/android/AndroidManifest.activity.xml")
      inserted = 1
    }
    { print }
  ' "$manifest" > "$manifest.tmp"
  mv "$manifest.tmp" "$manifest"
fi
grep -qF 'android:scheme="cc.attodao.ahd"' "$manifest"
# The widget indicator samples are a debug-only screen.
if [ "$profile" = debug ] && ! grep -q AhdIndicatorSamplesActivity "$manifest"; then
  awk '
    /<\/application>/ {
      while ((getline line < "dashboard-ui/android/AndroidManifest.samples.xml") > 0) print line
      close("dashboard-ui/android/AndroidManifest.samples.xml")
    }
    { print }
  ' "$manifest" > "$manifest.tmp"
  mv "$manifest.tmp" "$manifest"
fi
if [ "$profile" = debug ]; then
  grep -q AhdIndicatorSamplesActivity "$manifest"
fi
# Android only installs an update when versionCode does not go down; dx always writes 1.
gradle="$project/app/build.gradle.kts"
if ! grep -qF '// AHD: JVM unit tests' "$gradle"; then
  printf '\n%s\n' \
    '// AHD: JVM unit tests for the home-screen widget model (scripts/test-android.sh)' \
    'dependencies {' \
    '    testImplementation("junit:junit:4.13.2")' \
    '    testImplementation("org.json:json:20240303")' \
    '}' >> "$gradle"
fi
version=$(sed -n 's/^[[:space:]]*versionName = "\([^"]*\)"[[:space:]]*$/\1/p' "$gradle")
if [[ ! "$version" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
  echo "Invalid Android versionName: $version (expected X.Y.Z)" >&2
  exit 1
fi
major=$((10#${BASH_REMATCH[1]}))
minor=$((10#${BASH_REMATCH[2]}))
patch=$((10#${BASH_REMATCH[3]}))
if ((minor >= 100 || patch >= 100)); then
  echo "Android versionName minor and patch must be below 100: $version" >&2
  exit 1
fi
code=$((major * 10000 + minor * 100 + patch))
sed -i "s/^\([[:space:]]*\)versionCode = [0-9][0-9]*[[:space:]]*$/\1versionCode = $code/" "$gradle"
grep -qF "versionCode = $code" "$gradle"
(cd "$project" && ./gradlew --quiet assembleDebug)

apk="$project/app/build/outputs/apk/debug/app-debug.apk"
test -f "$apk"
echo "APK: $DEVENV_ROOT/$apk"
