# Androidアプリ：web版からの変更点

`dashboard-ui` をAndroidアプリとしてビルドできるようにした際に、web版から変えた点をまとめます。利用者向けの違いは1〜5節と7節、開発者向けの詳細は6節にあります。ビルド手順の正本は [README](../README.md) の「Androidアプリ」節です。本書はその要約と、変更の理由・範囲を説明します。

## 1. 概要

### 方針

- 新しいフロントエンドは作りません。web版と同じDioxus（RSX）コード、同じ画面構成、同じ文言カタログ、同じ `dashboard.css` をそのまま使います。
- アプリ固有の処理はCargo feature `mobile` の中に閉じ込めます。web版は従来どおり既定の `web` feature でビルドし、アプリのコードはwasmに入りません。
- サーバー（axum）とAPI契約は変えていません。アプリは既存の `/api/v1` を、ブラウザと同じセッションCookie認証で呼びます。
- ブラウザでしか動かない機能（WebAuthn、Web Push）と、アプリに不要な機能（管理者機能など。3節）は、アプリで表示も遷移もしません。必要なときは「ブラウザで開く」（5.1節）でweb版に移ります。

### 使用したDioxusの仕組み

| 仕組み | 使い方 |
| --- | --- |
| `dioxus` の `mobile` feature（0.7.10） | `mobile = ["dioxus/mobile", …]`。Android ActivityのWebView上でRSXを描画する（wry/tao）。web版は `web = ["dioxus/web"]`（既定） |
| `dx build --platform android` | Gradleプロジェクトの生成とAPKのビルド。`Dioxus.toml` の `[bundle]`・`[android]`・`[android.permissions]` を使う |
| `Dioxus.toml` の `[application] android_main_activity` | 独自の `MainActivity.kt`（`WryActivity` を継承）をそのままコピーさせる。Activityの寿命の扱い（7節）と、通知の権限要求・アラーム・receiverをここに置く |
| `document::eval` | web版がweb-sysで行うDOM操作（フォーカス、`<dialog>`、`matchMedia`、History API、テーマ適用、`visibilitychange`）を、アプリではJavaScriptの評価で行う（`mobile/dom.rs`） |
| `asset!` / `document::Stylesheet` / `document::Meta` | アプリ専用のCSSとviewportを `cfg(feature = "mobile")` の中だけで読み込む（`pages/app_nav.rs` の `AppHead`） |
| dioxus mobileの外部リンク処理 | `<a href="https://…" target="_blank">` のクリックを既定のブラウザへ送る。HoYoLABリンクは追加のコードなしで動く |

アプリかどうかは `crate::platform::APP`（`cfg!(feature = "mobile")` の定数）で判定します。

### サーバーとの通信

WebViewのページからはサーバーのHttpOnly Cookieを扱えないため、アプリはRustから直接サーバーへ接続します（`mobile/http.rs`、reqwest + rustls）。

- 接続先はビルド時の `AHD_SERVER_URL` に固定します。HTTPS、またはlocalhostのHTTPだけを受け付け、パスやクエリ、認証情報は付けられません（`mobile/session.rs::parse_origin`）。
- ログイン応答の `Set-Cookie` をアプリ専用領域の `session-cookies.json`（`getFilesDir()`、パーミッション0600、原子的に書き換え）へ保存し、以後の要求に `Cookie` を付けます。ログアウトや期限切れで消えます。
- サーバーがCSRF対策で検査する `Origin` には、サーバー自身のOriginを送ります。リダイレクトは追いません（Cookieと `Origin` を別のホストへ送らないため）。
- User-Agentは `AHD-Android/<バージョン> (Android)` です。セッション一覧では「Android」と表示されます。
- パス検査、応答サイズの上限、エラーコードから表示文への変換は、web版と同じ `api.rs` の処理を使います。

## 2. ビルド・実行手順

必要なもの：devenvの `android` プロファイル（Android SDK 34、build-tools 34.0.0、NDK、JDK 17。初回は数GBを取得し、Android SDKのライセンスに同意したものとして扱います）と、Gradleが依存を取得するためのネットワーク。Rustの `aarch64-linux-android`／`x86_64-linux-android` targetは `rust-toolchain.toml` で入ります。既定の `devenv shell` にはSDKが入りません。

```sh
# 実機（arm64）向けAPK。AHD_SERVER_URL はHTTPSのOrigin（パスなし）。必須
AHD_SERVER_URL=https://ahd.example.com devenv --profile android shell -- build-android
# x86_64エミュレータ向け
AHD_SERVER_URL=https://ahd.example.com AHD_ANDROID_TARGET=x86_64-linux-android devenv --profile android shell -- build-android
# 静的検査（clippy、mobile feature、aarch64-linux-android）
devenv --profile android shell -- check-android
# 導入
adb install -r target/dx/attos-dashboard-ui/debug/android/app/app/build/outputs/apk/debug/app-debug.apk
```

- `build-android`（`scripts/build-android.sh`）は次の順に処理します。
  1. 前回上書きしたリソースと `jniLibs` を消す（重複した `ic_launcher` でGradleが失敗すること、別ABI・別サーバーURLの古いライブラリがAPKに残ることを防ぐ）。
  2. `dx build --platform android` を実行する。
  3. `dashboard-ui/android/res/`（ランチャーアイコン、通知アイコン、アプリ名「AHD」）で生成プロジェクトを上書きする。
  4. `dashboard-ui/android/AndroidManifest.application.xml` のreceiverを、生成された `AndroidManifest.xml` の `<application>` に挿入する。
  5. Gradleで組み直す。
- APKはデバッグ署名です。`--release` を付けても、署名設定がなければデバッグ署名になります。
- パッケージ名は `cc.attodao.ahd`、minSdk 24、targetSdk 34です。権限は、dxが既定で付けるINTERNETに加えて、`POST_NOTIFICATIONS` と `RECEIVE_BOOT_COMPLETED` を使います（`Dioxus.toml`）。
- 開発用サーバーにつなぐ場合：`AHD_SERVER_URL` を省略すると `http://localhost:8080` になります。`adb reverse tcp:8080 tcp:8080` でホストの開発サーバー（`PUBLIC_ORIGIN=http://localhost:8080`）へつなぎます。ただし `build-android` では `AHD_SERVER_URL` が必須なので、`AHD_SERVER_URL=http://localhost:8080` と明示します。
- エミュレータは `devenv --profile android --profile android-emulator shell` で入ります（x86_64イメージ、KVMが必要）。
- `dx serve --platform android` でも起動できますが、アイコンと名前がDioxusの既定のままで、通知用のreceiverも入りません。通知を確認するときは `build-android` のAPKを使います。

## 3. Android版で除外した機能と除外方法

除外は「アプリでは画面へ遷移しない」と「操作部品を描画しない」の2段で行います。どちらも `crate::platform::APP` で分岐し、web版の挙動は変えていません。サーバー側のAPIはそのまま残っているため、ブラウザでは従来どおり使えます。

| 機能 | アプリでの扱い | 除外の方法（ファイル） |
| --- | --- | --- |
| 管理者機能（ユーザー管理、監査、管理者向け通知設定） | メニューに「管理者設定」を出さない。`/admin` へは遷移しない | `Page::web_only` に `Admin` を含め、`reachable()` が `set_page`・`restore_page`（戻る／進む）・`initial_page` でダッシュボードへ置き換える（`app/state.rs`）。メニュー項目と `AdminPage` の描画を `!APP` で囲む（`pages/shell.rs`）。管理者データの読み込みも行わない（`app/data.rs`） |
| APIドキュメント（`/docs`） | メニューに出さない。遷移しない | `Page::web_only` に `Docs` を含める（`app/state.rs`）。メニュー項目（`pages/shell.rs`）と `DocsPage`（`app.rs`）を `!APP` で囲む |
| ユーザー削除 | 設定タブに「削除」を出さない。`/settings/delete` へは遷移しない | `Page::web_only` に `Delete` を含める（`app/state.rs`）。`settings_tabs(app)` が削除タブを除き、矢印キー／Home／Endでのタブ移動も残りのタブの中で循環する。`DeletePage` は `!APP` のときだけ描画する（`pages/shell.rs`） |
| セキュリティのAPIトークン | カードを出さず、一覧も取得しない | `ApiTokenCard` を `!APP` で囲む（`pages/security.rs`）。`load_api_tokens` を呼ばない（`app/data.rs`） |
| パスワード変更 | 「認証・セキュリティ」では、パスワード変更カードの代わりに案内文と「ブラウザで開く」を出す。初回設定や復旧後にパスワード変更が必須のときも、案内文と「ブラウザで開く」を出す。変更後に「設定状況を更新」を押すと次へ進む | `PasswordCard` を `!APP` で囲む（`pages/security.rs`、`pages/auth.rs` の `OnboardingPage`）。文言 `security.manage_in_browser`、`auth.change_password_in_browser` |

ブラウザの機能が必要なため、依頼された5機能に加えて次もアプリから外しています。

| 機能 | アプリでの扱い | 方法 |
| --- | --- | --- |
| パスキーでのログイン | ログイン画面にログイン方式の選択（パスワード／パスキー）を出さず、パスワードだけにする | `pages/auth.rs` の `auth-method` fieldsetを `!APP` で囲む |
| パスキーの登録 | 登録フォームを出さない。登録済みパスキーの一覧・名前変更・削除はできる | `pages/security.rs` の `PasskeyCard` 内の登録フォームを `!APP` で囲む |
| ブラウザのプッシュ通知（Web Push） | カードを出さない。代わりに端末通知（5.2節）を使う | `pages/notifications.rs` の `PushCard` を `!APP` で囲み、説明文を `notifications.personal_hint_app` に切り替える |

`Page::web_only` と `reachable` は純関数として単体テストしています（`the_app_never_routes_to_web_only_pages`、`app_settings_tabs_leave_out_account_deletion`）。テスト中は `APP` が `false` になるため、`app` を引数で渡して両方の場合を確認します。

## 4. レイアウトの変更点（画面ごと）

アプリ専用の見た目は `dashboard-ui/assets/mobile.css` にまとめました。このファイルは `AppHead` が `cfg(feature = "mobile")` のときだけ `dashboard.css` の後に読み込みます。web版のdistには含まれず、`dashboard.css` も変更していません。ヘッダー、下部ナビゲーション、ボトムシート、設定タブ、本文の余白の変更は、幅600px以下の場合だけです。600pxを超える場合（横向きやタブレット）は、web版と同じヘッダーのメニューボタンと右側のドロワーを使います。「幅によらず」と書いた項目は、すべての幅で適用します。

### 全画面共通

- 幅によらず：`viewport-fit=cover` を指定し、`env(safe-area-inset-*)` の分だけページの上と左右に余白を足して、システムバーやノッチと重ならないようにした。下側の余白は、600px以下では下部バーとボトムシートに足す。拡大縮小は無効（`user-scalable=no`）にした。
- 幅によらず：タップ時のハイライトを消し、文字サイズの自動調整（`text-size-adjust`）を100%に固定した。
- 幅によらず：dxのボタンは縮まない設定のため、行幅を上限にして、文字が大きいときは長いラベルを折り返すようにした。
- 幅によらず：`type=url/number/email/search/tel` の入力欄は共有CSSでスタイルされていなかった（高さ31px）。アプリでは他の入力欄と同じ高さ48pxにした。

### ヘッダーと下部ナビゲーション（新規）

- ヘッダーの高さを56pxにし、ロゴを少し小さくした（web版の600px以下では72px）。ヘッダーのメニューボタンは下部バーへ移したので隠す。
- 画面下に「ダッシュボード／設定／メニュー」の3項目を並べた下部ナビゲーション（`AppNav`、高さ64px）を追加した。現在の画面は `aria-current="page"` とシアンのインジケータで示し、設定の各タブはすべて「設定」に該当する。「設定」を押すとUIDタブを開く。
- テキストを入力している間は、キーボードと重ならないよう下部バーを隠す。
- 文字が極端に大きい場合、下部バーのラベルは途中で改行せず「…」で省略する（支援技術には全文が伝わる）。

### メニュー

- 右からのドロワーを、画面下から出るボトムシートに変えた。上側だけ角丸にし、つまみを付けた。表示時は下からスライドする。
- 中身はweb版と同じ（HoYoLAB、設定、ログアウト、テーマ、言語）。ただし「管理者設定」と「APIドキュメント」はなく、「ブラウザで開く」がある（3節・5.1節）。

### ログイン・初回設定

- ログイン方式の選択がなく、ユーザー名とパスワードだけを入力する（3節）。
- パスワード変更が必須の場合は、変更フォームの代わりに案内文と「ブラウザで開く」を表示する。
- レイアウトはweb版の600px以下と同じ。

### ダッシュボード

- 900px以下で1ゲームずつタブで切り替える表示は、web版と同じ基準（`(max-width:900px)`）のまま。アプリでは `mobile/dom.rs` の `use_media_query` で判定する。
- 下部バーの分（64px + セーフエリア + 32px）だけ、本文の下の余白を広げた。
- アプリをバックグラウンドから戻したとき、前回の取得から60秒以上たっていれば状態を再取得する（web版の60秒ごとの更新に加えた動作）。

### 設定（共通）

- 見出し横の「ダッシュボードへ」ボタンは、下部バーと重複するので隠す。
- タブから「削除」がなくなる（3節）。文字が大きいときは、ページを横に広げずにタブ列の中だけでスクロールする（スクロールバーは非表示）。

### 設定：報酬

- ボーナスの実行履歴のゲーム選択と更新ボタンは、幅が足りなければ折り返す。

### 設定：連携（HoYoLAB）

- 幅によらず：手順中のHoYoLABリンクに縦12pxの余白を加え、タップ領域を44px以上にした。

### 設定：認証・セキュリティ

- パスワード変更カードの代わりに、案内文と「ブラウザで開く」を表示する。パスキーは登録フォームがなく、一覧・名前変更・削除だけになる。APIトークンのカードはない（3節）。セッション一覧はweb版と同じ。

### 設定：通知

- 説明文をアプリ用に変え、「この端末への通知」カード（5.2節）を先頭に置く。Webhookの設定はweb版と同じで、ブラウザのプッシュ通知のカードはない。
- Webhook URLと樹脂の警告値の入力欄は、上記のとおり48pxにした（幅によらず）。

確認した画面サイズ：エミュレータ（API 34）で、360/412/430dp（日本語・ライト）、360dpの文字130%（英語・ダーク）、360dpの文字200%。いずれも横方向のはみ出しはなく、44px未満のタップ領域もなかった。

## 5. 追加機能の仕様

### 5.1 ブラウザで開く

| 場所 | 開くページ |
| --- | --- |
| メニューの「ブラウザで開く」（「設定」の下、外部リンクのアイコン） | いま表示している画面に対応するweb版のページ（例：ダッシュボード → `/dashboard`、セキュリティ → `/settings/security`） |
| 設定 › 認証・セキュリティ | `/settings/security`（パスワード変更とAPIトークン） |
| 初回設定でパスワード変更が必須の画面 | `/dashboard`（web版がログイン後に初回設定へ転送する） |

- URLは `AHD_SERVER_URL` のOrigin（`mobile::http::server_origin()`）と、既存の `navigation::page_path(page)` をつないだもの（`navigation::web_page_url`。単体テストあり）。
- `crate::platform::open_external` が `webbrowser` クレート（1.2、`hardened`）で開きます。Androidでは `Intent.ACTION_VIEW` で既定のブラウザを起動します。
- ブラウザが見つからない場合や、サーバーURLが使えない場合は、エラー行「ブラウザを開けませんでした。」（`messages.could_not_open_the_browser`）を表示します。
- ブラウザ側にはアプリのセッションがないので、web版のログイン画面から入ります。アプリとブラウザのセッションは別物です。
- web版にはこのボタンはありません（`APP` で分岐。web版では `open_external` は常に失敗を返します）。

### 5.2 通知（端末へのローカル通知）

サーバーから送るのではなく、アプリが読み込んだゲームの状態から通知時刻を予測し、Androidのアラームで端末内に通知を出します。サーバーは変更していません。

**通知の種類**（`mobile/reminders.rs`。原神とスターレイルの表示中アカウントが対象）

| 種類 | 時刻の出し方 | 条件 |
| --- | --- | --- |
| 樹脂（原神）・開拓力（スターレイル）の上限到達 | `resin_full_at` / `stamina_full_at` | 現在値が上限未満 |
| 洞天宝銭の上限到達（原神） | `home_coin_full_at` | 現在値が上限未満 |
| 派遣の完了（すべて完了した時点） | `fetched_at` + 未完了の派遣の残り秒数の最大値 | 未完了の派遣がある |
| 参量物質変化器の使用可能（原神） | `transformer_ready_at` | 入手済みで、まだ使用可能でない |

通知のタイトルは「◯◯が上限に達しました」「派遣がすべて完了しました」「参量物質変化器が使用可能です」です。本文は「ゲーム · UID」で、アプリの表示言語（日本語／英語）で作ります。

**設定画面**（設定 › 通知の「この端末への通知」カード。`pages/device_notifications.rs`）

- 全体のON/OFF（既定はOFF）と、種類ごとのON/OFF（既定はすべてON。全体がOFFの間は操作できない）。
- ONにしたとき、Android 13以降で通知が許可されていなければ `POST_NOTIFICATIONS` の許可を求める。拒否された場合は警告と、「通知を許可する」「端末の通知設定を開く」のボタンを表示する。
- 予定している通知の件数を表示する。
- 「テスト通知を表示」で、その場で1件通知する。

**動作**（`app/device_notifications.rs`、`mobile/notify.rs`、`android/MainActivity.kt`）

- 状態、設定、言語のいずれかが変わるたびに予定を計算し直します。前回送った内容と違うときだけ、JNI経由でKotlinへ渡します。
- Kotlinは `AlarmManager.setAndAllowWhileIdle`（不正確なアラーム）で予約します。正確なアラームの権限は不要です。発火すると `AhdAlarmReceiver` が通知を出すので、アプリのプロセスが終了していても届きます。
- 通知チャンネルは `game_status`（名前「ゲームの状態」、重要度は既定）。通知アイコンは `res/drawable/ic_stat_ahd.xml` です。
- 通知をタップすると、既存のタスクを作り直さずにアプリを前面に出し、ダッシュボードを表示します（`CLEAR_TOP|SINGLE_TOP`）。
- 端末の再起動後とアプリの更新後は、`AhdBootReceiver`（`BOOT_COMPLETED`、`MY_PACKAGE_REPLACED`）が予約を戻します。
- 予約は配信済みのものを再び出さないようにしています。時刻を過ぎた予定も1時間は保持し、遅れて届くアラームを取り消さないようにしています。
- 予定を消すのは、ログアウト、セッションの期限切れ・失効（401）、ユーザー削除のときだけです。オフラインで起動した場合など、それ以外の理由でサインアウト状態になったときは予定を残します。別のユーザーでログインすると、前のユーザーの予定は消えます。

**保存先**（いずれもアプリ専用領域）

| ファイル | 内容 |
| --- | --- |
| `files/device-notifications.json` | 全体と種類ごとのON/OFF |
| `files/notification-schedule.json` | 予測した予定（所有ユーザー、アカウント、種類、時刻） |
| SharedPreferences `ahd_notifications` | Kotlin側の予約済み一覧とチャンネル名（再起動後の復元用） |

対象外：デイリー／週間任務の未完了、ログインボーナスの結果、Cookieの失効。これらは時刻を予測できず、通知の時点で最新の状態を取得し直さないと誤った通知になるため、今回は扱いません。サーバーのWebhook通知は従来どおり使えます。

## 6. web版との共通化と分岐

### 共通のまま使うもの

- 画面のコンポーネント（`pages/`）、状態管理（`app/state.rs`、`app/session.rs`、`app/actions.rs`、`app/data.rs`）、文言カタログ（`locales/*.json`）、`dashboard.css`、同梱フォント。
- `api.rs` のパス検査、応答サイズの上限、エラー処理（送信部分の `fetch` だけを分岐）。
- 画面とURLの対応（`navigation::page_path`／`page_from_path`）。アプリもWebView内のHistory APIを使うので、Androidの戻るボタンで前の画面へ戻れる。
- 言語とテーマの判定（`Preferences::from_saved`）。保存先だけが違い、web版は `localStorage`、アプリは `files/preferences.json` に保存する。保存がない場合、アプリは日本語・端末のテーマで始まる。

### 分岐のしかた

`cfg` は3通りに分けています。

| 条件 | 対象 |
| --- | --- |
| `cfg(target_arch = "wasm32")` | web版（web-sys、gloo、js-sys） |
| `cfg(feature = "mobile")` | Androidアプリ（`mobile::*`、reqwest、JNI） |
| `cfg(not(any(target_arch = "wasm32", feature = "mobile")))` | ネイティブのテスト用の代替（何もしない） |

アプリかどうかで表示を変える部分は、`cfg` ではなく `if crate::platform::APP` / `if !crate::platform::APP` で書いています。両方のビルドでコンパイルされ、型検査も両方で行われます。

web版と同じ処理をアプリ用に置き換えた箇所：

| 処理 | web版 | アプリ | 共通の入口 |
| --- | --- | --- | --- |
| HTTP | gloo-net（fetch、`credentials: include`） | `mobile::http::send`（reqwest、Cookie jar） | `api::fetch` |
| タイマー | `gloo_timers` | `tokio::time::sleep` | `platform::sleep_ms` |
| 現在時刻 | `js_sys::Date::now` | `SystemTime` | `platform::now_ms` |
| 時刻の表示と経過時間 | `js_sys::Date` | chrono（`mobile/time.rs`） | `app/display.rs` の各関数 |
| DOM（フォーカス、checked、`<dialog>`、`matchMedia`、History、`hidden`） | web-sys | `document::eval`（`mobile/dom.rs`） | `pages/common.rs`、`app/navigation.rs`、`pages/dashboard.rs`、`app.rs` |
| 言語とテーマの保存 | `localStorage` | `files/preferences.json` | `app/preferences.rs` |
| 外部URL | （なし） | `webbrowser` | `platform::open_external` |

`mobile/session.rs`・`mobile/time.rs`・`mobile/reminders.rs` は端末やI/Oに依存しないため、通常の `cargo test` でもコンパイルしてテストします（`main.rs` の `cfg(any(feature = "mobile", test))`）。

### 変更したファイル

新規（アプリ専用）：

| ファイル | 内容 |
| --- | --- |
| `dashboard-ui/src/platform.rs` | `APP`、`sleep_ms`、`now_ms`、`open_external` |
| `dashboard-ui/src/mobile/mod.rs` | アプリ用モジュールの宣言 |
| `dashboard-ui/src/mobile/http.rs` | サーバーへの直接接続、Cookieの保存、`server_origin` |
| `dashboard-ui/src/mobile/session.rs` | Cookie jarとサーバーURLの規則（I/Oなし、テストあり） |
| `dashboard-ui/src/mobile/storage.rs` | アプリ専用領域のファイル（JNIで `getFilesDir()` を取得） |
| `dashboard-ui/src/mobile/time.rs` | 端末時刻での表示 |
| `dashboard-ui/src/mobile/dom.rs` | `document::eval` によるDOM操作 |
| `dashboard-ui/src/mobile/reminders.rs` | 通知時刻の予測、設定と予定の保存形式、通知文（テストあり） |
| `dashboard-ui/src/mobile/notify.rs` | Kotlinの通知処理を呼ぶJNIブリッジ |
| `dashboard-ui/src/app/device_notifications.rs` | 端末通知の予定の更新、通知タップ時の画面遷移 |
| `dashboard-ui/src/pages/device_notifications.rs` | 「この端末への通知」カード |
| `dashboard-ui/src/pages/app_nav.rs` | 下部ナビゲーション `AppNav` と `AppHead`（モジュールは両ビルドでコンパイルするが、web版では描画しない） |
| `dashboard-ui/assets/mobile.css` | アプリ専用のスタイル |
| `dashboard-ui/android/MainActivity.kt` | `MainActivity`（ルートでの戻る、設定変更のその場反映、破棄時のプロセス終了、通知タップの受け取り）、`AhdNotifications`、`AhdAlarmReceiver`、`AhdBootReceiver` |
| `dashboard-ui/android/AndroidManifest.application.xml` | 生成manifestに挿入するreceiver |
| `dashboard-ui/android/res/**` | ランチャーアイコン、通知アイコン、アプリ名、色、DayNightテーマ（`values/styles.xml`） |
| `dashboard-ui/android/README.md` | リソースの作り方 |
| `scripts/build-android.sh` | APKのビルド（2節）。生成manifestへの `launchMode="singleTask"`・`configChanges` の書き換えとreceiverの挿入も行う |
| `docs/android-changes.md` | 本書 |

変更（web版と共有）：

| ファイル | 変更内容 |
| --- | --- |
| `dashboard-ui/src/main.rs` | `platform`・`mobile` モジュールの追加 |
| `dashboard-ui/src/api.rs` | `fetch` のアプリ分岐、関連する `cfg` の拡張 |
| `dashboard-ui/src/app.rs` | `AppHead`、端末通知のフック、復帰時の再取得、`DocsPage` の除外、`platform` のタイマー・時刻の使用 |
| `dashboard-ui/src/app/state.rs` | `Page::web_only`、`reachable`、ステータス自動消去のタイマー |
| `dashboard-ui/src/app/navigation.rs` | アプリ内History、`open_web_version`、`web_page_url` |
| `dashboard-ui/src/app/data.rs` | APIトークン・管理者データを読み込まない分岐、`platform::sleep_ms` |
| `dashboard-ui/src/app/display.rs` | 時刻表示のアプリ分岐 |
| `dashboard-ui/src/app/preferences.rs` | 言語とテーマの保存のアプリ分岐 |
| `dashboard-ui/src/pages/shell.rs` | `settings_tabs`、メニュー項目の分岐、`AppNav` |
| `dashboard-ui/src/pages/auth.rs` | パスキーのログイン方式の除外、パスワード変更の案内 |
| `dashboard-ui/src/pages/security.rs` | パスワード変更・APIトークン・パスキー登録の除外、「ブラウザで開く」 |
| `dashboard-ui/src/pages/notifications.rs` | 端末通知カード、Web Pushの除外、説明文の切り替え |
| `dashboard-ui/src/pages/common.rs` | `external` アイコン、DOM操作・モーダルのアプリ分岐 |
| `dashboard-ui/src/pages/dashboard.rs` | `matchMedia` と1分ごとの再描画のアプリ分岐 |
| `dashboard-ui/src/pages/mod.rs` | 新しいモジュールの宣言 |
| `dashboard-ui/locales/ja.json`, `en.json` | 24キーを追加（`shell.open_in_browser`、`shell.app_navigation`、`messages.could_not_open_the_browser`、`auth.change_password_in_browser`、`security.manage_in_browser`、`notifications.personal_hint_app`、`notifications.device_*` 18キー）。既存のキーは変えていない |
| `dashboard-ui/Cargo.toml` | `mobile` feature とoptional依存、Android向けの `jni`・`ndk-context`、dev-dependenciesの `chrono`・`url` |
| `dashboard-ui/Dioxus.toml` | `android_main_activity`、`[bundle]`、`[android]`、`[android.permissions]` |
| `Cargo.lock` | 上記の依存の追加 |
| `devenv.nix` | `android`・`android-emulator` プロファイル、`build-android`・`check-android` |
| `devenv.yaml` | `allowUnfree: true` |
| `rust-toolchain.toml` | Android向けtargetの追加 |
| `.gitignore` | `android` プロファイルが書く `/local.properties`、`/.android/` |
| `README.md`, `docs/README.md` | Android節、UIモジュール表、本書へのリンク |

### web版への影響

画面の表示、挙動、API、サーバーには変更がありません。変更後もweb版の `check-rust`（fmt、clippy native/wasm）、UI単体テスト、`test-ui`（Playwright 2回）が通ることを確認しています。ただし、次の点はweb版の開発環境やビルドにも関係します。

- **`dashboard-ui/Cargo.toml`**：`dioxus` 依存の基本featureから `web` を外し、`web` feature（既定）へ移しました。既定のfeatureでのビルドは従来と同じです。`--no-default-features` でビルドする場合は、web用のDioxusが入らなくなります。
- **`rust-toolchain.toml`**：targetsに `aarch64-linux-android` と `x86_64-linux-android` を追加しました。既定のdevenvシェルでも、初回にこの2つの標準ライブラリを取得します。
- **`devenv.yaml`**：`allowUnfree: true` にしました。unfreeのパッケージ（Android SDK/NDK）を使うのは `android` プロファイルだけで、既定のシェルの中身は変わりません。
- **共有コードの置き換え**：`gloo_timers::future::TimeoutFuture` を `platform::sleep_ms` に、`js_sys::Date::now` を `platform::now_ms` に置き換えました。wasmで実行される処理は同じです。`next_settings` は引数 `app` を受け取るようになりました（web版は `false`）。
- **文言カタログ**：カタログはwasmに埋め込まれるため、アプリ用の24キーもweb版のバンドルに入ります（数KB）。追加した文言の文字はすべて同梱フォントのsubsetに含まれており、フォントは再生成していません。
- **`Cargo.lock`**：mobile用の依存（reqwest、tokio、webbrowser、dioxus-mobile一式など）が増えました。web版のwasmビルドでは、これらはコンパイルされません。ネイティブの `cargo test` では、dev-dependenciesの `chrono` と `url` がコンパイルされます。
- **`mobile.css` と `AppNav`**：web版のdistには `mobile.css` が含まれず、`AppNav` も描画されません（`app-nav` はweb版のwasmに含まれないことを確認済み）。

## 7. 既知の制約と今後の課題

### 動作の制約

- **Activityは1つだけ**：tao/wryはActivityの破棄・再生成と2つ目のインスタンスに対応しておらず、そのままだとプロセスが `FORTIFY: pthread_mutex_lock called on a destroyed mutex` で異常終了します。そのため次のようにしています（`android/MainActivity.kt`、`scripts/build-android.sh`）。
  - `launchMode="singleTask"` で、ランチャー・通知・`am start` のどこから開いても同じインスタンスを使います。通知のタップは `onNewIntent` で受け取ります。
  - 最初の画面で戻るボタンを押すと、アプリを閉じずにバックグラウンドへ移します（ホームボタンと同じ）。それ以外の画面では、従来どおり前の画面へ戻ります。
  - ダークモード、文字サイズ、表示サイズ、太字、言語、キーボードなどの変更では、Activityを作り直さずにその場で反映します（`configChanges`）。WebViewは文字サイズを作成時にしか読まないため、変更時に `textZoom` を設定し直します。
  - 最近使ったアプリから消すなどでActivityが破棄されたときは、ネイティブの後始末を行わずにプロセスを終了します。次に開くと新しく起動します。
  - テーマはDayNightなので、アプリのテーマ「システム」は端末のダークモードに追従します（以前は常にライトでした）。
- **ダイアログと戻るボタン**：確認ダイアログを開いたまま戻るボタンを押すと、ダイアログが閉じるのではなく前の画面に戻ります（ダイアログは片付きます）。
- **サーバーはビルド時に固定**：接続先は `AHD_SERVER_URL` の1つだけです。変えるにはビルドし直します。
- **セッションCookieの保存**：アプリ専用領域に平文で保存します（Android Keystoreは使っていません）。他のアプリからは読めませんが、root化した端末やバックアップでは読める可能性があります。
- **パスワード表示の自動マスク**：web版は、表示中のパスワードを、ウィンドウや入力欄からフォーカスが外れたときとページが非表示になったときに伏せます。アプリではこの処理を実装していません（表示ボタンでの切り替えはできます）。
- **言語の初期値**：保存がない場合は日本語で始まり、端末のロケールは見ません。
- **通知**
  - アラームは不正確で、Dozeなどにより数分遅れることがあります。
  - 予測はアプリが状態を読み込んだ時点の値に基づきます。ゲーム内で樹脂を使った場合などは、次にアプリを開くまで早すぎる通知が出ることがあります。バックグラウンドでの再取得は行いません。
  - 対象は、ダッシュボードで読み込んだアカウント（各ゲームで選択中のもの）です。
  - 強制停止やアプリの更新でOSがアラームを消します。次にアプリを起動したとき、またはパッケージ更新時のreceiverで戻ります。
  - 通知をタップしてダッシュボードへ移っても、スクロール位置はそのままです。
  - `dx serve` のAPKにはreceiverが入りません。
- **ビルド**：release署名を設定していません（デバッグ署名だけ）。`dx serve` ではDioxusの既定のアイコンと名前になります。

### 未確認の項目

- 実機では確認していません（API 34のx86_64エミュレータだけで確認）。
- アプリのプロセスが生きたままシステムがActivityだけを破棄する場合は、エミュレータの「アクティビティを保持しない」設定が効かなかったため、最近使ったアプリから消す操作でしか確認していません。
- ステータスバーは、ライト・ダークどちらのテーマでもAppCompatの既定の灰色のままです。
- 実機のIMEでの入力は確認していません。adbで高速に入力すると文字が落ちることがあります。
- 初回設定の画面は、レイアウトの検査で確認していません（合成データに該当するユーザーがいないため）。
- API 34イメージのWebView 113では、ボトムシートの表示アニメーション（`@starting-style`）が動きません。新しいWebViewでの見え方は確認していません。

### 今後の課題

- ダイアログが開いているときは、戻るボタンでダイアログを閉じる。
- release署名と配布方法（ストア、または直接配布）を決める。
- セッションCookieをAndroid Keystoreで暗号化して保存する。
- 予測できない通知（デイリー／週間の未完了、ログインボーナスの結果、Cookieの失効）が必要なら、FCMやUnifiedPushによるサーバーからのプッシュを検討する。サーバー側に送信の仕組みが必要になる。
- 接続先のサーバーをアプリ内で設定できるようにするかを検討する。
- 端末のロケールから言語の初期値を決める。
- 実機で確認する。
