# Android resources and activity

`dx build --platform android` generates the Gradle project with Dioxus' default launcher
icon and the crate name as the app label. `build-android` (devenv `android` profile)
copies `res/` over the generated project and assembles the APK again, so the app shows
as "AHD" with the dashboard icon.

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
