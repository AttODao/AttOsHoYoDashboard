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

## Home-screen widgets

原神とスターレイルの2つのホーム画面ウィジェットの基盤です。現在はゲーム名と状態を示すプレースホルダーで、各3×3セル（180dp、targetCellWidth/Height = 3）、縦横にリサイズできます。

- `kotlin/AhdWidgetData.kt`: Android依存のないモデル、RFC 3339パーサー、取得後の回復値の推定。
- `kotlin/AhdWidgets.kt`: SharedPreferences `ahd_widgets`、Rust JNIブリッジ、更新ジョブ、共通provider。
- `kotlin/GenshinWidget.kt` / `kotlin/StarRailWidget.kt`: ゲーム別の入口（後続実装で描画を追加）。
- `res/xml/ahd_widget_{genshin,starrail}_info.xml` / `res/layout/ahd_widget_{genshin,starrail}.xml`: サイズ・更新周期と、ゲームごとのRemoteViewsレイアウト（ピッカーのプレビューと初期表示を兼ねる。後続タスクはこのファイルを書き換えればよい）。`AndroidManifest.application.xml` にreceiverとジョブを登録。

Rustの `app/home_widgets.rs` はダッシュボードの変更ごとに選択中のアカウントと取得済みの状態を `mobile/widgets.rs` からpublishし、Kotlinが `ahd_widgets` に保存します。未取得でも同じアカウントのキャッシュは保持します。JobSchedulerは30分周期で状態APIを読み、`updatePeriodMillis` も30分です。401またはセッションを忘れるログアウトで保存を消去しSignedOutに戻します。通信失敗時は既存の状態に失敗フラグを付けて保持します。

### How to implement a game widget

1. 各ゲームのクラスで `render(context, state, nowMs)` をoverrideします。`when (state)` で `SignedOut`、`NoAccount`、`Unavailable`、`Ready` を扱い、未準備の状態では `placeholder(context, state)` を利用できます。
2. `Ready` の `state.snapshot.at(nowMs)` で樹脂・開拓力・宝銭を推定します。残り時間は `remainingSecondsAt(nowMs)`、派遣の完了数は `Expeditions.finishedAt(nowMs)` を使います。
3. RemoteViews対応のビューだけを使い、タップで `/dashboard` を開くためルートのidを `ahd_widget_root` にします。
4. Kotlinに `R.` 参照を追加せず、`context.resources.getIdentifier(name, type, context.packageName)` を使います。リソースはdxの最初のGradle実行の後にコピーされます。**MainActivity.ktから新しいKotlinファイルを参照してはいけません**（同じビルド順の制約）。
5. 純粋モデルのテストは `android/test/` に追加します。先に `build-android` を実行し、`devenv --profile android shell -- test-android` でJUnitを実行します。Kotlinとテストの変更だけなら再ビルドせずに再コピーしてテストできます。

JobSchedulerは不正確でDoze中は遅延します。取得の間の数値は推定で、サーバーの状態キャッシュTTLも適用されます。ウィジェットはダッシュボードで選択したアカウントに追従します。

### 動作確認手順（エミュレータ）

1. `AHD_ANDROID_TARGET=x86_64-linux-android` で `build-android` し、インストールする。ホーム画面の長押し →「ウィジェット」→ AHD に原神／スターレイルが各「3 × 3」で並び、プレビューにゲーム名が出ることを確認して配置する（未ログインなら「AHDを開いてログインしてください」）。
2. アプリでログインしダッシュボードを開くと、`run-as cc.attodao.ahd cat shared_prefs/ahd_widgets.xml` に `origin` と `game.genshin` / `game.starrail`（`kind":"ready"`）が入り、ウィジェットが「HH:MM に更新」になる。`dumpsys jobscheduler` に `AhdWidgetJob` が登録される。
3. サーバー側でキャッシュを更新してから `cmd jobscheduler run -f cc.attodao.ahd <job id>` で更新ジョブを強制実行すると、保存の `fetched_at` とウィジェットの時刻が変わる。
4. ウィジェットのタップでアプリのダッシュボードが開く。メニューからログアウトすると `ahd_widgets` が空になり、ジョブが解除され、ウィジェットは未ログイン表示に戻る。
