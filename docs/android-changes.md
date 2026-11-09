# Androidアプリ：web版からの変更点

`dashboard-ui` をAndroidアプリとしてビルドできるようにした際に、web版から変えた点をまとめます。利用者向けの違いは1〜5節と7節、開発者向けの詳細は6節にあります。ビルド手順の正本は [README](../README.md) の「Androidアプリ」節です。本書はその要約と、変更の理由・範囲を説明します。

## 1. 概要

### 方針

- 新しいフロントエンドは作りません。web版と同じDioxus（RSX）コード、同じ画面構成、同じ文言カタログ、同じ `dashboard.css` をそのまま使います。
- アプリ固有の処理はCargo feature `mobile` の中に閉じ込めます。web版は従来どおり既定の `web` feature でビルドし、アプリのコードはwasmに入りません。
- アプリは既存の `/api/v1` を、ブラウザと同じセッションCookie認証で呼びます。サーバーへの追加は、web版の通知をアプリへ届けるための端末登録と受信箱（5.3節）と、その受信箱をUnifiedPushで知らせる購読（5.9節）だけです。
- ブラウザでしか動かない機能（WebAuthn）と、アプリに不要な機能（管理者機能など。3節）は、アプリで表示も遷移もしません。必要なときは「ブラウザで開く」（5.1節）でweb版に移ります。

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

- 接続先の既定はビルド時の `AHD_SERVER_URL` です。利用者はログイン画面の「接続先サーバー」で別のサーバーに変えられます（5.11節）。どちらもHTTPS、またはlocalhostのHTTPだけを受け付け、保存するのはOriginだけです（パスやクエリ、認証情報は持ちません。`mobile/session.rs::parse_origin`、`mobile/server.rs`）。
- ログイン応答の `Set-Cookie` をCookie jarに入れ、Android Keystoreの鍵で暗号化してアプリ専用領域の `session-cookies.enc`（`getFilesDir()`）へ保存し、以後の要求に `Cookie` を付けます。ログアウトや期限切れで消えます。暗号化と移行の詳細は5.7節にあります。
- サーバーがCSRF対策で検査する `Origin` には、選択中のサーバー自身のOriginを送ります。リダイレクトは追いません（Cookieと `Origin` を別のホストへ送らないため）。
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
- アプリのタグ（`vX.Y.Z`。版の正本は `dashboard-ui/Cargo.toml`、webの版・タグとは独立。[versioning.md](versioning.md)）のCI（`.forgejo/workflows/android.yml`）はarm64のrelease APKと `SHA256SUMS` をForgejoリリースへ添付し、リリース鍵で署名し直します（証明書は `android/release-signing-cert.sha256` で固定。命名規則と手順は [releases.md](releases.md)）。`build-android` はバージョン `X.Y.Z` から `versionCode = X*10000+Y*100+Z`（Y・Zは100未満）を設定します。
- パッケージ名は `cc.attodao.ahd`、minSdk 24、targetSdk 34です。権限は、dxが既定で付けるINTERNETに加えて、`POST_NOTIFICATIONS`、`RECEIVE_BOOT_COMPLETED` と更新用の `REQUEST_INSTALL_PACKAGES` を使います（`Dioxus.toml`）。
- 開発用サーバーにつなぐ場合：`AHD_SERVER_URL` を省略すると `http://localhost:8080` になります。`adb reverse tcp:8080 tcp:8080` でホストの開発サーバー（`PUBLIC_ORIGIN=http://localhost:8080`）へつなぎます。ただし `build-android` では `AHD_SERVER_URL` が必須なので、`AHD_SERVER_URL=http://localhost:8080` と明示します。
- エミュレータは `devenv --profile android --profile android-emulator shell` で入ります（x86_64イメージ、KVMが必要）。
- `dx serve --platform android` でも起動できますが、アイコンと名前がDioxusの既定のままで、通知用のreceiverも入りません。通知を確認するときは `build-android` のAPKを使います。

## 3. Android版で除外した機能と除外方法

除外は「アプリでは画面へ遷移しない」と「操作部品を描画しない」の2段で行います。どちらも `crate::platform::APP` で分岐し、web版の挙動は変えていません。サーバー側のAPIはそのまま残っているため、ブラウザでは従来どおり使えます。

| 機能 | アプリでの扱い | 除外の方法（ファイル） |
| --- | --- | --- |
| 管理者機能（ユーザー管理、監査、管理者向け通知設定） | メニューに「管理者設定」を出さない。`/admin` へは遷移しない | `Page::web_only` に `Admin` を含め、`reachable()` が `set_page`・`restore_page`（戻る／進む）・`initial_page` でダッシュボードへ置き換える（`app/state.rs`）。メニュー項目と `AdminPage` の描画を `!APP` で囲む（`pages/shell.rs`）。管理者データの読み込みも行わない（`app/data.rs`） |
| APIドキュメント（`/docs`） | メニューに出さない。遷移しない | `Page::web_only` に `Docs` を含める（`app/state.rs`）。メニュー項目（`pages/shell.rs`）と `DocsPage`（`app.rs`）を `!APP` で囲む |
| 設定の移行（書き出し・読み込み） | 設定タブに「移行」を出さない。`/settings/transfer` へは遷移しない。ブラウザで開けば使える | `Page::web_only` に `Transfer` を含め、`settings_tabs(app)` が移行タブを除く。`TransferPage` は `!APP` のときだけ描画する（`pages/shell.rs`）。WebViewにはファイル選択（`onShowFileChooser`）とダウンロードの処理がなく、ネイティブの追加実装が要るため、web版だけにした |
| ユーザー削除 | 設定タブに「削除」を出さない。`/settings/delete` へは遷移しない | `Page::web_only` に `Delete` を含める（`app/state.rs`）。`settings_tabs(app)` が削除タブを除き、矢印キー／Home／Endでのタブ移動も残りのタブの中で循環する。`DeletePage` は `!APP` のときだけ描画する（`pages/shell.rs`） |
| セキュリティのAPIトークン | カードを出さず、一覧も取得しない | `ApiTokenCard` を `!APP` で囲む（`pages/security.rs`）。`load_api_tokens` を呼ばない（`app/data.rs`） |
| パスワード変更 | 「認証・セキュリティ」では、パスワード変更カードの代わりに案内文と「ブラウザで開く」を出す。初回設定や復旧後にパスワード変更が必須のときも、案内文と「ブラウザで開く」を出す。変更後に「設定状況を更新」を押すと次へ進む | `PasswordCard` を `!APP` で囲む（`pages/security.rs`、`pages/auth.rs` の `OnboardingPage`）。文言 `security.manage_in_browser`、`auth.change_password_in_browser` |

ブラウザの機能が必要なため、依頼された5機能に加えて次もアプリから外しています。

| 機能 | アプリでの扱い | 方法 |
| --- | --- | --- |
| パスキーでのログイン | パスワードフォームの下の「パスキーでログイン」「ブラウザでログイン」からブラウザで認証し、許可後にアプリへ戻る（5.3節） | `pages/auth.rs` の代替ボタンから `app/browser_login.rs` でハンドオフする |
| パスキーの登録 | 登録フォームを出さない。登録済みパスキーの一覧・名前変更・削除はできる | `pages/security.rs` の `PasskeyCard` 内の登録フォームを `!APP` で囲む |

`Page::web_only` と `reachable` は純関数として単体テストしています（`the_app_never_routes_to_web_only_pages`、`app_settings_tabs_leave_out_the_settings_file_and_account_deletion`）。テスト中は `APP` が `false` になるため、`app` を引数で渡して両方の場合を確認します。

## 4. レイアウトの変更点（画面ごと）

アプリ専用の見た目は `dashboard-ui/assets/mobile.css` にまとめました。このファイルは `AppHead` が `cfg(feature = "mobile")` のときだけ `dashboard.css` の後に読み込みます。web版のdistには含まれず、`dashboard.css` も変更していません。ヘッダー、下部ナビゲーション、ボトムシート、設定タブ、本文の余白の変更は、幅600px以下の場合だけです。600pxを超える場合（横向きやタブレット）は、web版と同じヘッダーのメニューボタンと右側のドロワーを使い、アプリではヘッダーのメニューボタンの左に設定アイコンボタンも表示します。「幅によらず」と書いた項目は、すべての幅で適用します。

### 全画面共通

- 幅によらず：`viewport-fit=cover` を指定し、`env(safe-area-inset-*)` の分だけページの上と左右に余白を足して、システムバーやノッチと重ならないようにした。下側の余白は、600px以下では下部バーとボトムシートに足す。拡大縮小は無効（`user-scalable=no`）にした。
- 幅によらず：タップ時のハイライトを消し、文字サイズの自動調整（`text-size-adjust`）を100%に固定した。
- 幅によらず：dxのボタンは縮まない設定のため、行幅を上限にして、文字が大きいときは長いラベルを折り返すようにした。
- 幅によらず：`type=url/number/email/search/tel` の入力欄は共有CSSでスタイルされていなかった（高さ31px）。アプリでは他の入力欄と同じ高さ48pxにした。その後、web版でも同じ問題があったため、この指定は共有の `dashboard.css` に移した（`mobile.css` からは削除。アプリの見た目は変わらない）。

### ヘッダーと下部ナビゲーション（新規）

- ヘッダーの高さを56pxにし、ロゴを少し小さくした（web版の600px以下では72px）。ヘッダーのメニューボタンは下部バーへ移したので隠す。下部バーに「設定」があるため、ヘッダーの設定ボタンも隠す。
- 画面下に「ダッシュボード／イベント／設定／メニュー」の4項目を並べた下部ナビゲーション（`AppNav`、高さ64px、項目の順は `pages/app_nav.rs` の `NAV`）を追加した。現在の画面は `aria-current="page"` とシアンのインジケータで示し、設定の各タブはすべて「設定」に該当する。「設定」を押すとUIDタブを開く。「イベント」（カレンダーのアイコン）は「イベント」画面（`/events`）を開く。360dpでは1項目90dp（左右の余白2dp）で、標準の文字サイズなら「ダッシュボード」まで省略せずに収まり、それより大きい文字ではラベルを「…」で省略する。
- テキストを入力している間は、キーボードと重ならないよう下部バーを隠す。
- 文字が極端に大きい場合、下部バーのラベルは途中で改行せず「…」で省略する（支援技術には全文が伝わる）。

### メニュー

- 右からのドロワーを、画面下から出るボトムシートに変えた。上側だけ角丸にし、つまみを付けた。表示時は下からスライドする。
- 中身はweb版と異なり、アプリのメニューは上から「イベント」、「ブラウザで開く」、HoYoLAB、「このアプリについて」、ログアウト、テーマ、言語。「イベント」は下部バーのない600pxを超える幅（タブレット）のためのもので、600px以下では隠す（`mobile.css` の `.menu-events`）。「設定」「管理者設定」「APIドキュメント」はない。設定へは下部ナビゲーション（幅600px以下）か、ヘッダーの設定ボタン（600pxを超える場合）から移動する。web版のメニューも「イベント」から始まる。

### ログイン・初回設定

- アプリ内にはユーザー名とパスワードのフォームを置き、その下に「パスキーでログイン」「ブラウザでログイン」を表示する（3節・5.3節）。いちばん下に「接続先サーバー」の欄を置く（5.11節）。
- パスワード変更が必須の場合は、変更フォームの代わりに案内文と「ブラウザで開く」を表示する。
- レイアウトはweb版の600px以下と同じ。

### ダッシュボード

- 900px以下で1ゲームずつタブで切り替える表示は、web版と同じ基準（`(max-width:900px)`）のまま。アプリでは `mobile/dom.rs` の `use_media_query` で判定する。
- 下部バーの分（64px + セーフエリア + 32px）だけ、本文の下の余白を広げた。
- アプリをバックグラウンドから戻したとき、前回の取得から60秒以上たっていれば状態を再取得する（web版の60秒ごとの更新に加えた動作）。
- web版のカード右上にある更新ボタン（丸矢印）は出さない。代わりに、ページが最上部までスクロールされた状態で下に引っ張って離すと（減衰後72px以上）、UID一覧・両ゲームの状態・ボーナス設定を読み直す（`load_dashboard_data`、pull-to-refresh）。引っ張っている間は画面上部に丸矢印のインジケータが指に付いて回り、しきい値を超えるとシアンになり、読み込み中は回転し続ける。スクリーンリーダーには「更新中…」を `role="status"` で伝える。開いているダイアログ、入力欄・選択欄から始めたタッチ、横方向が主のスワイプでは反応しない。ダッシュボードではWebViewのオーバースクロール効果を止める（`overscroll-behavior-y:none`）。リスナーは `mobile/dom.rs` の `PullToRefresh` が持ち、ダッシュボードを離れると外す。
- ゲームタブ（900px以下）はパネルの上で左右にスワイプしても切り替わる（左で次、右で前、端では止まる）。web版と同じ `SwipeableTabs` を使い、アプリではタッチを `mobile/swipe.rs` が集める。横方向が主のスワイプでは上の pull-to-refresh が外れ、縦方向が主の引っ張りではスワイプにならないので、2つは同時に動かない。
- 各ゲームカードの「高難度コンテンツ」欄（`pages/challenges.rs`）はweb版と同じ部品・同じCSSで、アプリ専用のレイアウトはない。pull-to-refreshは両ゲームの状態の後にこの要約も読み直す（サーバーのキャッシュTTLは `CHALLENGE_CACHE_TTL_MINUTES`）。ホーム画面ウィジェットと端末通知には入れていない。エンドポイントのない古いサーバーにつないだ場合（404）は、エラーを出さずに欄ごと表示しない。
- イベントと祈願／跳躍のカレンダーはダッシュボードから「イベント」画面へ移した（下の「イベント」）。ダッシュボードのpull-to-refreshはカレンダーを読まない。
- 全体レビューでweb版と共通に変えた点（アプリ専用のレイアウトはない）：見出しの「ログインボーナスを今すぐ受け取る」の下に、ゲームごとの本日の受け取り状態（「本日分 原神 未受け取り · 崩壊：スターレイル 受け取り済み」）を出す。状態の取得の経過時間はカードの下ではなくゲーム名の下に置く（アプリには更新ボタンがないので、見出しは名前と経過時間だけ）。読み込みは状態→ボーナス設定→7日間の記録→高難度コンテンツの順で、ボタンと受け取り状態がHoYoLABを待つ読み込みの後ろに並ばない。順番待ちの状態・高難度コンテンツは「読み込み中…」と表示する。表示UIDの選択肢は「UID … · 地域」の順。
- 主リソース（樹脂・開拓力）の直下の「上限に達していた時間（直近7日間）」（`pages/cap_history.rs`）もweb版と同じ部品・同じCSSで、アプリ専用のレイアウトはない。合計の時間はラベルの右（360px幅では次の行）に置き、タイムラインはカード幅いっぱいのSVG（inline styleを使わない）、凡例は折り返す。pull-to-refreshは状態とボーナス設定の次に履歴も読み直す。履歴はサーバーに保存した状態の取得結果だけから計算するので、上流への要求は増えない。ホーム画面ウィジェットには出さない。
- 畳んだ「取得の詳細」（UID・地域・最終取得日時、失敗時はエラー・コード・日時）は、web版と同じく各ゲームカードの最後（ログインボーナスの下）に置く（`pages/dashboard.rs` の `StatusDetails`）。状態の取得結果があるときだけ出し、開閉行にはエラーの要約も出す。アプリ専用の分岐やレイアウトはない。

### イベント（新規、web版と共通）

- 「イベント」画面（`pages/events.rs`）は下部バーの「イベント」から開く。アプリでは幅によらず縦向きのタイムライン（`pages/event_list.rs`）だけを描く。web版の601px以上の横向きの表（`pages/event_timeline.rs`）はアプリでは描かない（`platform::APP`）。
- 「開催中」の見出しの下に「現在 11/09 21:30 JST」の印、そこから開催中の祈願／跳躍とイベントを終了の早い順に、続く「開催予定」は「11/12 開始」の日付ごとに、縦の線にぶら下がるカードで並べる。カードは種類・残り時間、名前、ピックアップ（公式画像CDNだけをRefererなしで読み込み、読み込めないときは名前だけを残す）、期間（端末のローカル時刻）、進捗。開始も終了も不明な項目は開催中に数えず、最後の「期間不明」の見出しの下に期間の行（「–」だけの行）なしで並べる。
- ゲームの切り替えはダッシュボードと同じタブとスワイプ（900px以下）で、選択中のゲームも共通。ページを最上部から下に引っ張ると、UID一覧と両ゲームのカレンダーを読み直す（`load_events_data`）。pull-to-refreshのリスナーは画面ごとに別のID（`window.__ahdPulls[id]`）で持つ。ダッシュボードからイベントへ移ると新しい画面のリスナーが先に付き、古い画面の後片付けがその後に走るため、共有の1つのハンドルでは新しい画面のリスナーまで外れてしまう（`mobile/dom.rs` の `PullToRefresh`）。web版の更新ボタンは出さない。
- カレンダーは画面を開いたときに読み、操作のロック（`busy`）は使わない。ダッシュボードの「ダッシュボードへ」に当たるページ見出し横のボタンは、600px以下では下部バーがあるので隠す。
- エンドポイントのない古いサーバーでは（404）「カレンダーを取得できませんでした。」と表示する。ホーム画面ウィジェットと端末通知には入れていない。

### 設定（共通）

- 見出し横の「ダッシュボードへ」ボタンは、下部バーと重複するので隠す。
- タブから「削除」がなくなる（3節）。文字が大きいときは、ページを横に広げずにタブ列の中だけでスクロールする（スクロールバーは非表示）。
- 幅によらず：web版と同じく、本文を左右にスワイプするとタブが切り替わる（`SwipeableTabs`、左で次・右で前、端では止まる）。タブ列が横スクロールしているときは、スワイプで選んだタブが見えるようにタブ列だけを横にスクロールする（`mobile::dom::reveal_horizontally`）。

### 設定：報酬

- ボーナスの実行履歴のゲーム選択と更新ボタンは、幅が足りなければ折り返す。

### 設定：連携（HoYoLAB）

- 幅によらず：手順中のHoYoLABリンクに縦12pxの余白を加え、タップ領域を44px以上にした。

### 設定：認証・セキュリティ

- パスワード変更カードの代わりに、案内文と「ブラウザで開く」を表示する。パスキーは登録フォームがなく、一覧・名前変更・削除だけになる。APIトークンのカードはない（3節）。セッション一覧はweb版と同じ。

### 設定：通知

- ページの説明文は出さず、「この端末への通知」カード（全体のON/OFF・許可・登録状態・「UnifiedPushですぐに受け取る」とその状態・テスト。5.2節・5.3節・5.9節）を先頭に置く。
- その下の1枚のパネルで、通知の種類ごとに「この端末」と「Webhook」のON/OFFを並べた表（`pages/notifications.rs` の `EventMatrix`）、リマインダー、Webhookの設定（URL・形式・言語）をまとめて扱い、「保存」で全部を保存する。項目は名前だけで、機能の説明文は出さない。
- 「この端末」の列はサーバーのアプリ用の選択（`app_events`）を送る。樹脂・開拓力の上限、派遣、参量物質変化器はこの選択が5.2節の予測通知にも適用され（保存の成功時に端末の設定へ反映）、洞天宝銭の上限は予測通知だけなので「Webhook」の列が空欄になる。web版は表を出さず、Webhookの選択だけを送る（`app_events` を送らないので、アプリで選んだ内容は変わらない）。
- Webhook URLと樹脂の警告値の入力欄は、上記のとおり48pxにした（幅によらず）。
- 表の「この端末」「Webhook」の列は4.5rem（見出しは14px）にして、360dpでも通知の名前の列に幅を残す（5.5remでは名前が約80dpに縮み、3〜4行に折り返していた）。

確認した画面サイズ：エミュレータ（API 34）で、360/412/430dp（日本語・ライト）、360dpの文字130%（英語・ダーク）、360dpの文字200%。いずれも横方向のはみ出しはなく、44px未満のタップ領域もなかった。

## 5. 追加機能の仕様

### 5.1 ブラウザで開く

| 場所 | 開くページ |
| --- | --- |
| メニューの「ブラウザで開く」（メニューの先頭、外部リンクのアイコン） | いま表示している画面に対応するweb版のページ（例：ダッシュボード → `/dashboard`、セキュリティ → `/settings/security`） |
| 設定 › 認証・セキュリティ | `/settings/security`（パスワード変更とAPIトークン） |
| 初回設定でパスワード変更が必須の画面 | `/dashboard`（web版がログイン後に初回設定へ転送する） |

- URLは選択中のサーバーのOrigin（`mobile::http::server_origin()`。既定は `AHD_SERVER_URL`、5.11節）と、既存の `navigation::page_path(page)` をつないだもの（`navigation::web_page_url`。単体テストあり）。
- `crate::platform::open_external` が `webbrowser` クレート（1.2、`hardened`）で開きます。Androidでは `Intent.ACTION_VIEW` で既定のブラウザを起動します。
- ブラウザが見つからない場合や、サーバーURLが使えない場合は、エラー行「ブラウザを開けませんでした。」（`messages.could_not_open_the_browser`）を表示します。
- ブラウザ側にはアプリのセッションがないので、web版のログイン画面から入ります。アプリとブラウザのセッションは別物です。
- web版にはこのボタンはありません（`APP` で分岐。web版では `open_external` は常に失敗を返します）。

### 5.2 通知（予測によるローカル通知）

アプリが読み込んだゲームの状態から通知時刻を予測し、Androidのアラームで端末内に通知を出します。web版と同じ通知をサーバーから受け取る仕組みは5.3節にあります。

**通知の種類**（`mobile/reminders.rs`。原神とスターレイルの表示中アカウントが対象）

| 種類 | 時刻の出し方 | 条件 |
| --- | --- | --- |
| 樹脂（原神）・開拓力（スターレイル）の上限到達 | `resin_full_at` / `stamina_full_at` | 現在値が上限未満 |
| 洞天宝銭の上限到達（原神） | `home_coin_full_at` | 現在値が上限未満 |
| 派遣の完了（すべて完了した時点） | `fetched_at` + 未完了の派遣の残り秒数の最大値 | 未完了の派遣がある |
| 参量物質変化器の使用可能（原神） | `transformer_ready_at` | 入手済みで、まだ使用可能でない |

通知のタイトルは「◯◯が上限に達しました」「派遣がすべて完了しました」「参量物質変化器が使用可能です」です。本文は「ゲーム · UID」で、アプリの表示言語（日本語／英語）で作ります。

**設定画面**（設定 › 通知の「この端末への通知」カード。`pages/device_notifications.rs`。種類ごとの選択は「設定：通知」の表）

- 全体のON/OFF（既定はOFF。5.3節のサーバーからの通知と共通）。予測通知の種類ごとのON/OFF（端末の既定はすべてON）は、表の「この端末」の列を保存したときに合わせる。保存するまでは、以前の端末側の選択が残る。
- ONにしたとき、Android 13以降で通知が許可されていなければ `POST_NOTIFICATIONS` の許可を求める。拒否された場合は警告と、「通知を許可する」「端末の通知設定を開く」のボタンを表示する。
- 予定している通知の件数を表示する。
- 「テスト通知を表示」で、その場で1件通知する。「サーバーからテスト通知を送る」は5.3節。

**動作**（`app/device_notifications.rs`、`mobile/notify.rs`、`android/MainActivity.kt`）

- 状態、設定、言語のいずれかが変わるたびに予定を計算し直します。前回送った内容と違うときだけ、JNI経由でKotlinへ渡します。
- Kotlinは `AlarmManager.setAndAllowWhileIdle`（不正確なアラーム）で予約します。正確なアラームの権限は不要です。発火すると `AhdAlarmReceiver` が通知を出すので、アプリのプロセスが終了していても届きます。
- 通知チャンネルは `game_status`（名前「ゲームの状態」、重要度は既定）。通知アイコンは `res/drawable/ic_stat_ahd.xml` です。
- 通知をタップすると、既存のタスクを作り直さずにアプリを前面に出し、ダッシュボードを表示します（`CLEAR_TOP|SINGLE_TOP`）。
- 通知にはタグ「ゲームアカウントID:種類」を付けます。サーバーの同じ状態の通知（5.3節）と同じタグなので、片方が2時間以内に表示済みなら、もう片方は表示しません。
- 端末の再起動後とアプリの更新後は、`AhdBootReceiver`（`BOOT_COMPLETED`、`MY_PACKAGE_REPLACED`）が予約を戻します。
- 予約は配信済みのものを再び出さないようにしています。時刻を過ぎた予定も1時間は保持し、遅れて届くアラームを取り消さないようにしています。
- 予定を消すのは、ログアウト、セッションの期限切れ・失効（401）、ユーザー削除のときだけです。オフラインで起動した場合など、それ以外の理由でサインアウト状態になったときは予定を残します。別のユーザーでログインすると、前のユーザーの予定は消えます。

**保存先**（いずれもアプリ専用領域）

| ファイル | 内容 |
| --- | --- |
| `files/device-notifications.json` | 全体と種類ごとのON/OFF |
| `files/notification-schedule.json` | 予測した予定（所有ユーザー、アカウント、種類、時刻） |
| SharedPreferences `ahd_notifications` | Kotlin側の予約済み一覧とチャンネル名（再起動後の復元用） |

デイリー／週間任務の未完了、ログインボーナスの結果、Cookieの失効、ログインなどは時刻を予測できないため、予測通知では扱いません。これらは5.3節のサーバーからの通知で届きます。

### 5.3 通知（web版と同じ通知をサーバーから受け取る）

web版がWebhookへ送る個人の通知を、すべてアプリでも受け取れます。管理者専用の通知は除きます。通知ごとの条件・内容・受信者と、移植・除外の一覧は [notifications-inventory.md](notifications-inventory.md) にあります。

**仕組み**

- 「この端末に通知する」をONにすると、アプリはサーバーに端末を登録します（`POST /api/v1/me/notifications/app-devices`）。サーバーは、Webhookへ送るのと同じ箇所（`Notifier::deliver`）で、同じ通知をそのユーザーの受信箱に入れます。条件、重複排除、文面（通知設定の言語）はWebhookと同じです。通知する種類は、Webhook（`events`）とは別にアプリ用の選択（`app_events`）で決まります（既存の設定はWebhookの選択をコピーして移行）。
- Kotlinの `AhdInbox`（`android/MainActivity.kt`）が、Keystoreで暗号化して保存したセッションCookie（`AhdSessionStore`、5.7節）で受信箱を読み（`GET …/app-devices/{id}/inbox?after=…`）、Androidの通知を出します。読む時機は、JobSchedulerの定期ジョブ（15分ごと、ネットワーク接続時、アプリを閉じていても・再起動後も）、アプリの表示中の1分ごと、アプリに戻ったとき、登録直後とテスト送信直後です。
- FCMは使いません。サーバーに外部サービスの資格情報が要らず、Googleのサービスがない端末でも動きます。ポーリングだけでは、アプリを開いていないときに届くまで最大15分程度（Dozeではさらに遅れることがある）かかります。UnifiedPushの配信アプリがあれば、5.9節のプッシュですぐに読みに行けます。
- 通知チャンネルは5.2節と同じ「ゲームの状態」です。サーバーの重複排除キーをタグにし、同じタグの通知は置き換えます。
- タップすると、通知を扱う画面を開きます：ログインボーナスの結果 → 設定 › 報酬、Cookieの失効 → 設定 › 連携、ログイン・ログイン失敗 → 設定 › 認証・セキュリティ、テスト → 設定 › 通知、ゲームの状態 → ダッシュボード（`navigation::notification_target`）。
- 通知の許可は5.2節と同じです（ONにしたときにAndroid 13以降なら `POST_NOTIFICATIONS` を求め、拒否されたら警告と設定へのボタンを出す）。
- 設定カードに登録の状態（受け取っている／登録している／登録できなかった）を出します。登録に失敗した場合は、全体をOFFにしてONに戻すと再試行します。
- 「サーバーからテスト通知を送る」は `POST /me/notifications/test` を呼びます（Webhookを設定していればWebhookにも届く。web版の「テスト送信」と同じ）。
- OFFにすると端末の登録を消します（`DELETE …/app-devices/{id}`）。ログアウト、セッションの期限切れ・失効、ユーザー削除では端末側の受信だけを止めます（セッションがないためサーバーの登録は消せず、30日間読まれなければサーバーが消す）。別のユーザーでログインすると、そのユーザーで登録し直します。サーバーが端末を忘れていた（404）ときは受信を止め、次にアプリを開いたときに登録し直します。

**保存先**

| 場所 | 内容 |
| --- | --- |
| SharedPreferences `ahd_inbox` | 端末ID、所有ユーザー、サーバーのOrigin、読んだ位置、User-Agent |
| SharedPreferences `ahd_notifications` の `recent` | 直近2時間に表示したタグと出所（予測／サーバー） |
| サーバーの `app_devices`・`app_notifications` | 登録した端末と、端末向けに作った通知（3日で削除） |

**エミュレータでの確認**（API 34、x86_64、合成データのバックエンド。外部への通信は遮断）

サーバーの受信箱（`app_notifications`）に入った通知と、`dumpsys notification`・`cmd notification list` に出た通知を照合し、タップで開いた画面をスクリーンショットで確認しました。

| 操作 | 結果 |
| --- | --- |
| 天然樹脂・開拓力を上限にした状態キャッシュ（アプリ表示中） | `resource_full`（原神・スターレイル）と `daily_unfinished` が受信箱に入り、1分以内に表示された。同じタグの予測通知とは重ならなかった |
| web版のログインAPIへ誤ったパスワードを送る | `sign_in_failed` が表示され、タップで 設定 › セキュリティ が開いた |
| 失敗したログインボーナス実行を入れる（アプリはホーム画面へ移した状態） | `reward_failed` が受信箱に入り、`cmd jobscheduler run -f` で定期ジョブを動かすと、アプリが前面にないまま表示された。タップで 設定 › ボーナス が開いた |
| HoYoLAB連携を `cookie_invalid` にする（同上） | `cookie_invalid` が同じジョブで表示された。タップすると、連携が無効なのでアプリは初期設定のHoYoLAB連携の画面を出した（web版と同じく、連携が使えない間は初期設定へ誘導する） |
| `POST_NOTIFICATIONS` を取り消してから通知設定を開く | 警告と「通知を許可する」が出て、押すとAndroidの許可ダイアログが出た。許可すると「サーバーからの通知を受け取っています」に戻った |
| 「この端末に通知する」をOFF→ON | OFFでサーバーの端末登録が消え、アプリ側の登録とジョブも消えた。ONで新しい端末IDで登録し直した |

同じアプリの通知が4件以上になると、Androidが自動でグループにまとめます。確認では、まとめたグループをタップしたときにグループ内の通知がすべて消え、先頭の通知（`cookie_invalid`）の画面が開きました。アプリは通知を個別に出しているだけなので、この動作はAndroidによるものです。

### 5.3 ブラウザ経由のパスワード／パスキーログイン

- アプリは32バイトの乱数からPKCE S256のverifierとchallengeを作り、verifierと作成時刻だけをアプリ専用の `files/browser-login.json` に保存します。保存した要求の有効期間は10分です。
- ブラウザで `/app-login?challenge=…` を開きます。未ログインなら通常のログインを行い、ログイン済みのユーザー名と警告を表示してから、明示的な許可で `POST /api/v1/me/app-login` を呼びます。
- 「アプリに戻る」で `cc.attodao.ahd://login?code=…` を開きます。コードは単回使用・120秒有効で、verifierをURLには含めません。キャンセルすると通常のダッシュボードまたは初回設定に戻ります。
- `AhdBrowserLogin` が起動時または `onNewIntent` でコールバックを受け取り、`mobile/login_intent.rs` がJNIとWebViewイベントで一度だけ取り出します。`mobile/browser_login.rs` がURLと保存要求の期限を検査し、`app/browser_login.rs` が起動時のセッション確認完了を待って保存要求を消費し、`POST /api/v1/auth/app-login/exchange` にコードとverifierを送ります。
- `dashboard-ui/android/AndroidManifest.activity.xml` のintent-filterは `scripts/build-android.sh` が最初のactivity内へ一度だけ挿入します。既存のsingleTask・Activity寿命の対策は維持します。
- 安全性：verifierは端末から出ず、URLに載るのはchallengeと単回使用のコードだけです。コードは120秒で失効し、交換時に先に消費されるため、誤ったverifierでの再試行もできません。外部から開かれた `cc.attodao.ahd://login` は、アプリが保存した要求がなければ無視されます（攻撃者のコードは攻撃者のchallengeに結び付いているため、こちらのverifierでは交換できません）。カスタムschemeは他のアプリも登録できるため、許可画面ではログインするユーザー名と「AHDアプリから開いたときだけ続ける」警告を出し、ユーザーの明示的な操作でだけコードを発行します。交換時には許可したブラウザのセッションがまだ有効なことを確かめるので、ブラウザでのログアウトや管理者による復旧・無効化の後は未交換のコードも使えません。
- ログイン画面では、ユーザー名＋パスワードのフォームの下を区切り線で分け、全幅のSecondaryボタン「パスキーでログイン」「ブラウザでログイン」を縦に並べます（web共通の `dashboard.css` の `.auth-alternatives`）。どちらもブラウザの `/app-login` を開き、パスワードまたはパスキーでログインして許可するとアプリへ戻ります。案内文は両ボタンの下に1つ置きます。
- PKCE・コールバック・保存形式・期限の純粋関数は通常のネイティブ単体テストで検査します。ブラウザ回帰テストは許可とキャンセルまでを検査し、headless Chromiumではカスタムschemeを開きません。

#### ネイティブのパスキーAPIを使わなかった理由

Android Credential Manager（`androidx.credentials`）でアプリ内からパスキーを使う方法も調べましたが、今回はブラウザ経由にしました。

- Credential Managerでweb版と同じパスキー（RP ID＝サーバーのドメイン）を使うには、RP IDのドメインの `/.well-known/assetlinks.json` に、アプリの署名証明書のSHA-256と `delegate_permission/common.get_login_creds` を載せる必要があります。署名鍵はCIのsecret（`ANDROID_KEYSTORE_*`）で、デバッグビルドの鍵は環境ごとに違います。各サーバーの運用者が配布APKの証明書フィンガープリントを設定する仕組みが要ります。
- アプリからのassertionのoriginは `android:apk-key-hash:<証明書のハッシュ>` になります。webauthn-rs（0.5）は追加のoriginを完全一致で許可できるので技術的には対応できますが、サーバーに新しい設定（証明書ハッシュ）とassetlinks.jsonの配信を加える必要があります。
- Digital Asset Linksの検証はGoogleのサービスがインターネット越しに取得するため、localhostの検証環境では確認できません。エミュレータでもGoogleアカウント（Googleパスワードマネージャー）か別のパスキープロバイダが必要で、実際に動くことをこの環境で確かめられません。
- dxが生成するGradleプロジェクトに `androidx.credentials` の依存を後から足す必要があり、ビルドスクリプトの書き換えが増えます。

ブラウザ経由なら、既存のweb版のWebAuthn処理をそのまま使え、ブラウザが対応する方法（端末のパスキー、セキュリティキー、別端末のQRコード）がすべて使え、サーバーごとの証明書設定も要りません。代わりにブラウザとの往復が1回増えます。

### 5.4 ホーム画面ウィジェット用のインジケーター部品

ウィジェット本体（AppWidgetProvider）が使う共通部品として、横バー型と丸型（270°の円弧）のインジケーターを `dashboard-ui/android/kotlin/AhdIndicators.kt` に置きます。どちらも `RemoteViews` を返し、ウィジェットのレイアウト内の入れ物に `addView` して使います。使い方とオプションの一覧は [dashboard-ui/android/README.md](../dashboard-ui/android/README.md) の「Widget indicators」です。

- バー型：ラベルと数値（`120/200`）を1行に、その下にバー、任意で残り時間などの補足。丸型：中央に数値またはアイコン、円弧の開いた下部に残り時間（`3時間12分`）。丸型は88dpと56dpの2サイズ（API 31未満のRemoteViewsはビューの大きさを変えられないため）。
- 値なし（`null`）は空のトラックと「—」、上限の90%以上は「まもなく上限」の色、上限は上限の色で、数値も上限の色にします（ダッシュボードの `fill_level` と同じ規則）。色は原神がミント、スターレイルがラベンダー、まもなく上限・上限はダッシュボードのwarning色です（`res/values(-night)/ahd_indicator_colors.xml`）。
- 描画：RemoteViewsは独自のViewを描けず、ProgressBarの色をコードから変えられるのはAPI 31からです（アプリはAPI 24以上）。そのため、バーは `<clip>`、円弧は回転したring `<shape android:useLevel="true">` のdrawableを `ImageView.setImageLevel` で伸ばします。色はレイアウトの `android:tint` で `values`／`values-night` から取るので、ランチャーが再inflateすればダークモードの切り替えにも追従します。Canvasで描いたBitmapは、描いた時点の色と密度に固定され、更新のたびにランチャーへ画像を送るため使っていません。
- 文言は `values/ahd_indicator_strings.xml`（日本語、アプリと同じく既定）と `values-en`。ウィジェットは、アプリの言語（5.10節）の `createConfigurationContext` で選びます。ウィジェットの文言（`ahd_widget_*`）も同じく `values` が日本語、`values-en` が英語です。
- 確認：デバッグビルドには、全状態（値なし・0%・50%・90%・100%）を両ゲーム・ライト／ダークで並べるサンプル画面 `AhdIndicatorSamplesActivity` が入ります（`adb shell am start -n cc.attodao.ahd/dev.dioxus.main.AhdIndicatorSamplesActivity`、release用のmanifestには入れません）。規則（状態の判定、drawableのlevel、数値と残り時間の表記）と、レイアウトのidや色の定義は `devenv --profile android shell -- test-android-kotlin` がJVMで検査します（`dashboard-ui/android/tests/AhdIndicatorModelTest.kt`）。
- 確認した表示：エミュレータ（API 34、1080×2400）のサンプル画面で、密度420（日本語・英語）と密度160・文字200%、それぞれライト／ダーク。文字の欠けや円弧のつぶれはない。tintの既定モード（src_atop）では半透明のトラック色が白い図形の上で白く残るため、tintした全ビューを `tintMode="src_in"` にしている（JVM検査でも確認）。

### 5.5 起動時の更新確認

- `app/updates.rs` の `use_update_check` が `App` の起動時に `mobile::updates::check` を `use_future` で1回だけ走らせます。起動・ログインはこれを待ちません。
- `check` はForgejoの `GET /api/v1/repos/{owner}/{repo}/releases/latest` を1回呼び（接続10秒・全体30秒、リダイレクトなし、本文512 KiBまで）、`evaluate` でタグ（`v` 付きも可）をSemVer 2.0の順序で `CARGO_PKG_VERSION` と比べます。`draft`・`prerelease` やプレリリースのタグは `NotStable` として除外します。
- 比べるのはアプリの版（`dashboard-ui/Cargo.toml`、アプリのタグ `vX.Y.Z`）だけです。webの版（ルートの `Cargo.toml`、タグ `web-vX.Y.Z`）はForgejoリリースを作らないので `releases/latest` に出ず、webだけのリリースではアプリは更新を検知しません。誤ってwebのタグでリリースを作っても、タグが `v` + SemVer でないため `BadTag` になり、更新ありにはなりません（[versioning.md](versioning.md)）。
- アプリはwebのコンテンツをリモートで読み込みません。UIはAPKに組み込まれ、サーバーとはJSON API（`/api/v1/...`）でだけ通信します。そのためwebの更新（ブラウザ版UIの見た目など）はアプリの更新なしにはアプリへ届かず、webだけのリリースはアプリの更新を必要としません。共有UIの変更をアプリに出すにはアプリのリリースが要ります。webはアプリ1.8.0以降と互換を保つ規則です（[versioning.md](versioning.md) 2.5）。
- 「このアプリについて」のバージョンもアプリの版です。web版の同じページはwebの版（ルートの `Cargo.toml`）を表示し、Cargo.lockへのリンクはそれぞれのタグ（`vX.Y.Z` / `web-vX.Y.Z`、1.8.0以前は共通の `vX.Y.Z`）を指します（`pages/about.rs`）。
- 結果 `UpdateCheck` は `update_available`・`current`・`latest`・`release_notes`（リリース本文）・`release_url`・`asset`（`name`・`download_url`・`size`・`checksum_url`）です。アセットは `ahd-{os}-{version}-{arch}.{ext}`（Androidのarm64なら `ahd-android-X.Y.Z-arm64.apk`）と完全一致で選び、チェックサムは `{asset}.sha256`、なければ `SHA256SUMS` です。この端末向けのアセットがないリリースは更新ありにしません。
- 成功した結果は `Signal<Option<UpdateCheck>>` のcontextに入ります（未完了・失敗の間は `None`）。後続の確認ダイアログ・ダウンロード・インストールはこのcontextを読みます。失敗（`Network`・`Status`・`Malformed`・`NotStable`・`BadTag`）は `mobile/log.rs` でlogcatのタグ `AHD` に出すだけです。
- 取得先は既定で `https://git.attodao.cc` の `AttODao/AttOsHoYoDashboard`。ビルド時の環境変数 `AHD_UPDATE_FORGEJO_URL`・`AHD_UPDATE_REPOSITORY`・`AHD_UPDATE_TOKEN`（任意、`Authorization: token …`）で変えます。トークンはAPKに埋め込まれるため、読み取り専用で他に権限のないものに限ります。
- バージョン比較・アセット選択・draft/prerelease除外・webのタグの無視・不正な応答は、Forgejoの応答を模したJSONで通常の `cargo test` から検査します（`mobile/updates.rs`）。

### 5.6 更新の確認ダイアログとインストール

- 更新があると、現在／新しいバージョンと変更内容（HTMLではなくプレーンテキスト）を表示し、「今すぐ更新」「後で」「このバージョンをスキップ」を選べます。「今すぐ更新」の上に「インストールが終わるとAHDは閉じます。閉じたら、もう一度開いてください。」と表示し、インストール中の画面にも同じ文を出します。「後で」は次回起動時にまた確認します。スキップは `files/update-skip.json` に保存し、さらに新しいバージョンが出るとまた確認します。ダイアログは `fieldset.app-shell` の外なので、他の処理中も操作できます。
- APKは `cacheDir/updates` にストリーミングで保存し、進捗を表示します。ダウンロード・確認中はキャンセルできます。上限は200 MiB、チェックサム一覧は64 KiBです。接続10秒・読み取り30秒のタイムアウトを設け、ダウンロード全体の時間制限は設けません。
- `SHA256SUMS` または `{asset}.sha256` のSHA-256と照合します。一覧がない／対象のハッシュがない場合は更新を拒否します。リダイレクトは追いません（Forgejoはアセットを直接返します）。
- APKのパッケージ名、versionNameがリリースと一致すること、versionCodeがインストール済みより大きいこと、署名者の集合がインストール済みと一致することを確認します。正式ビルドは `dashboard-ui/android/release-signing-cert.sha256` のリリース鍵だけ、デバッグビルドは自身のデバッグ鍵だけを受け付けます。
- 確認ダイアログの「今すぐ更新」を押したとき、通知が許可されていなければ（Android 13以降）`POST_NOTIFICATIONS` を求めます（`AhdUpdater.requestNotificationPermission`。設定画面は開きません）。更新後にアプリを開き直す手段が「更新しました」の通知だけだからです。答えを待ってから次に進み、拒否しても更新は続けます。許可のダイアログの答え（`onRequestPermissionsResult`）はActivityがまだ一時停止中に届き、そこでページを更新するとWebViewへの反映（とダウンロード）が次にアプリへ戻るまで止まったため、`onResume` で続けます（ダイアログが出ずに答えが返ったときだけ、その場で続けます）。待たずに進むと、インストールの確認画面の下に許可のダイアログが残り、更新でアプリが閉じた後もホーム画面に出たままになりました（エミュレータで確認）。「もう一度試す」や設定から戻ったときには求めません。
- ダウンロードの前に「不明なアプリのインストール」の許可（`canRequestPackageInstalls`）を確かめます。未許可なら何もダウンロードせず、ダイアログに説明と「設定を開く」（`ACTION_MANAGE_UNKNOWN_APP_SOURCES`）を出します。許可してアプリに戻ると（`MainActivity.onResume` が更新ダイアログにイベントを送る）、そのままダウンロードに進みます。許可しないで戻った場合は同じ説明のままです。この判断（`update_install::plan`）は通常の `cargo test` で検査します。
- `REQUEST_INSTALL_PACKAGES` を宣言し、PackageInstallerのセッションへコピーしてインストールを依頼します。システムが確認画面（「このアプリを更新しますか？」）を出します。`USER_ACTION_NOT_REQUIRED` を指定していますが、AHDがinstaller of recordの場合もAndroid 14（エミュレータ）では確認画面が出ました（確認なしの更新にはなりません）。
- エラーとキャンセルで一時ファイルを削除し、次回起動時にも残りを掃除します。失敗はダイアログに表示し、再試行できます。インストールのセッションが取り消された（確認画面で「キャンセル」など）ときは検証済みのAPKを残し、「もう一度試す」はダウンロードせずに同じファイルで新しいセッションを作ります。ダイアログを閉じる・「後で」・スキップ・インストールの失敗で、このファイルも削除します。Androidがアトミックにインストールするので、失敗してもインストール済みアプリは変更されません。
- 許可を先に確かめるのは、Android 14（エミュレータ）では、インストーラーの案内から設定で許可して戻るとそのセッションが「取り消されました」になり、以前はAPK全体をダウンロードし直していたためです。一度AHDから更新すると、AHDがinstaller of recordになります。
- 更新後は `MY_PACKAGE_REPLACED` で「AHDを…に更新しました」の通知を出し、アプリの再表示も試みます。Android 10以降はバックグラウンドからのActivity起動が止められる（Android 14で `BAL_BLOCK` を確認）ので、通知のタップで開きます。通知が許可されていなければ通知も出ないため、ダイアログの「閉じたら、もう一度開いてください」が唯一の案内です。アプリ起動時にはこの通知を消します。
- 確認方法（エミュレータ、x86_64）: `AHD_UPDATE_FORGEJO_URL=http://localhost:<port>`・`AHD_UPDATE_REPOSITORY=o/r` を付けて旧版と新版（`dashboard-ui/Cargo.toml` の版だけ上げる）を同じworktreeでビルドし（同じデバッグ鍵になる）、`releases/latest`・APK・`SHA256SUMS` を返すモックを `adb reverse` でつなぎます。更新あり／なし・後で・スキップ・キャンセル・ダウンロード中断・SHA-256不一致・一覧なし・別の鍵で署名したAPK・インストールの取り消し・1.7.0→1.7.1の更新を確認しました。通知と「不明なアプリのインストール」の許可の扱いは、使い捨てのForgejo（`releases/latest` とアセット）とAPI 34のエミュレータで、権限なしの新規インストールから確かめました（通知を拒否→ダウンロードせずに許可の案内→設定で許可して戻ると自動でダウンロード→確認画面で取り消し→「もう一度試す」はダウンロードなしで確認画面→更新。APKの取得はForgejoのログで1回。通知を許可した場合はその答えの後にダウンロードし、更新後に「更新しました」の通知が出る）。

### 5.7 セッションCookieの暗号化保存

セッションCookie（Rustの `mobile/session.rs::Jar`、`{"名前":"値"}` のJSON）は、Android Keystoreの鍵で暗号化してから保存します。

- **鍵**：`AndroidKeyStore` の別名 `ahd_session_cookies`、AES-256、GCM、パディングなし。鍵は端末の外へ取り出せません。ユーザー認証（画面ロックの解除）を要求しないため、アプリを閉じていても端末がロック中でも、15分ごとの受信箱のジョブ（`AhdInbox`）と30分ごとのウィジェットのジョブ（`AhdWidgets`）がCookieを読めます。最初の保存時に作り、ログアウトしても消しません。
- **形式**：`files/session-cookies.enc` に、`AHDS`・形式番号1・IVの長さ・IV（12バイト、暗号ごとにKeystoreが選ぶ）・暗号文とGCMタグを並べたバイト列を保存します。AADは `cc.attodao.ahd session-cookies v1` です。書き込みは一時ファイルからの置き換えです。
- **処理の場所**：暗号化と復号はKotlinの `AhdSessionStore`（`android/MainActivity.kt`）が行います。Rustの `mobile/http.rs` は最初の要求のときに `read`、Cookieが変わったときに `write`、空になったときに `clear` を、ほかのJNI呼び出しと同じ方法（アクティビティのクラスローダー、`with_local_frame`、例外の消去。`mobile/notify.rs::call_on`）で呼びます。受信箱とウィジェットのジョブは `AhdSessionStore.cookieHeader` を直接呼びます。Android依存のない部分（形式、移行の判断、失敗の分類、JSONの検証、Cookieヘッダー）は `AhdSessionFormat` にあり、`android/test/AhdSessionFormatTest.kt` でテストします。
- **移行**：1.9.0までは平文の `files/session-cookies.json` に保存していました。最初に読んだとき、平文のファイルしかなければ、それを暗号化して `session-cookies.enc` に書き、平文のファイルを消します。そのため更新後もログインしたままです。暗号化に失敗したときは平文のファイルを残して次回もう一度試します。両方のファイルがあるとき（暗号化の後、削除の前に止まった場合）は、暗号化した方を使い平文のファイルを消します。JSONとして読めない平文のファイルは消して、ログアウトとして扱います。
- **復号できないとき**：鍵がない、別の鍵で暗号化されている（別の端末で復元したデータなど）、ファイルが壊れている場合は、ファイルを消してログアウトとして扱います。アプリは異常終了しません。次の `/me` が401になるので、通常のセッション切れと同じログアウトの理由（`messages.please_sign_in` など、`reminders::FORGETTING_SIGN_OUTS`）で通知の予定とウィジェットを消します。それ以外の消去はしません。
- **Keystoreの一時的な失敗**：Keystoreが応答しないなどの失敗（Keystoreは処理中や起動直後の失敗も `InvalidKeyException` で報告するため、`KeyPermanentlyInvalidatedException` 以外の `InvalidKeyException` もここに含めます）では、ファイルを残し、ログアウトとしては扱いません。アプリの要求は通信エラーと同じ扱いで失敗し（予定は残ります）、次の要求でもう一度読みます。ウィジェットのジョブは失敗した更新として既存の表示を残し、受信箱のジョブは再試行します。同じプロセスで5回以上、5分以上にわたって続けて失敗したときは、壊れたKeystoreでアプリが使えなくなり続けないよう、復号できないときと同じくファイルを消してログアウトとして扱います（再ログインした後はメモリ上のCookieで動きます）。
- **保存の失敗と排他**：Cookieが変わったのに保存できなかったときは、Rustが次の要求の後にもう一度保存します。ファイルの読み書きと移行は、アプリの要求とジョブのスレッドの間で1つのロックで排他します。
- **バックアップ**：`session-cookies.enc` と旧形式の `session-cookies.json`（とそれぞれの一時ファイル）を、クラウドバックアップと端末間の移行から除外します（`android/res/xml/ahd_backup_rules.xml` はAndroid 11以前の `fullBackupContent`、`ahd_data_extraction_rules.xml` はAndroid 12以降の `dataExtractionRules`。`scripts/build-android.sh` がmanifestの `<application>` に追加）。Keystoreの鍵はもともとバックアップされません。

### 5.8 戻るボタンとダイアログ

- ダイアログ（`ModalDialog`・`ConfirmDialog`・メニューのボトムシート・更新ダイアログ）が開いているときに戻るボタンを押すと、いちばん上のダイアログだけを閉じます。閉じ方はEscapeと同じで、各ダイアログの `onclose` を呼びます（メニューは閉じる、更新の案内は「後で」、確認ダイアログはキャンセル）。何も開いていなければ、前の画面へ戻ります。最初の画面では、これまでどおりバックグラウンドへ移します（`moveTaskToBack`）。
- 開いているダイアログの記録は `mobile/back.rs` の1か所です。アプリのダイアログはすべて `use_modal` → `mobile::dom::Modal` で開くので、`Modal` が開くときに登録し、破棄で外します（開いた順のスタック。いちばん上は最後に開いたもの）。ダイアログごとに戻るボタンの処理を書く必要はありません。
- 閉じてはいけない状態は、`ModalDialog` の `back_closes: false`（`use_modal` の引数）で明示します。今は更新ダイアログだけで、`Flow::back_closes` が、通知の許可の答え待ち・ダウンロード中・検証中・インストール中を「閉じない」にします。この間、戻るボタンは何もしません（下のダイアログも閉じず、前の画面にも戻りません）。ダイアログのボタン（キャンセル・閉じる）は従来どおり使えます。開いたまま状態が変わるので、`Modal::set_back_closes` で登録を更新します。
- 流れ：`MainActivity.kt` は `handleBackNavigation = false` でwryの `onKeyDown`（WebViewの履歴を先に戻す）を止め、`OnBackPressedCallback` で `window.__ahdBack()` を呼びます。ページ側（`mobile::back::use_back_key`、ルートで1回）がスタックで判断し（`OpenDialogs::back`、通常の `cargo test` で検査）、閉じる・何もしない・`AhdBack.navigate`（JNI。UIスレッドで `canGoBack` なら `goBack`、そうでなければ `moveTaskToBack`）のどれかを行います。ページが読み込まれる前で `__ahdBack` がないときは、Kotlinが直接 `navigateBack` します。
- 戻るボタンのイベントはActivityが前面（RESUMED）にあるときにしか届かないので、ダイアログを閉じるシグナルの書き込みは一時停止中には起きません（`onRequestPermissionsResult` のような停止中の更新の問題は起きません）。Activityを作り直すこともありません。
- web版のブラウザの戻るは変わりません（`back_closes` はweb版では使わず、`__ahdBack` もアプリだけが定義します）。
- 更新ダイアログの `dismiss`（Escape・背景のタップ・戻るボタン）は、状態を `peek` した値のガードを持ったまま書き込んでいたため、案内（Prompt）で閉じると `AlreadyBorrowed` でpanicし、次のWebViewのリクエストでアプリが異常終了していました。状態を先にコピーしてから書き込むように直しました。
- 確認方法（API 34のx86_64エミュレータ、実際の `input keyevent KEYCODE_BACK`／`input tap`）：メニュー（ダッシュボード・設定）、Cookie削除の確認ダイアログ（閉じてもCookieは残り、次の戻るで前の画面）、更新の案内（「後で」になる）・ダウンロード中（2回押しても閉じず、前面のまま、ダウンロードは続く）・キャンセル後の案内・失敗表示、何も開いていない最初の画面（バックグラウンドへ移り、同じActivityで戻る）。背景のタップで閉じる動作も変わらないことを確かめました。logcatに `FORTIFY`・`Fatal signal`・`panicked` は出ませんでした。インストール中と通知の許可待ちは、ダウンロード中と同じ `Flow::back_closes` の判断で、エミュレータでは確認していません。

### 5.9 UnifiedPushによるサーバーからのプッシュ

5.3節の受信箱に通知が入ったことを、UnifiedPush（[仕様 AND_3.0.0](https://unifiedpush.org/developers/spec/android/)）で端末に知らせます。デイリー／週間の未完了、ログインボーナスの結果、Cookieの失効など、予測できない通知が15分の確認を待たずに届きます。FCMやGoogleのサービスは使わず、利用者が選んだ配信アプリ（ntfyなど）とそのサーバーを使います。

**仕組み**

- 設定 › 通知の「この端末への通知」カードで、サーバーへの登録が済むと「UnifiedPushですぐに受け取る」（既定はOFF）が出ます。ONにすると、`AhdPush`（`android/kotlin/AhdPush.kt`）が次を行います。
  1. サーバーのVAPID公開鍵を読みます（`GET …/app-devices/{id}/push`）。
  2. P-256の鍵ペアと16バイトの `auth` を作り、配信アプリへ `org.unifiedpush.android.distributor.REGISTER`（`token`、`vapid`、`message`。Android 14以降は送信元の共有、それ以前は `pi`。旧仕様の配信アプリ向けに `application` も付ける）を送ります。
  3. 配信アプリの `NEW_ENDPOINT` を受けたら、エンドポイントと公開鍵・`auth` をサーバーへ送ります（`PUT …/push`）。
- サーバーは受信箱に通知を入れるたびに、暗号化した起床用のメッセージ（`{"type":"inbox"}` だけ。通知の内容や秘密は含まない）をエンドポイントへ送ります。
- `AhdPushReceiver` が `MESSAGE` を受けると、確認応答（`MESSAGE_ACK`）を返し、自分の鍵で復号できたときだけ `AhdInbox.poll` を `goAsync` のスレッドで実行して受信箱を読みます。復号は `AhdPushCrypto.kt`（RFC 8291、JCAだけ）です。復号できないメッセージは無視します。
- 重複：プッシュでもポーリングでも同じ `AhdInbox.poll`（同期化され、読んだ位置を保存する）を通るので、同じ通知が2回表示されることはありません。予測通知との重なりは5.2節のタグの規則のままです。
- 配信アプリの選び方：前回の配信アプリが入っていればそれ、1つだけならそれ、複数あれば `unifiedpush://link` の既定のアプリ、なければ名前順の最初です（選択画面は出しません）。
- 再登録：アプリの起動ごとに1回 `REGISTER` を送り直し（仕様の整合性確認）、配信アプリかサーバーのVAPID鍵が変わったら新しいtokenと鍵で登録し直します。`REGISTRATION_FAILED` の後は次回に新しいtokenを使います。
- 前面化：配信アプリが数秒バインドできるように、`org.unifiedpush.android.connector.RAISE_TO_FOREGROUND` のservice（`AhdPushForegroundService`）を公開します。
- OFFにすると、配信アプリへ `UNREGISTER` を送り、サーバーの購読を消します（`DELETE …/push`）。ログアウトでは、セッションがあるうちにサーバーの購読を消してからログアウトします。「この端末に通知する」をOFFにしたとき、ログアウト・セッションの失効、別のユーザーでのログインでも配信アプリの登録を解除します（スイッチの選択は残り、次の登録で使います）。配信アプリが `UNREGISTERED` を送ってきたときは、サーバーの購読を消して失敗の表示にします。
- Android 11以降のパッケージの可視性のため、`REGISTER` を受ける配信アプリと `unifiedpush://link` を `<queries>` に入れます（`android/AndroidManifest.queries.xml` を `build-android` が挿入）。receiverとserviceは `android/AndroidManifest.application.xml` にあります。
- Kotlinからページへの状態の通知は `AhdPageEvents`（`MainActivity.kt`）を通し、Activityが表示中（RESUMED）のときだけ送ります。表示中でない間のページ更新はWebViewを止めるためで（7節）、アプリに戻ったときにページが状態を読み直します。

**表示**（スイッチの下の1行）

| 状態 | 表示 |
| --- | --- |
| 配信アプリが入っていない | 「UnifiedPushのアプリ（ntfyなど）が入っていません。通知は15分おきの確認で受け取ります。」（警告） |
| 登録中 | 「{配信アプリ名}に登録しています…」 |
| 受け取り中 | 「{配信アプリ名}からすぐに受け取ります。」 |
| サーバーがエンドポイントを拒否（HTTPSの公開アドレスでない） | 「UnifiedPushのアプリのサーバーがHTTPSの公開アドレスでないため使えません。…」（警告） |
| その他の失敗 | 「すぐに受け取る設定ができませんでした。通知は15分おきの確認で受け取ります。」（警告） |

**保存先**

| 場所 | 内容 |
| --- | --- |
| SharedPreferences `ahd_push` | スイッチ、配信アプリ、token、登録に使ったVAPID鍵、Web Pushの鍵（秘密鍵はPKCS#8でアプリ専用領域に平文。この鍵で読めるのは秘密を含まない起床用のメッセージだけのため、Keystoreでは暗号化しない）、エンドポイント、状態。この端末の配信アプリに結び付くため、バックアップと端末間の移行から除外する（`ahd_backup_rules.xml`・`ahd_data_extraction_rules.xml`） |
| サーバーの `app_devices.push_subscription` | エンドポイントと公開鍵・`auth`（封をして保存。5.3節の端末と一緒に消える） |

**エミュレータでの確認**

attodeskのエミュレータ（AVD `ahd`、Android 14、x86_64）で、合成バックエンドと配信アプリntfy（F-Droid版）、公開の `ntfy.sh` を使って確かめました。エンドポイントはHTTPSの公開アドレスである必要があるため、バックエンドの外向き通信は `ntfy.sh:443` だけを通すプロキシ経由にしました（HoYoLABなどは遮断）。

| 確認 | 結果 |
| --- | --- |
| 配信アプリなし（ntfyを無効化） | 「UnifiedPushのアプリ（ntfyなど）が入っていません。…」を表示し、受信箱は15分ごとの確認のまま |
| ONにする | ntfyへ `REGISTER`、`NEW_ENDPOINT` を受けて `PUT …/push`、DBの `push_subscription` が入り、「ntfyからすぐに受け取ります。」 |
| アプリをバックグラウンドにし、受信箱のジョブを `cmd jobscheduler cancel` で消してから、テスト通知を送る | ポーリングなし（ジョブは登録されていない）で、送信の0.4秒後にAndroidの通知が出た |
| 同じ条件で、ワーカーがCookieの失効を検出する（`auth_status` を `cookie_invalid` に変更） | 受信箱に入ってから約0.5秒後に通知が出た（検出まではワーカーの周期の約56秒） |
| OFFにしてから、テスト通知を送る | 15秒たっても出ず、受信箱のジョブを実行すると出た（ポーリングだけで届き、2回は出ない） |
| ログアウト | ログアウトの前に `DELETE …/push` が届き、DBの `push_subscription` が空になった |
| Keystoreで暗号化したセッションCookie（5.7節）との組み合わせ | ログイン後は `session-cookies.enc` だけがあり、`AhdInbox.deviceRequest` の `GET`/`PUT …/push` と起床後の受信箱の読み込みがそのCookieで動いた |

ntfyはバッテリーの最適化から除外していないと、数時間後にサーバーとの接続が止まり、プッシュが届かなくなることがありました（ntfyを開くと再接続し、届くようになりました）。これは配信アプリ側の設定で、届かない間も15分ごとの確認で受け取ります。

### 5.10 言語の初期値（端末のロケール）

- 言語は、メニューで選んで保存したものが常に優先します。保存がない場合だけ、ロケールの一覧（優先順）から決めます。一覧を先頭から見て、最初に日本語（`ja`、`ja-JP` など）か英語（`en-*`）があればその言語、どちらもなければ英語です。一覧を読めなかったとき（空）は、従来どおり日本語です。規則は `i18n.rs` の `Language::for_locales`（純粋関数、テストあり）です。
- アプリの一覧は、Android 13以降でアプリ別言語（設定の「アプリの言語」、または `cmd locale set-app-locales`）があればそれ、なければシステムの言語です（Kotlin `AhdLocale.deviceLocales` を `mobile/locale.rs` のJNIで呼ぶ）。設定の「アプリの言語」に出るよう、`build-android` がmanifestに `android:localeConfig="@xml/ahd_locales"`（日本語・英語）を入れます。
- web版も同じ規則で、`navigator.languages`（なければ `navigator.language`）を使います。以前のweb版は `navigator.language` だけを見て、日本語以外はすべて英語でした（一覧の2番目以降の日本語は見ていなかった）。
- 保存は、メニューで言語を選んだときだけ行います（`Preferences::choose_language`）。以前は初回起動（初回訪問）で決めた言語もそのまま保存していたため、すでに保存がある利用者（従来のアプリとweb版の利用者のほぼ全員）の言語は変わりません。保存がない間は、端末の言語を変えるとそれに従います。アプリはActivityを作り直さないため、ページが再び表示されたとき（`visibilitychange`、`preferences::use_device_language`）にロケールを読み直します。web版は次の読み込みからです。
- 端末通知（5.2節）は、これまでどおりアプリの表示言語で作ります。ホーム画面ウィジェットのコードで入れる文言も、同じ言語（保存した言語、なければ同じ規則で決めた端末の言語）にそろえます。Kotlinの `AhdLocale` が `files/preferences.json` と端末のロケールから言語を決め（`languageFor` はRustと同じ規則で、`test/AhdLocaleTest.kt` が同じ例を検査）、その言語の `createConfigurationContext` で描きます。原神ウィジェットも、レイアウトの文字列（ゲーム名、樹脂、「全回復まで」、タップの案内）をコードから入れるようにしました。アプリは設定が変わるたびに `AhdLocale.languageChanged` を呼んでウィジェットを描き直します。アプリを開かずにアプリ別言語を変えた場合は、次の描画（30分ごとの更新など）から新しい言語になります（アプリ別言語の変更はアプリ自身にbroadcastされません）。
- サーバーからの通知（5.3節）の文面は、サーバーが通知設定の言語（Webhookと共通）で作るため、この規則の対象外です。
- 確認（エミュレータ API 34、新規インストール）：システムの言語を英語・日本語にして、それぞれログイン画面と、ウィジェットの未ログイン表示の言語を確かめました。詳細はPRの説明にあります。

### 5.11 接続先サーバーの変更

- ログイン画面の下に「接続先サーバー」の欄を置き、選択中のサーバーのOriginを表示する。「サーバーを変更」でアドレスの入力欄を開き、「確認して変更」で変える。既定のサーバー以外を使っているときは「既定のサーバーを入力」（ビルド時の `AHD_SERVER_URL` を入力欄に入れる）も出す。web版には表示しない（web版の接続先は常にページ自身のOrigin）。
- 入力の規則（`mobile/server.rs::origin_from_input`、純粋関数、テストあり）：前後の空白を除き、スキームがなければ `https://` を補う。HTTPSだけを受け付け、HTTPは `localhost`／`127.0.0.1` だけ（開発用。`network_security_config.xml` と同じ範囲）。認証情報（`user@`）付き、`https`/`http` 以外のスキーム、ホストのないものは使えない。パス・クエリ・フラグメントは捨て、Originだけを保存する（ページのURLを貼り付けても使える）。
- 変える前に、新しいサーバーの `GET /health` を確かめる（Cookieも `Origin` も送らない。15秒で打ち切り、応答は16KiBまで）。200の `{"status":"ok"}` か、503の `{"status":"degraded"}`（バックグラウンド処理の停止中）だけをAHDのサーバーとみなす（`health_answer`）。接続できない場合とAHDのサーバーでない場合は、それぞれの理由を入力欄の直下に表示し、何も変えない。
- 変えるときは、次の順で古いサーバーの状態をすべて消す（`app/server.rs::change`）。
  1. 古いサーバーのセッションCookieが残っていれば（通信エラーでログイン画面になった場合など）、5秒以内のベストエフォートで、この端末の受信箱の登録（UnifiedPushの購読を含む）を削除し、ログアウトを送る。応答がなくても続ける（サーバー側の端末登録は30日、セッションは期限で消える）。
  2. UnifiedPushの配信アプリの登録と鍵（「すぐに受け取る」のスイッチは端末の設定として残す）、受信箱の登録、表示中のゲーム状態の通知（更新の通知は残す）、ホーム画面ウィジェットのデータ、通知の予定（`notification-schedule.json`）、ブラウザ経由ログインの待ち（`browser-login.json`）を消す（`app/server.rs::forget_local_server_data`、`mobile/server.rs::PER_SERVER_FILES`）。どれか1つでも消せなければ変更をやめ、エラーを表示する（古いサーバーを選んだまま）。
  3. Cookie jarを空にし、Keystoreで暗号化した `session-cookies.enc` を消してから、新しいOriginを `files/server-origin.txt` に保存する（`http::switch_server`。jarのロックを持ったまま行う。既定のサーバーを選んだ場合はファイルを消すので、既定を変えたビルドに更新したとき、サーバーを選んでいない利用者は新しい既定に従う）。Cookieを消せなければ変更をやめ、エラーを表示する。古いCookieが新しいサーバーへ送られることはない。
  4. 画面の状態を新しい `AppState` に置き換え（ダッシュボードのデータを含む）、「接続先サーバーを変更しました。新しいサーバーにログインしてください。」と表示する。この理由は通知の予定と受信箱を忘れるサインアウトの理由（`FORGETTING_SIGN_OUTS`）に含まれる。
- 新しいサーバーを選択中として保存するのは、古いサーバーの状態をすべて消した後だけです。途中で失敗したり、アプリが終了したりした場合は、古いサーバーを選んだままになります。
- 要求はOriginとCookieを同じロックの中で読むので、あるサーバーへ別のサーバーのCookieを送ることはない。変更の後に古いサーバーから届いた応答の `Set-Cookie` は、jarに入れない（要求を送ったOriginが選択中のものと違う場合）。
- 言語、テーマ、どの端末通知を使うか、スキップした更新など端末の設定は残す。
- ブラウザで開く（5.1節）、ブラウザ経由のパスワード／パスキーログイン（5.3節）、受信箱、ウィジェットは、選択中のサーバーを使う。
- 更新の確認（5.5節・5.6節）は、接続先とは関係なく、ビルド時の `AHD_UPDATE_*`（既定は `git.attodao.cc` の `AttODao/AttOsHoYoDashboard`）のForgejoリリースを使い続ける。別のサーバーを選んでも、そのサーバーからAPKを取得することはない。
- サーバーの変更はログイン画面だけで行う（ログイン中は表示しない）。

## 6. web版との共通化と分岐

### 共通のまま使うもの

- 画面のコンポーネント（`pages/`）、状態管理（`app/state.rs`、`app/session.rs`、`app/actions.rs`、`app/data.rs`）、文言カタログ（`locales/*.json`）、`dashboard.css`、同梱フォント。
- `api.rs` のパス検査、応答サイズの上限、エラー処理（送信部分の `fetch` だけを分岐）。
- 画面とURLの対応（`navigation::page_path`／`page_from_path`）。アプリもWebView内のHistory APIを使うので、Androidの戻るボタンで前の画面へ戻れる。
- 言語とテーマの判定（`Preferences::from_saved`、言語の規則は `Language::for_locales`）。保存先と保存がない場合の言語の取得元だけが違い、web版は `localStorage` と `navigator.languages`、アプリは `files/preferences.json` と端末のロケール（5.10節）。テーマは保存がなければ端末（ブラウザ）のダークモードに従う。

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
| タブの横スワイプのタッチ収集 | web-sys（`pages/swipe_tabs.rs`） | `document::eval`（`mobile/swipe.rs`） | `pages/swipe_tabs.rs` の `SwipeableTabs`、判定は `app/swipe.rs` |
| 言語とテーマの保存 | `localStorage` | `files/preferences.json` | `app/preferences.rs` |
| 保存がない場合の言語の取得元 | `navigator.languages` | アプリ別言語→システムのロケール（`mobile/locale.rs`、Kotlin `AhdLocale.deviceLocales`） | `Preferences::load`、`Language::for_locales` |
| 外部URL | （なし） | `webbrowser` | `platform::open_external` |

`mobile/browser_login.rs`・`mobile/server.rs`・`mobile/session.rs`・`mobile/time.rs`・`mobile/reminders.rs`・`mobile/updates.rs`（通信部分を除く）、`mobile/update_install.rs`（ハッシュ・APK情報・スキップ・進捗の純粋な検証と、一時ファイルのSink）は通常のテスト環境で動くため、通常の `cargo test` でもコンパイルしてテストします（`main.rs` の `cfg(any(feature = "mobile", test))`）。

### 変更したファイル

新規（アプリ専用）：

| ファイル | 内容 |
| --- | --- |
| `dashboard-ui/src/platform.rs` | `APP`、`sleep_ms`、`now_ms`、`open_external` |
| `dashboard-ui/src/mobile/mod.rs` | アプリ用モジュールの宣言 |
| `dashboard-ui/src/mobile/http.rs` | サーバーへの直接接続、Cookie jarの読み込みと保存（Kotlinの `AhdSessionStore` をJNIで呼ぶ）、選択中のサーバー（`server_origin`・`switch_server`）と `/health` の確認 |
| `dashboard-ui/src/mobile/server.rs` | 接続先サーバーの入力の規則、保存値、`/health` の判定、サーバーごとのファイル（I/Oなし、テストあり。5.11節） |
| `dashboard-ui/src/app/server.rs` | 接続先サーバーの変更：確認、古いサーバーからの退出、サーバーごとの状態の消去（5.11節） |
| `dashboard-ui/src/pages/app_server.rs` | ログイン画面の「接続先サーバー」欄（5.11節） |
| `dashboard-ui/src/mobile/session.rs` | Cookie jarとサーバーURLの規則、保存先の一時的な失敗をログアウトと区別する規則（I/Oなし、テストあり） |
| `dashboard-ui/src/mobile/storage.rs` | アプリ専用領域のファイル（JNIで `getFilesDir()`・`getCacheDir()` を取得） |
| `dashboard-ui/src/mobile/time.rs` | 端末時刻での表示 |
| `dashboard-ui/src/mobile/locale.rs` | 端末のロケール一覧の取得とウィジェットへの言語変更の通知（JNI、5.10節） |
| `dashboard-ui/src/mobile/dom.rs` | `document::eval` によるDOM操作（pull-to-refreshのタッチ処理を含む） |
| `dashboard-ui/src/mobile/swipe.rs` | タブの横スワイプのタッチ収集（`document::eval`）。判定は共通の `app/swipe.rs` |
| `dashboard-ui/src/mobile/reminders.rs` | 通知時刻の予測、設定と予定の保存形式、通知文（テストあり） |
| `dashboard-ui/src/mobile/notify.rs` | Kotlinの通知処理（受信箱・UnifiedPushを含む）を呼ぶJNIブリッジ |
| `dashboard-ui/src/mobile/push.rs` | UnifiedPushの状態から表示する1行の決定（I/Oなし、テストあり） |
| `dashboard-ui/src/mobile/browser_login.rs` | ブラウザ経由ログインのPKCE・コールバックURL・保存要求の規則（I/Oなし、テストあり） |
| `dashboard-ui/src/mobile/login_intent.rs` | Kotlinが受け取ったコールバックを一度だけ取り出すJNIブリッジとWebViewイベント |
| `dashboard-ui/src/app/browser_login.rs` | ブラウザ経由ログインの開始と、コールバック受信後のコード交換 |
| `dashboard-ui/src/mobile/updates.rs` | Forgejoリリースの更新確認：SemVer比較、アセット選択、`releases/latest` の取得（5.5節、テストあり） |
| `dashboard-ui/src/mobile/update_install.rs` | APKのダウンロード、SHA-256・パッケージ・署名者の検証、一時ファイル管理、PackageInstallerへのJNI（5.6節、テストあり） |
| `dashboard-ui/src/mobile/log.rs` | logcat（タグ `AHD`）への出力 |
| `dashboard-ui/src/mobile/back.rs` | 戻るボタン：開いているダイアログのスタック（テストあり）、`window.__ahdBack` の受け取り、`AhdBack.navigate` へのJNI（5.8節） |
| `dashboard-ui/src/app/device_notifications.rs` | 端末通知の予定の更新、通知タップ時の画面遷移 |
| `dashboard-ui/src/app/updates.rs` | 起動時の更新確認と結果のcontext、ダイアログの状態・スキップ・ダウンロードタスクとキャンセル |
| `dashboard-ui/src/pages/app_update.rs` | 更新の確認・進捗・失敗・再試行のダイアログ |
| `dashboard-ui/src/pages/device_notifications.rs` | 「この端末への通知」カード |
| `dashboard-ui/src/pages/app_nav.rs` | 下部ナビゲーション `AppNav` と `AppHead`（モジュールは両ビルドでコンパイルするが、web版では描画しない） |
| `dashboard-ui/assets/mobile.css` | アプリ専用のスタイル |
| `dashboard-ui/android/MainActivity.kt` | `MainActivity`（戻るボタンをページへ渡す・ダイアログがなければ前の画面かルートでバックグラウンドへ（`AhdBack`）、設定変更のその場反映、破棄時のプロセス終了、通知タップの受け取り）、`AhdNotifications`、`AhdAlarmReceiver`、`AhdBootReceiver`、`AhdUpdater`（APK情報・PackageInstaller・更新後通知）、`AhdUpdateReceiver` |
| `dashboard-ui/android/AndroidManifest.application.xml` | 生成manifestに挿入するreceiver |
| `dashboard-ui/android/AndroidManifest.activity.xml` | 生成manifestのactivityに挿入する `cc.attodao.ahd://login` のintent-filter |
| `dashboard-ui/android/res/**` | ランチャーアイコン、通知アイコン、アプリ名、色、DayNightテーマ（`values/styles.xml`） |
| `dashboard-ui/android/README.md` | リソースの作り方 |
| `dashboard-ui/android/kotlin/*.kt` | ウィジェット用インジケーター（`AhdIndicators.kt`、`AhdIndicatorModel.kt`）、デバッグ用サンプル画面（`AhdIndicatorSamples.kt`）、UnifiedPush（`AhdPush.kt`、`AhdPushCrypto.kt`。5.9節）。`build-android` が生成プロジェクトへコピーする |
| `dashboard-ui/android/kotlin/AhdLocale.kt`, `android/test/AhdLocaleTest.kt`, `android/res/xml/ahd_locales.xml` | アプリ別言語・システムの言語の取得、ウィジェットの言語、設定の「アプリの言語」の一覧（5.10節） |
| `dashboard-ui/android/test/AhdPushCryptoTest.kt` | Web Pushの復号をRFC 8291 付録Aで確認するJUnit（`test-android`） |
| `dashboard-ui/android/AndroidManifest.queries.xml` | UnifiedPushの配信アプリを見つけるための `<queries>`（生成manifestの `<application>` の前に挿入） |
| `dashboard-ui/android/tests/AhdIndicatorModelTest.kt` | インジケーターの規則とレイアウトのJVM検査（`test-android-kotlin`） |
| `dashboard-ui/android/AndroidManifest.samples.xml` | デバッグビルドだけ生成manifestに挿入するサンプル画面のactivity |
| `scripts/build-android.sh` | APKのビルド（2節）。生成manifestへの `launchMode="singleTask"`・`configChanges`・`localeConfig` の書き換えとreceiverの挿入も行う |
| `docs/android-changes.md` | 本書 |

変更（web版と共有）：

| ファイル | 変更内容 |
| --- | --- |
| `dashboard-ui/src/main.rs` | `platform`・`mobile` モジュールの追加 |
| `dashboard-ui/src/api.rs` | `fetch` のアプリ分岐、関連する `cfg` の拡張 |
| `dashboard-ui/src/app.rs` | `AppHead`、端末通知のフック、起動時の更新確認とapp-shell外の更新ダイアログ、復帰時の再取得、`DocsPage` の除外、`platform` のタイマー・時刻の使用 |
| `dashboard-ui/src/app/state.rs` | `Page::web_only`、`reachable`、ステータス自動消去のタイマー |
| `dashboard-ui/src/app/navigation.rs` | アプリ内History、`open_web_version`、`web_page_url` |
| `dashboard-ui/src/app/data.rs` | APIトークン・管理者データを読み込まない分岐、`platform::sleep_ms` |
| `dashboard-ui/src/app/display.rs` | 時刻表示のアプリ分岐 |
| `dashboard-ui/src/app/preferences.rs` | 言語とテーマの保存のアプリ分岐、保存がない場合の端末のロケール、復帰時の読み直し（5.10節） |
| `dashboard-ui/src/pages/shell.rs` | `settings_tabs`、メニュー項目の分岐、ヘッダーの設定ボタン、`AppNav` |
| `dashboard-ui/src/pages/auth.rs` | パスワードフォーム、「パスキーでログイン」「ブラウザでログイン」、パスワード変更とweb版の許可画面への案内文 |
| `dashboard-ui/src/pages/app_login.rs` | web版の `/app-login` 許可画面（アプリのログイン要求を承認してコードを発行する） |
| `dashboard-ui/src/pages/security.rs` | パスワード変更・APIトークン・パスキー登録の除外、「ブラウザで開く」 |
| `dashboard-ui/src/pages/notifications.rs` | 端末通知カードの配置、種類ごとの「この端末」／「Webhook」の表、`app_events` の送信と予測通知への反映 |
| `dashboard-ui/src/pages/common.rs` | `external` アイコン、DOM操作・モーダルのアプリ分岐、戻るボタンで閉じるかどうか（`ModalDialog` の `back_closes`、`use_modal` の引数。web版では使わない） |
| `dashboard-ui/src/pages/dashboard.rs` | `matchMedia`・毎秒の時計のアプリ分岐、pull-to-refresh（更新ボタンの代わり） |
| `dashboard-ui/src/pages/mod.rs` | 新しいモジュールの宣言 |
| `dashboard-ui/locales/ja.json`, `en.json` | 24キーを追加（`shell.open_in_browser`、`shell.app_navigation`、`messages.could_not_open_the_browser`、`auth.change_password_in_browser`、`security.manage_in_browser`、`notifications.personal_hint_app`、`notifications.device_*` 18キー）。既存のキーは変えていない |
| `dashboard-ui/Cargo.toml` | `mobile` feature とoptional依存、Android向けの `jni`・`ndk-context`、dev-dependenciesの `chrono`・`url` |
| `dashboard-ui/Dioxus.toml` | `android_main_activity`、`[bundle]`、`[android]`、`[android.permissions]` |
| `Cargo.lock` | 上記の依存の追加 |
| `devenv.nix` | `android`・`android-emulator` プロファイル、`build-android`・`check-android`・`test-android-kotlin`（`kotlin` パッケージ） |
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
  - 最初の画面で戻るボタンを押すと、アプリを閉じずにバックグラウンドへ移します（ホームボタンと同じ）。それ以外の画面では、従来どおり前の画面へ戻ります。ダイアログが開いているときは、先にそれを閉じます（5.8節）。
  - ダークモード、文字サイズ、表示サイズ、太字、言語、キーボードなどの変更では、Activityを作り直さずにその場で反映します（`configChanges`）。WebViewは文字サイズを作成時にしか読まないため、変更時に `textZoom` を設定し直します。
  - 最近使ったアプリから消すなどでActivityが破棄されたときは、ネイティブの後始末を行わずにプロセスを終了します。次に開くと新しく起動します。
  - テーマはDayNightなので、アプリのテーマ「システム」は端末のダークモードに追従します（以前は常にライトでした）。
- **接続先サーバー**：同時に使えるサーバーは1つだけです。変えると古いサーバーからログアウトし、そのサーバーの通知とデータを端末から消します（5.11節）。HTTPのサーバー（localhostを除く）と、独自CAの証明書のサーバーには接続できません。
- **セッションCookieの保存**：Android Keystoreの鍵で暗号化して保存します（5.7節）。鍵はユーザー認証を要求しないため、端末のロック中でも受信箱とウィジェットのジョブが読めます。そのため、アプリとして動くコード（root化した端末で同じUIDとして動くものを含む）からは復号できます。暗号化が防ぐのは、ファイルだけを取り出した場合（バックアップ、ファイルのコピー）です。1.9.0以前のバージョンで作られたバックアップには平文のファイルが含まれている可能性があります。
- **パスワード表示の自動マスク**：web版は、表示中のパスワードを、ウィンドウや入力欄からフォーカスが外れたときとページが非表示になったときに伏せます。アプリではこの処理を実装していません（表示ボタンでの切り替えはできます）。
- **言語**：保存がない場合の言語は端末のロケールで決まります（5.10節）。サーバーからの通知（5.3節）の文面は、端末ではなく通知設定の言語に従います。ウィジェットピッカーに出る説明文は、ランチャーがシステム言語で解決します。
- **通知**
  - アラームは不正確で、Dozeなどにより数分遅れることがあります。
  - 予測はアプリが状態を読み込んだ時点の値に基づきます。ゲーム内で樹脂を使った場合などは、次にアプリを開くまで早すぎる通知が出ることがあります。バックグラウンドでの再取得は行いません。
  - 対象は、ダッシュボードで読み込んだアカウント（各ゲームで選択中のもの）です。
  - 強制停止やアプリの更新でOSがアラームを消します。次にアプリを起動したとき、またはパッケージ更新時のreceiverで戻ります。
  - 通知をタップしてダッシュボードへ移っても、スクロール位置はそのままです。
  - `dx serve` のAPKにはreceiverが入りません。
  - UnifiedPushで受け取れるのは、配信アプリのサーバーがHTTPSの公開アドレスのときだけです（サーバーがWebhook URLと同じ規則でエンドポイントを検査するため。LAN内のntfyなどは使えず、15分ごとの確認になります）。配信アプリが複数あるときに選ぶ画面はありません（5.9節）。
  - `ntfy.sh` は購読者のいなくなったトピックへの送信に404/410ではなく507を返すため、配信アプリ側で登録が消えた購読はサーバーで「失敗」のまま残ります。端末の登録が30日使われずに消えるときに一緒に消えます。
  - プッシュを受けた後の受信箱の読み込みは、receiverの `goAsync` の時間内で行います。通知が多くページをまたぐ場合、残りは次の確認で届きます。
  - セッションの失効（期限切れ・管理者による失効）では、セッションがないためサーバーの購読を消せません。配信アプリの登録は解除するので、プッシュサーバーが404/410を返せば次の送信で購読は消えます。ntfy.shは登録のなくなったトピックに507を返すため失敗として数えるだけで、購読は端末の登録と一緒に30日後に消えます（ログアウトでは先に消すので残りません）。
- **ビルド**：release署名にはCIの署名用secretが必要です（未設定ならビルドごとのデバッグ署名）。`dx serve` ではDioxusの既定のアイコンと名前になります。

### 未確認の項目

- 実機では確認していません（API 34のx86_64エミュレータだけで確認）。
- アプリのプロセスが生きたままシステムがActivityだけを破棄する場合は、エミュレータの「アクティビティを保持しない」設定が効かなかったため、最近使ったアプリから消す操作でしか確認していません。
- ステータスバーは、ライト・ダークどちらのテーマでもAppCompatの既定の灰色のままです。
- 実機のIMEでの入力は確認していません。adbで高速に入力すると文字が落ちることがあります。
- 初回設定の画面は、レイアウトの検査で確認していません（合成データに該当するユーザーがいないため）。
- API 34イメージのWebView 113では、ボトムシートの表示アニメーション（`@starting-style`）が動きません。新しいWebViewでの見え方は確認していません。

### 今後の課題

- release署名と配布方法（ストア、または直接配布）を決める。
- 実機で確認する。

## ホーム画面ウィジェット（基盤）

原神／スターレイルの3×3ウィジェットを追加しました。`app/home_widgets.rs` が選択アカウントと状態を `mobile/widgets.rs` のJNIでKotlinへ渡し、アプリ専用SharedPreferences `ahd_widgets` に保存します。描画とモデルは `dashboard-ui/android/kotlin/`、JVMテストは `dashboard-ui/android/test/` に分離しています。web版への動作変更はありません。

ウィジェットの面の色（`ahd_widget_surface`・`_text`・`_muted`・`_warning`）は `values`（ライト）／`values-night`（ダーク）にあり、インジケーターの色と同じく端末のダークモードに従います。

状態はSignedOut（未ログイン／ログアウト）、NoAccount（対象ゲームの表示アカウントなし）、Unavailable（状態未取得）、Ready（キャッシュ済み状態＋更新失敗フラグ）の4つです。既知の同じアカウントが未取得ならキャッシュを保持し、切り替わったアカウントに別のアカウントの値を表示しません。明示的なログアウトなどセッションを忘れる理由と、状態取得の401では保存を消去してジョブを停止します。一時的なオフラインによるサインアウトは保存を消しません。

原神ウィジェット（`GenshinWidget.kt`）は、上段に天然樹脂の丸型、下段に洞天宝銭・探索派遣・デイリー任務の小さな丸型を3列に並べ、残り時間（全回復・満タン・派遣の最短／最長）と追加報酬の受取状況、取得時刻を表示します。幅210dp・高さ250dp未満（文字サイズが1より大きいときはその倍率をかけた大きさ未満）では4本のバーに切り替えます。文字サイズの変更は通知されないため、切り替わるのは次の描画（アプリを開く、30分ごとの更新）からで、それまでの間は文字200%だと樹脂の残り時間が下で切れたままです。面の色はダッシュボードの原神カード、樹脂・宝銭の90%以上と上限、全員帰還した派遣、追加報酬が未受取の4/4のデイリーはwarning色です。未ログイン・アカウントなし・未取得では案内とタップの案内を、更新に失敗したときは見出しの時刻を「更新失敗」で表示します。詳細は `dashboard-ui/android/README.md` の「Genshin widget」です。

JobSchedulerはネットワーク接続時に30分周期でGETし、providerの `updatePeriodMillis` も30分です。ウィジェット全体のタップでアプリの `/dashboard?game=<genshin|starrail>` を開き、ダッシュボードはそのゲームのタブを選びます（`app/navigation.rs` の `target_game`。タブの選択は `AppState.dashboard_game` に置き、設定画面から戻っても保持します）。ログイン前に開いた場合もタブだけは先に選び、`/me` 後の状態の作り直しでも引き継ぎます。ジョブの時刻は不正確でDozeにより遅れ、取得間の樹脂・開拓力・宝銭は推定値です。選択はダッシュボードに追従し、サーバーの状態キャッシュTTLも適用されるため、常に最新の実値を保証しません。端末の言語を変えたときは、receiverの `LOCALE_CHANGED` で描き直します（文言はアプリの言語、5.10節）。タッチではタップしたタブに `:hover` が残り、共通のボタンのhover枠が選択されていないタブの下線に見えるため、`mobile.css` で選択中でないタブのhover表示を消しています（ウィジェットから別のゲームのタブを開いたときに目立つ）。

### スターレイルのウィジェット

`kotlin/StarRailWidget.kt` と `res/layout/ahd_widget_starrail.xml`。表示値は Android に依存しない `StarRailWidgetModel.kt`（JVMテスト `test/StarRailWidgetModelTest.kt`）で、取得時刻からの経過を描画時点へ推定します。

- ヘッダー：星のアイコンとゲーム名。
- 上段：開拓力を丸型インジケーター（中央に現在値、下に「/上限」）で表示し、右に「全回復まで」と稲妻アイコン付きの「◯時間◯分」（1時間未満は「◯分」。上限では「上限」を警告色）、「予備 ◯」（予備が上限なら警告色）、取得時刻（小さく、「HH:MM 更新」）を並べます。更新に失敗したときは取得時刻を「更新失敗 HH:MM」として警告色にし、前回の値を残します（文言は原神のウィジェットと同じ）。
- 狭い幅・大きい文字：円弧の右に残り時間が入らない大きさ（`StarRailWidgetLayout.forSize`。幅142dp＋90dp×文字サイズ、高さ148dp＋98dp×文字サイズ未満）では `ahd_widget_starrail_compact.xml` に切り替え、開拓力もバーにして「全回復まで ◯時間◯分」をその下に、予備と取得時刻を次の行に、依頼の残り時間もバーの下に出します。ゲーム名は1行のまま文字を縮めます（API 24/25は省略）。密度560（3×3が約200dp）と文字200%で残り時間が「あと9…」に省略されていたためです。通常レイアウトでも残り時間は「全回復まで」の下に1行で置くので、文字サイズを変えてから次に描画されるまで（文字サイズの変更は通知されないため、アプリを開くか30分ごとの更新まで）の間も、文字200%で残り時間は読めます。
- 下段：依頼（「完了数/派遣数・すべて完了までの残り時間」。すべて完了で「4/4・すべて完了」を警告色、派遣なしは「派遣なし」）、デイリー訓練（活躍度/500）、模擬宇宙（週間ポイント）を幅いっぱいのバー型インジケーターで表示します。バーを幅いっぱいにしたのは、3×3の最小幅（180dp）や大きい文字で値が切れないようにするためです。
- 警告色は「放置すると無駄になる・受け取りが必要」なもの（開拓力・予備開拓力の上限、依頼の全完了）だけに使い、デイリー訓練と模擬宇宙の達成はゲームの色のままにします。
- 配色はダッシュボードのスターレイルのカード（`values(-night)/ahd_widget_starrail_colors.xml`、ライト／ダーク）とインジケーターのラベンダーです。
- 未ログイン・アカウントなし・状態なし（取得失敗）では、原神と同じくアイコン・案内文・「タップしてAHDを開く」を表示します（ピッカーのプレビューは説明文）。描画で例外が起きた場合は、共通の案内表示に戻します。
- 宝銭の枠：スターレイルには洞天宝銭がありません。HoYoLABのリアルタイムノートで取得でき、上限まで蓄積して週ごとにリセットされる「模擬宇宙の週間ポイント」（`current_rogue_score`/`max_rogue_score`）をこの枠に採用しました。予備開拓力は開拓力と同じ資源なので開拓力の欄に併記し、「歴戦余韻」は残り回数で蓄積ではないため採用していません。
- 確認した表示（エミュレータ API 34、Pixel Launcher、1080×2400、日本語）：密度420の文字100%・200%、密度560の文字100%・130%で、両ウィジェットとも樹脂／開拓力の値と残り時間が欠けずに出ます。密度560と文字200%の組み合わせ（最大の表示サイズと最大の文字）は、どちらも下のバーが切れ、残り時間も省略されます（対象外）。文字が大きいとき、バーの名前（「デイリー…」「模擬…」）は値を優先して省略されます。
- ゲーム名と各ラベルはコードから設定します。レイアウトの文字列はランチャー側のシステム言語で解決されるため、アプリの言語（5.10節）で表示をそろえるためです。
- 依頼の上限はスターレイルの `total_expedition_num` です。アプリから渡す状態にも含めるため、`models.rs` の `DailySummary` に `total_expedition_num` / `accepted_expedition_num` を追加しました。

