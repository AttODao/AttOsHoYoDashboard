# Android resources and activity

`dx build --platform android` generates the Gradle project with Dioxus' default launcher
icon and the crate name as the app label. `build-android` (devenv `android` profile)
copies `res/` over the generated project and assembles the APK again, so the app shows
as "AHD" with the dashboard icon.

`res/drawable/ic_stat_ahd.xml` is the white bell notification icon. Dioxus copies
`MainActivity.kt` for the permission/JNI bridge, alarms and notification receivers;
`build-android` inserts `AndroidManifest.application.xml` into the generated manifest
so alarms and boot/package-update restoration work in the final APK.

The PNGs are generated from `../assets/dashboard-icon.png` (legacy icons on the app
canvas color, adaptive-icon foregrounds at 108dp with the star inside the safe zone):

```sh
nix shell nixpkgs#imagemagick -c bash -c '
for d in mdpi:1 hdpi:1.5 xhdpi:2 xxhdpi:3 xxxhdpi:4; do
  n=${d%%:*}; s=${d##*:}; mkdir -p res/mipmap-$n
  fg=$(awk "BEGIN{print int(108*$s)}"); star=$(awk "BEGIN{print int(64*$s)}")
  magick -size ${fg}x${fg} xc:none \( ../assets/dashboard-icon.png -resize ${star}x${star} \) -gravity center -composite -strip res/mipmap-$n/ic_launcher_foreground.png
  lg=$(awk "BEGIN{print int(48*$s)}"); ls=$(awk "BEGIN{print int(38*$s)}")
  magick -size ${lg}x${lg} xc:"#0C0B0E" \( ../assets/dashboard-icon.png -resize ${ls}x${ls} \) -gravity center -composite -strip res/mipmap-$n/ic_launcher.png
done'
```

## Activity lifecycle

tao/wry cannot recreate their native side: once the Activity is destroyed the Rust event
loop ends and the process aborts (`FORTIFY: pthread_mutex_lock called on a destroyed
mutex`), and a second Activity instance panics in `create`. `MainActivity.kt` (used by dx
through `android_main_activity` in `Dioxus.toml`) therefore moves the task to the
background when Back reaches the first WebView history entry, re-applies the system font
size to the WebView on configuration changes, and kills its own process in `onDestroy`
(task swiped away from recents, unhandled changes), so the next launch starts cleanly.
`build-android` sets `launchMode="singleTask"` (every launch intent reaches the one
instance) and widens the manifest's `configChanges` so dark mode, font size or weight,
display size, locale and keyboard changes do not recreate the Activity, and
`res/values/styles.xml` uses a DayNight theme so `prefers-color-scheme` follows the
system.

## Widget indicators

`kotlin/AhdIndicators.kt` provides two RemoteViews components for home-screen widgets
(`build-android` copies `kotlin/*.kt` next to dx's `MainActivity.kt`, package
`dev.dioxus.main`). Each call returns a `RemoteViews` to add into a container of the widget
layout:

```kotlin
views.removeAllViews(slot)   // slot: a FrameLayout/LinearLayout id in the widget layout
views.addView(slot, AhdIndicators.bar(context, AhdBarSpec(
    label = "Original Resin", current = 120, max = 200, theme = AhdIndicatorTheme.GENSHIN,
    caption = AhdIndicators.hoursMinutes(context, secondsToFull))))
views.addView(slot, AhdIndicators.arc(context, AhdArcSpec(
    current = 120, max = 200, theme = AhdIndicatorTheme.GENSHIN, size = AhdArcSize.LARGE,
    caption = AhdIndicators.hoursMinutes(context, secondsToFull))))
```

- **Bar** (`layout/ahd_indicator_bar.xml`, full width): label and value text (`120/200`,
  or `valueText`) on one line, the bar below, an optional `caption` under it.
- **Arc** (`layout/ahd_indicator_arc.xml` 88dp, `_small.xml` 56dp): a 270° gauge with the
  value (or `centerText`) in the center, or an `icon` instead, and an optional `caption` in
  the open bottom (e.g. the time to full). Bundled icons: `AhdIndicators.icon(context,
  "ahd_indicator_icon_resin" | "_power" | "_coin" | "_expedition" | "_daily")`; any drawable
  id works. Set `contentDescription` when the center is an icon.
- **States**: `current = null` (or no positive `max`) shows an empty track and `—`; 0 shows
  an empty track; from 90% (`nearFullPercent`) the fill uses the near-full color, at the cap
  the full color and the value text turns to the full color (the dashboard's rule,
  `AhdIndicatorModel.fillLevel`). `level = AhdFillLevel.FULL` forces a state, e.g. when every
  expedition is done.
- **Colors**: `theme` picks the game accent (Genshin mint, Star Rail lavender). All colors are
  `res/values(-night)/ahd_indicator_colors.xml` (light/dark from `dashboard.css`) applied by
  `android:tint`, so they follow dark mode when the launcher re-inflates the widget. Change
  them there; `fillColor` overrides the fill with a fixed ARGB color (not dark-mode aware).
  The indicators draw no background: put them on the widget's own panel.
- **Text**: `hoursMinutes` gives `3時間12分` / `3h 12m` (rounded up like the dashboard) in
  the context's locale; Japanese is the default resource like the app, English is
  `values-en`. To use the app's language instead of the system one, pass
  `context.createConfigurationContext(config)` with that locale.
- **Why level drawables**: RemoteViews cannot host custom views, and ProgressBar colors are
  settable only from API 31; a Canvas bitmap would freeze its colors (no dark-mode switch)
  and its density. See the header of `AhdIndicators.kt`.

Samples: debug APKs contain `AhdIndicatorSamplesActivity` (all states: no value, 0%, 50%,
90%, 100% for both games, light and dark side by side, rendered through `RemoteViews.apply`):

```sh
adb shell am start -n cc.attodao.ahd/dev.dioxus.main.AhdIndicatorSamplesActivity
adb shell wm density 160; adb shell settings put system font_scale 2.0   # small density / large font
```

Rules and layout ids are checked on the JVM with
`devenv --profile android shell -- test-android-kotlin` (`tests/AhdIndicatorModelTest.kt`).
