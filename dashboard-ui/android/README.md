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

原神とスターレイルの2つのホーム画面ウィジェットです。各3×3セル（180dp、targetCellWidth/Height = 3）、縦横にリサイズできます。

- `kotlin/AhdWidgetData.kt`: Android依存のないモデル、RFC 3339パーサー、取得後の回復値の推定。
- `kotlin/AhdWidgets.kt`: SharedPreferences `ahd_widgets`、Rust JNIブリッジ、更新ジョブ、共通provider。
- `kotlin/GenshinWidget.kt` / `kotlin/StarRailWidget.kt`: ゲーム別の描画（下記「Genshin widget」「Star Rail widget」）。
- `res/xml/ahd_widget_{genshin,starrail}_info.xml` / `res/layout/ahd_widget_{genshin,starrail}.xml`（狭い幅・大きい文字用は `_compact.xml`）: サイズ・更新周期と、ゲームごとのRemoteViewsレイアウト（通常レイアウトがピッカーのプレビューと初期表示を兼ねる）。`AndroidManifest.application.xml` にreceiverとジョブを登録。

Rustの `app/home_widgets.rs` はダッシュボードの変更ごとに選択中のアカウントと取得済みの状態を `mobile/widgets.rs` からpublishし、Kotlinが `ahd_widgets` に保存します。未取得でも同じアカウントのキャッシュは保持します。JobSchedulerは30分周期で状態APIを読み、`updatePeriodMillis` も30分です。401またはセッションを忘れるログアウトで保存を消去しSignedOutに戻します。通信失敗時は既存の状態に失敗フラグを付けて保持します。

### How to implement a game widget

1. 各ゲームのクラスで `render(context, state, nowMs)` をoverrideします。`when (state)` で `SignedOut`、`NoAccount`、`Unavailable`、`Ready` を扱い、未準備の状態では `placeholder(context, state)` を利用できます。
2. `Ready` の `state.snapshot.at(nowMs)` で樹脂・開拓力・宝銭を推定します。残り時間は `remainingSecondsAt(nowMs)`、派遣の完了数は `Expeditions.finishedAt(nowMs)` を使います。
3. RemoteViews対応のビューだけを使い、ルートのidを `ahd_widget_root` にします。タップは `/dashboard?game=<key>` を開き、アプリはそのゲームのタブを選びます（`widgetOpenTarget`、Rustの `app/navigation.rs` `target_game`）。大きさで描き分ける場合は `render(context, state, nowMs, options)` をoverrideします（`options` は `AppWidgetManager.OPTION_APPWIDGET_*` のdp値。リサイズ時にも再描画されます）。文字はspで端末の文字サイズに合わせて大きくなるため、レイアウトの選択には `context.resources.configuration.fontScale` も使います（`GenshinWidgetLayout` / `StarRailWidgetLayout.forSize`）。表示サイズ（密度）の変更はウィジェットのdpが変わるので即座に再描画されますが、文字サイズの変更を知らせるbroadcastはなく（manifestのreceiverでは `CONFIGURATION_CHANGED` を受け取れず、裏のプロセスは凍結される）、ランチャーは前回のRemoteViewsを読み直すだけです。新しい文字サイズでのレイアウト選択は次の描画（アプリを開く、30分ごとの更新ジョブ、`updatePeriodMillis`）からです。
   文言は `values/`（日本語、既定）と `values-en/` で、アプリの言語（`AhdLocale.localized`、`docs/android-changes.md` 5.7）で選びます。レイアウトに書いた文字列はランチャーがシステム言語で解決するため、表示する文言はすべてコードから入れます。面の色は `values/`・`values-night/` の `ahd_widget_*` です。
4. Kotlinに `R.` 参照を追加せず、`context.resources.getIdentifier(name, type, context.packageName)` を使います。リソースはdxの最初のGradle実行の後にコピーされます。**MainActivity.ktから新しいKotlinファイルを参照してはいけません**（同じビルド順の制約）。
5. 純粋モデルのテストは `android/test/` に追加します。先に `build-android` を実行し、`devenv --profile android shell -- test-android` でJUnitを実行します。Kotlinとテストの変更だけなら再ビルドせずに再コピーしてテストできます。

JobSchedulerは不正確でDoze中は遅延します。取得の間の数値は推定で、サーバーの状態キャッシュTTLも適用されます。ウィジェットはダッシュボードで選択したアカウントに追従します。

### Genshin widget

`kotlin/GenshinWidget.kt` が描画し、表示する値と色の判断は `kotlin/GenshinWidgetModel.kt`（Android非依存、`test/GenshinWidgetModelTest.kt`）にあります。

- 通常（`layout/ahd_widget_genshin.xml`、幅210dp・高さ250dp以上。ピッカーのプレビューと初期表示も兼ねる）: 見出し（原神と取得時刻）、上段に天然樹脂の88dpの円弧（中央に現在値）と「120/200・全回復まで・3時間12分」、下段に洞天宝銭・探索派遣・デイリー任務の56dpの円弧（中央はアイコン）と名前・値・補足を3列で並べます。補足は宝銭が「あと …」（満タンまで）、派遣が「最短 …」「最長 …」、デイリーが追加報酬の受取状況です。
- それより小さいとき（`layout/ahd_widget_genshin_compact.xml`）は、同じ4項目をバーで縦に並べます（補足は樹脂の「全回復まで …」だけ。デイリーは値の横に「受取済」／「未受取」）。大きさは `OPTION_APPWIDGET_MIN_WIDTH` と `OPTION_APPWIDGET_MAX_HEIGHT`（縦向きの大きさ）で選び、不明なら通常です。文字サイズが1より大きいときは、必要な幅・高さを文字サイズ倍にします（3×3の約270×343dpでは文字115%まで通常、130%以上はバー。文字200%で樹脂の残り時間が下で切れていたため）。
- 色: 面はダッシュボードの原神カードの色（`values(-night)/ahd_widget_genshin_colors.xml`）、ゲージは `ahd_indicator_genshin`（ミント）。樹脂・宝銭は90%以上と上限でwarning色になり、上限では値と補足もwarning色です。派遣は全員帰還（受取待ち）で、デイリーは4/4で追加報酬が未受取のときにwarning色にし、受取済みの4/4はミントのままです。
- 状態: 未ログイン・アカウントなし・未取得は、アイコンと案内（「AHDを開いてログインしてください」など）と「タップしてAHDを開く」を表示します。取得済みで最後の更新に失敗したときは、値はそのまま見出しの時刻を「更新失敗 HH:MM」のwarning色にします。
- 値は描画時刻に合わせて推定します（`snapshot.at(nowMs)`）。描画はアプリのpublish、30分ごとの更新ジョブ、`updatePeriodMillis`（30分）、リサイズのときです。
- 残り時間は「3時間12分」、1時間未満は「10分」です（`AhdIndicators.hoursMinutes`。スターレイルも同じ）。

### Star Rail widget

`kotlin/StarRailWidget.kt` renders `layout/ahd_widget_starrail.xml`: a Trailblaze Power arc (value, `/max`) with "全回復まで" and the time to full, the reserve and the fetch time beside it, then full-width bars for assignments (done/sent and time left on the value line), daily training and Simulated Universe weekly points (the realm-currency slot; reasons in `docs/android-changes.md`). The values come from the Android-free `kotlin/StarRailWidgetModel.kt` (`test/StarRailWidgetModelTest.kt`). Lines that can turn to the warning color exist twice in the layout (`name` / `name_full`) so the colors stay resources that follow dark mode. Strings are `values(-en)/ahd_widget_starrail_strings.xml` (Japanese default) and are set from code in the app's language (`AhdLocale`; layout strings are resolved by the launcher in the system locale, which would ignore the app's language); colors are `values(-night)/ahd_widget_starrail_colors.xml`.

When the time beside the arc would not fit, `StarRailWidgetLayout.forSize` (width ≥ 142dp + 90dp × font scale, height ≥ 148dp + 98dp × font scale; unknown size = regular) picks `layout/ahd_widget_starrail_compact.xml`: four bars (Trailblaze Power with "全回復まで 9時間00分" as its caption, reserve and fetch time on the next line, assignments with their time as the caption) and a one-line title that shrinks to fit. A 3×3 widget is compact at density 560 (about 200dp wide) and from font size about 150% at density 420. The fetch time and the message box (icon, text, "タップしてAHDを開く") use the same wording as the Genshin widget ("3:31 更新", "更新失敗 3:40").

### 動作確認手順（エミュレータ）

1. `AHD_ANDROID_TARGET=x86_64-linux-android` で `build-android` し、インストールする。ホーム画面の長押し →「ウィジェット」→ AHD に原神／スターレイルが各「3 × 3」で並び、プレビューにゲーム名が出ることを確認して配置する（未ログインなら「AHDを開いてログインしてください」）。
2. アプリでログインしダッシュボードを開くと、`run-as cc.attodao.ahd cat shared_prefs/ahd_widgets.xml` に `origin` と `game.genshin` / `game.starrail`（`kind":"ready"`）が入り、ウィジェットが「HH:MM 更新」になる。`dumpsys jobscheduler` に `AhdWidgetJob` が登録される。
3. サーバー側でキャッシュを更新してから `cmd jobscheduler run -f cc.attodao.ahd <job id>` で更新ジョブを強制実行すると、保存の `fetched_at` とウィジェットの時刻が変わる。
4. ウィジェットのタップでアプリのダッシュボードが開く。メニューからログアウトすると `ahd_widgets` が空になり、ジョブが解除され、ウィジェットは未ログイン表示に戻る。

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
- **Text**: `hoursMinutes` gives `3時間12分` / `3h 12m`, or `10分` / `10m` under an hour (rounded up like the dashboard) in
  the context's locale; Japanese is the default resource like the app, English is
  `values-en`. The widgets pass `AhdLocale.localized(context)`, a configuration context in the
  app's language (the language saved in the app, else the per-app or system locale by the
  rule of Rust `Language::for_locales`).
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
