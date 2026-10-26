# Android resources

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
