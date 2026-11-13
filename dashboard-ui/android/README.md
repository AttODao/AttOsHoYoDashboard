# Android resources and activity

`dx build --platform android` generates the Gradle project with Dioxus' default launcher
icon and the crate name as the app label. `build-android` (devenv `android` profile)
copies `res/` over the generated project and assembles the APK again, so the app shows
as "AHD" with the dashboard icon.

`res/drawable/ic_stat_ahd.xml` is the white bell notification icon. Dioxus copies
`MainActivity.kt` for the permission/JNI bridge, alarms and notification receivers;
`build-android` inserts `AndroidManifest.application.xml` into the generated manifest
so alarms and boot/package-update restoration work in the final APK.

`MainActivity.kt` also holds `AhdSessionStore`, which seals the Rust client's session
cookie jar with an Android Keystore AES-GCM key (`files/session-cookies.enc`) and migrates
the plain `session-cookies.json` of earlier versions; its Android-free format and decisions
(`AhdSessionFormat`) are tested in `test/AhdSessionFormatTest.kt`. `res/xml/ahd_backup_rules.xml`
and `res/xml/ahd_data_extraction_rules.xml` keep both files out of backups and device
transfers; `build-android` adds them to the manifest's `<application>`
(`docs/android-changes.md` 5.7).

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

原神とスターレイルの2つのホーム画面ウィジェットです。各3×3セル（180dp、targetCellWidth/Height = 3）、縦横にリサイズできます（下限は180dp、スターレイルの高さだけ200dp）。

- `kotlin/AhdWidgetData.kt`: Android依存のないモデル、RFC 3339パーサー、取得後の回復値の推定。
- `kotlin/AhdWidgets.kt`: SharedPreferences `ahd_widgets`、Rust JNIブリッジ、更新ジョブ、共通provider。
- `kotlin/GenshinWidget.kt` / `kotlin/StarRailWidget.kt`: ゲーム別のprovider。表示する値は `GenshinWidgetModel.kt` / `StarRailWidgetModel.kt`、描画は両ゲーム共通の `AhdWidgetFrame.kt` と `AhdIndicators.kt`（下記「Widget layout」「Widget parts」）。
- `res/xml/ahd_widget_{genshin,starrail}_info.xml` / `res/layout/ahd_widget_{genshin,starrail}.xml`（狭い幅・大きい文字用は `_compact.xml`、ピッカーのプレビューは `_preview.xml`）: サイズ・更新周期と、ゲームごとのRemoteViewsレイアウト（通常レイアウトが初期表示を兼ねる）。`AndroidManifest.application.xml` にreceiverとジョブを登録。

Rustの `app/home_widgets.rs` はダッシュボードの変更ごとに選択中のアカウントと取得済みの状態を `mobile/widgets.rs` からpublishし、Kotlinが `ahd_widgets` に保存します。未取得でも同じアカウントのキャッシュは保持します。JobSchedulerは30分周期で状態APIを読み、`updatePeriodMillis` も30分です。401またはセッションを忘れるログアウトで保存を消去しSignedOutに戻します。通信失敗時は既存の状態に失敗フラグを付けて保持します。

### How to implement a game widget

1. 各ゲームのクラスで `render(context, state, nowMs, options)` をoverrideし、`guarded(context) { … }` の中で `AhdWidgetFrame.render` を呼びます。`SignedOut`、`NoAccount`、`Unavailable` の案内は共通で、`Ready` の内容（`AhdWidgetContent`：主リソースとタイル）だけをゲームのモデルが作ります。描画で例外が起きると `placeholder(context)`（「状態を取得できませんでした…」だけの表示）に戻ります。
2. `Ready` の `state.snapshot.at(nowMs)` で樹脂・開拓力・宝銭を推定します。残り時間は `remainingSecondsAt(nowMs)`、派遣の完了数は `Expeditions.finishedAt(nowMs)` を使います。
3. RemoteViews対応のビューだけを使い、ルートのidを `ahd_widget_root` にします。タップは `/dashboard?game=<key>` を開き、アプリはそのゲームのタブを選びます（`widgetOpenTarget`、Rustの `app/navigation.rs` `target_game`）。大きさで描き分ける場合は `render(context, state, nowMs, options)` をoverrideします（`options` は `AppWidgetManager.OPTION_APPWIDGET_*` のdp値。リサイズ時にも再描画されます）。文字はspで端末の文字サイズに合わせて大きくなるため、レイアウトの選択には `context.resources.configuration.fontScale` も使います（`GenshinWidgetLayout` / `StarRailWidgetLayout.forSize`）。表示サイズ（密度）の変更はウィジェットのdpが変わるので即座に再描画されますが、文字サイズの変更を知らせるbroadcastはなく（manifestのreceiverでは `CONFIGURATION_CHANGED` を受け取れず、裏のプロセスは凍結される）、ランチャーは前回のRemoteViewsを読み直すだけです。新しい文字サイズでのレイアウト選択は次の描画（アプリを開く、30分ごとの更新ジョブ、`updatePeriodMillis`）からです。
   文言は `values/`（日本語、既定）と `values-en/` で、アプリの言語（`AhdLocale.localized`、`docs/android-changes.md` 5.10）で選びます。レイアウトに書いた文字列はランチャーがシステム言語で解決するため、表示する文言はすべてコードから入れます。面の色は `values/`・`values-night/` の `ahd_widget_*` です。
4. Kotlinに `R.` 参照を追加せず、`context.resources.getIdentifier(name, type, context.packageName)` を使います。リソースはdxの最初のGradle実行の後にコピーされます。**MainActivity.ktから新しいKotlinファイルを参照してはいけません**（同じビルド順の制約）。
5. 純粋モデルのテストは `android/test/` に追加します。先に `build-android` を実行し、`devenv --profile android shell -- test-android` でJUnitを実行します。Kotlinとテストの変更だけなら再ビルドせずに再コピーしてテストできます。

JobSchedulerは不正確でDoze中は遅延します。取得の間の数値は推定で、サーバーの状態キャッシュTTLも適用されます。ウィジェットはダッシュボードで選択したアカウントに追従します。

### Widget layout

両ウィジェットは同じ組み立てです（配置と状態の一覧は `docs/android-changes.md` の「ウィジェットの表示」、承認されたモックは kanban t_fdaa1ff9 のD案）。

- 最上位は `layout/ahd_widget_<game>.xml`（通常）と `_compact.xml`。パネルの色とゲームのテーマ（`android:theme="@style/AhdGameGenshin"` / `AhdGameStarRail`）を持ち、共通の見出し（`ahd_widget_header.xml` / `_compact.xml`）と本文（`ahd_widget_body.xml` / `_compact.xml`）をincludeします。
- 本文は、案内（`ahd_widget_message_box`：最初の更新まで、未ログイン、アカウントなし、未取得）か内容（`ahd_widget_content`）のどちらかを表示します。内容は主リソースの円弧と残り時間、その他の値を入れる枠（`ahd_widget_slot_1..4`。通常は2列の `ahd_widget_tiles.xml`、スターレイルは5:6の `ahd_widget_tiles_wide.xml` の `ahd_widget_wide_slot_1..4`）です。
- 大きさは `OPTION_APPWIDGET_MIN_WIDTH` と `OPTION_APPWIDGET_MAX_HEIGHT`（縦向きの大きさ）で選びます：幅210dp・高さ250dp（×文字サイズ）以上なら通常、小さければコンパクト、不明なら通常（`AhdWidgetLayout.forSize`、両ゲーム共通）。
- 値は描画時刻に合わせて推定します（`snapshot.at(nowMs)`）。描画はアプリのpublish、30分ごとの更新ジョブ、`updatePeriodMillis`（30分）、リサイズのときです。残り時間は「3時間12分」、1時間未満は「10分」です（`AhdIndicators.hoursMinutes`）。
- ゲームごとの違いはモデルだけです。原神（`GenshinWidgetModel.content`）：天然樹脂、洞天宝銭（円弧）、デイリー任務（丸＋追加依頼報酬の記号）、探索派遣（丸＋最短〜最長）。スターレイル（`StarRailWidgetModel.content`）：開拓力、予備開拓力、依頼（丸＋残り時間）、デイリー訓練と模擬宇宙（円弧、達成はチェック）。
- ピッカーのプレビュー（`ahd_widget_<game>_preview.xml`）は静的な作り例です。本文やタイルのレイアウトを変えたら合わせて直してください。

### 動作確認手順（エミュレータ）

1. `AHD_ANDROID_TARGET=x86_64-linux-android` で `build-android` し、インストールする。ホーム画面の長押し →「ウィジェット」→ AHD に原神／スターレイルが各「3 × 3」で並び、プレビューに作り例が出ることを確認して配置する（未ログインなら「AHDを開いてログインしてください」）。
2. アプリでログインしダッシュボードを開くと、`run-as cc.attodao.ahd cat shared_prefs/ahd_widgets.xml` に `origin` と `game.genshin` / `game.starrail`（`kind":"ready"`）が入り、ウィジェットの見出しに時計と取得時刻が出る。`dumpsys jobscheduler` に `AhdWidgetJob` が登録される。
3. サーバー側でキャッシュを更新してから `cmd jobscheduler run -f cc.attodao.ahd <job id>` で更新ジョブを強制実行すると、保存の `fetched_at` とウィジェットの時刻が変わる。
4. ウィジェットのタップでアプリのダッシュボードが開く。メニューからログアウトすると `ahd_widgets` が空になり、ジョブが解除され、ウィジェットは未ログイン表示に戻る。

## Widget parts

`build-android` copies `kotlin/*.kt` next to dx's `MainActivity.kt` (package
`dev.dioxus.main`). What a widget shows is plain data built without Android
(`kotlin/AhdIndicatorModel.kt`); two files turn it into `RemoteViews`:

```kotlin
// In the game's provider (GenshinWidget.kt / StarRailWidget.kt):
AhdWidgetFrame.render(context, game, layout.layout, layout.kind, widthDp, heightDp, state, nowMs) { ready ->
    AhdWidgetContent(
        main = AhdMain(AhdGlyph.RESIN, "ahd_widget_genshin_resin", current = 124, max = 200, secondsToFull = 36_480),
        tiles = listOf(
            AhdTile.gauge(AhdGlyph.COIN, "ahd_widget_genshin_coin", 1320, 2400, secondsToFull = 126_720),
            AhdTile(AhdGlyph.DAILY, "ahd_widget_genshin_daily", 3, 4, pips = AhdIndicatorModel.pips(3, 4),
                detail = AhdDetail.Reward(AhdMark.UNCLAIMED)),
        ),
        fetchedAtMs = ready.snapshot.fetchedAtMs, refreshFailed = ready.refreshFailed,
    )
}
```

- **Main resource** (`AhdMain`): the arc (96dp, compact 60dp) with the icon and number
  inside and `/max` in its open bottom; beside it hourglass + time left and the clock time,
  the cap badge from 90% (`AhdIndicatorModel.fillLevel`, the dashboard's rule), or the unknown
  symbol + "no data".
- **Tiles** (`AhdTile`, drawn by `AhdIndicators.tile` into `layout/ahd_indicator_tile.xml`, or
  the one-line `ahd_indicator_row.xml` in the compact layout): an icon with `value/max`;
  `ring` puts the icon inside a small arc (a share: `AhdTile.gauge` warns near and at the cap,
  `AhdTile.score` marks a reached goal as done); `pips` draws up to five dots for a count; and
  one `AhdDetail`: `Time` (hourglass), `Range` (two times, only where they fit), `AtCap`,
  `Status` (a status symbol with its word), `Reward` (gift + status symbol) or `Label` (a
  short name). `AhdTile.unknown` shows `—` and the unknown mark, never a number.
- **Icons**: `AhdGlyph` names `drawable/ahd_indicator_icon_<key>.xml`; each is the path of the
  same icon in `dashboard-ui/src/pages/icons.rs`. Add an icon there first, then here (the JVM
  check compares the paths and rejects icons outside the set). The game marks are
  `drawable/ahd_widget_mark_<game>.xml`, one file per game.
- **Colors**: the game's colors, mark and name are theme attributes set by
  `AhdGameGenshin` / `AhdGameStarRail` on every layout root (`values/ahd_indicator_styles.xml`,
  `ahd_indicator_attrs.xml`); a tile added from code gets them through
  `ahd_indicator_(tile|row)_<game>.xml`. Warning, done, text and track colors are
  `values(-night)/ahd_indicator_colors.xml`, the panel colors
  `ahd_widget_<game>_colors.xml` (light/dark from `dashboard.css`). Everything is applied by
  `android:tint` / `android:textColor`, so it follows dark mode when the launcher re-inflates
  the widget. A part whose color depends on the state exists once per color and the code
  shows one.
- **Arcs**: `drawable/ahd_indicator_arc_<dp>.xml` is a ring whose sweep is the drawable level
  (`ImageView.setImageLevel`, 0..`AhdIndicatorModel.ARC_LEVEL`) with round ends;
  `ahd_indicator_arc_track_<dp>.xml` is its track. RemoteViews cannot host custom views, and
  ProgressBar colors are settable only from API 31; a Canvas bitmap would freeze its colors
  (no dark-mode switch) and its density.
- **Text**: all texts are set from code in the app's language (`AhdLocale.localized(context)`;
  Japanese is the default resource like the app, English is `values-en`). The tile texts and
  the small times are sized from code: the system font size, capped by what fits the widget
  (`AhdIndicatorModel.fit`); the title and the time left shrink with autoSize.
- **Spoken text**: `AhdWidgetFrame` sets one content description on the widget root (game,
  main resource, every tile with its full name and status words, fetch time); the parts are
  hidden from accessibility.

Samples: debug APKs contain `AhdIndicatorSamplesActivity` (both widgets in every state,
regular and compact, light and dark, rendered through `RemoteViews.apply`):

```sh
adb shell am start -n cc.attodao.ahd/dev.dioxus.main.AhdIndicatorSamplesActivity \
  --ez night true --es game genshin --ei first 0 --ei count 3   # all extras are optional
adb shell wm density 320; adb shell settings put system font_scale 2.0   # more room / large font
```

Rules and resources are checked on the JVM with
`devenv --profile android shell -- test-android-kotlin` (`tests/AhdIndicatorModelTest.kt`).
