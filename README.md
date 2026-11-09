# AttOsHoYoDashboard

原神・崩壊：スターレイルのHoYoLAB連携、天然樹脂・開拓力などの状態確認、ログインボーナスの手動／定期受け取りを提供するセルフホスト型ダッシュボード。パスワード／パスキー認証と管理者によるユーザー管理に対応します。

## 導入

Docker Compose v2とリリースイメージ `git.attodao.cc/attodao/attoshoyodashboard:latest` を使用します。構成はアプリ＋PostgreSQLで、アプリの公開先は `127.0.0.1:8080`、DBは非公開です。**アプリは1インスタンス限定**です。

### Composeの書き方

任意のディレクトリに `compose.yaml` を1つ用意します。空のDBボリュームの初回起動時に、PostgreSQLイメージが `POSTGRES_USER`・`POSTGRES_PASSWORD`・`POSTGRES_DB` からアプリ用ロールとDBを作成します。このロールはDBのスーパーユーザーになります（DBコンテナはこのアプリ専用とし、DBポートは公開しないでください）。

`compose.yaml` の例です。`<...>` の値を置き換えてください。秘密値を直接書くため `chmod 600 compose.yaml` とし、Gitなどには含めないでください。値に `$` を含める場合は `$$` と書きます。

```yaml
services:
  db:
    image: postgres:18-alpine3.23
    restart: unless-stopped
    environment:
      POSTGRES_USER: attos_dashboard
      POSTGRES_PASSWORD: "<DBパスワード>"
      POSTGRES_DB: attos_dashboard
    volumes:
      - db-data:/var/lib/postgresql
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -h 127.0.0.1 -U attos_dashboard -d attos_dashboard"]
      interval: 2s
      timeout: 5s
      retries: 30

  app:
    image: git.attodao.cc/attodao/attoshoyodashboard:latest
    restart: unless-stopped
    environment:
      DB_HOST: db
      DB_PORT: "5432"
      DB_NAME: attos_dashboard
      DB_USER: attos_dashboard
      DB_PASSWORD: "<DBパスワード>"
      INITIAL_ADMIN_USERNAME: admin
      INITIAL_ADMIN_PASSWORD: "<初期管理者の仮パスワード>"
      HOYOLAB_COOKIE_KEY: "<openssl rand -hex 32 の出力>"
      BIND_ADDR: 0.0.0.0:8080
      PUBLIC_ORIGIN: https://ahd.example.com
      RP_ID: ahd.example.com
      SESSION_TTL_DAYS: "7"
      TIMEZONE_OFFSET: "+09:00"
    ports:
      - "127.0.0.1:8080:8080"
    depends_on:
      db:
        condition: service_healthy

volumes:
  db-data:
```

- `app` の `DB_NAME`・`DB_USER`・`DB_PASSWORD` は、`db` の `POSTGRES_DB`・`POSTGRES_USER`・`POSTGRES_PASSWORD` と同じ値にします。
- `DB_HOST`・`DB_PORT`・`BIND_ADDR` はこの構成用の値で、通常は変更しません。UI配置先はイメージ側で設定済みです。

起動します。

```sh
docker compose up -d
curl --fail http://127.0.0.1:8080/health
```

起動時にDBマイグレーションと初期管理者作成を行います。初期管理者はユーザーが0人のときだけ作成され、初回ログインでパスワード変更が必要です。作成後は `INITIAL_ADMIN_PASSWORD` を空にできます。既存ユーザーの復旧は管理画面から行います。

### 環境変数

| 変数 | 用途・既定値 |
| --- | --- |
| `DB_NAME` / `DB_USER` | DB名／DBロール。`db` の `POSTGRES_DB`／`POSTGRES_USER` と同じ値。例では `attos_dashboard` |
| `DB_PASSWORD` | 必須。DBパスワード。`db` の `POSTGRES_PASSWORD` と同じ強固な値 |
| `INITIAL_ADMIN_USERNAME` | 初期管理者名。`admin` |
| `INITIAL_ADMIN_PASSWORD` | 初回必須の仮パスワード。12〜256 UTF-8バイト |
| `HOYOLAB_COOKIE_KEY` | 必須。Cookie暗号化鍵。乱数32バイトの64桁hex（`openssl rand -hex 32`） |
| `PUBLIC_ORIGIN` | 公開Origin。例：`https://ahd.example.com`。パスなし |
| `RP_ID` | パスキー用ドメイン。例：`ahd.example.com` |
| `SESSION_TTL_DAYS` | 固定有効日数。正の整数、`7`。アクセスによる延長なし |
| `TIMEZONE_OFFSET` | 自動受け取りの固定UTCオフセット。`+09:00`。夏時間非対応 |
| `TIMEZONE` | 任意。IANAタイムゾーン名（例：`Asia/Tokyo`、`Europe/Berlin`）。指定時は `TIMEZONE_OFFSET` より優先し、夏時間に対応 |
| 通知 | WebhookのURLは `HOYOLAB_COOKIE_KEY` から派生した鍵で保存するため、Cookie鍵と同じバックアップ・ローテーション運用が必要 |
| `RUST_LOG` / `LOG_FORMAT` | 任意。ログレベル（既定 `info`）／`json` で1行1JSONのログ |
| `METRICS_TOKEN` | 任意。16文字以上。設定時のみ `/metrics` を `Authorization: Bearer` 付きで公開 |
| `REWARD_HISTORY_DAYS` | 実行履歴の保持日数。1〜90、既定 `5` |
| `STATUS_CACHE_TTL_MINUTES` | ゲーム・報酬状態のキャッシュ有効分数。1〜60、既定 `5` |
| `CALENDAR_CACHE_TTL_MINUTES` | イベントカレンダー（開催中・開催予定のイベントと祈願／跳躍）のキャッシュ有効分数。5〜1440、既定 `60`。キャッシュ後に開始・終了した項目があれば期限前でも取り直し、取得に失敗した後は `STATUS_CACHE_TTL_MINUTES` の間取り直さない。イベントの進捗はこの分だけ遅れて反映される |
| `RESOURCE_HISTORY_DAYS` | 樹脂・開拓力の履歴（ゲーム状態を取得するたびの1件）の保持日数。7〜90、既定 `7`。ダッシュボードは直近7日間を集計するため7日未満にはできない。1時間ごとの保守で、この日数に90分（窓の直前の起点）を足したより古い行を消す |
| `CHALLENGE_CACHE_TTL_MINUTES` | 高難度コンテンツ（深境螺旋・混沌の記憶など）の要約のキャッシュ有効分数。5〜1440、既定 `60`。期間がキャッシュ後に終わった場合は期限前でも取り直し、一部のモードだけ取得に失敗した要約は `STATUS_CACHE_TTL_MINUTES` 後に取り直す |
| `REWARD_MAX_ATTEMPTS` / `REWARD_RETRY_DELAY_SECONDS` | 一時的な失敗の試行回数（1〜4、既定 `4`）／再試行までの秒数（1〜600、既定 `10`） |

HoYoLAB Cookieは環境変数ではなく画面から登録します。

### 公開先

HTTPSリバースプロキシから `http://127.0.0.1:8080` へパスを保持して転送します。UI・APIは同一Originで、サブパス設置は非対応です。

ローカル利用時は `PUBLIC_ORIGIN: http://localhost:8080`、`RP_ID: localhost` に変更します。IPリテラルやlocalhost以外のHTTPは使用できません。

| パス | 内容 |
| --- | --- |
| `/` | セッション確認後、未ログインなら `/login`、ログイン済みなら `/dashboard` |
| `/login` / `/dashboard` | ログイン／ダッシュボード。初期設定未完了時は設定ゲートを表示 |
| `/events` | イベント：開催中・開催予定のイベントと祈願／跳躍のタイムライン（認証必須。web版のメニュー先頭、アプリは下部バーのダッシュボードの隣） |
| `/settings` | `/settings/games` へ308転送 |
| `/settings/games` / `/settings/bonus` / `/settings/link` / `/settings/security` / `/settings/notifications` / `/settings/transfer` / `/settings/delete` | 各設定画面（認証必須。設定の移行と削除はweb版だけ） |
| `/admin` | 管理者画面（管理者権限必須） |
| `/about` | このアプリについて：作者・バージョン・ライセンス・Forgejoリポジトリへのリンク（認証必須） |
| `/docs` / `/openapi.json` | API仕様の閲覧／OpenAPI JSON |
| `/manifest.webmanifest` | ホーム画面追加用のWebマニフェスト |
| `/health` | DB疎通と報酬ワーカーの生存確認。正常時 `{"status":"ok","worker":"ok"}`、ワーカー停止時は503 |
| `/metrics` | `METRICS_TOKEN` 設定時のみ。Prometheus形式の件数・ワーカー状態・直近24時間に終わった報酬実行の結果・APIトークン数・アプリの端末数（UnifiedPushの有無別）と起動後のプッシュ送信結果 |

## 開発とUI設計

ビルド・検証は `devenv shell -- …` 経由で実行します。UI回帰は `devenv shell -- test-ui`、リリース用スクリプト（タグの判定とアセット公開）は `devenv shell -- test-release`。

Impeccableは `.pi/skills/impeccable/` にGit管理したプロジェクト専用スキルです。ルートの [PRODUCT.md](PRODUCT.md) と [DESIGN.md](DESIGN.md) が本実装のコンテキストです。

```sh
.pi/skills/impeccable/scripts/impeccable context --target dashboard-ui/src
# ランチャーの隔離検証（外部通信なし）
devenv shell -- node .pi/skills/impeccable/scripts/launcher.test.mjs
```

同梱のNoto Sans JPは表示文の文字だけを含むsubsetです。`dashboard-ui/locales/` の文言に新しい文字を加えると、UI unit testが未収録の文字を示して失敗します。その場合は `scripts/subset-fonts.sh`（取得元URL・SHA-256・実行方法を記載）で再生成し、出力されたファイル名を参照箇所へ反映します。

エンジンは初回に公開releaseからSHA-256を確認して `.impeccable/cache/` へ取得します（未追跡）。curl／wgetがない場合は既存のNodeを使用します。ユーザー共通設定・スキルは変更しません。Pi向け自動detector hookはないため、UI変更の最後にスキルの指示に従い手動検査します。

### Androidアプリ

ホーム画面ウィジェットとして原神／スターレイルの2つの3×3ウィジェットを用意しています（原神は樹脂・宝銭・探索派遣・デイリー任務、スターレイルは開拓力・依頼・デイリー訓練・模擬宇宙を表示）。タップでアプリのダッシュボードをそのゲームのタブで開きます。ダッシュボードの選択アカウントと状態を共有し、30分周期で更新します。先に `build-android` で生成した後、`devenv --profile android shell -- test-android` でモデルのJVMテストを実行できます。

`dashboard-ui` はDioxusのmobile機能でAndroidアプリとしてもビルドできます（Cargo feature `mobile`。web版は既定の `web` featureのまま）。画面構成はweb版とほぼ同じですが、次の機能はアプリに含めず、web版でのみ使えます。

- 管理者機能（ユーザー管理・管理者向け通知設定）、APIドキュメント（`/docs`）、ユーザー削除
- 認証・セキュリティのAPIトークンとパスワード変更（初回／復旧後のパスワード変更もブラウザで行い、アプリでは「設定状況を更新」を選びます）
- WebAuthnはブラウザ専用のため、パスキーでのログイン・登録はアプリにはありません（登録済みパスキーの一覧・名前変更・削除は可能）

アプリのメニューの「ブラウザで開く」は、表示中の画面に対応するweb版のページ（選択中のサーバーのオリジン＋同じパス。既定は `AHD_SERVER_URL`）を端末の既定のブラウザで開きます（Androidの `Intent.ACTION_VIEW`、`webbrowser` クレート経由）。初回設定のパスワード変更画面と「セキュリティ」設定にも同じボタンがあります。ブラウザを開けなかったときはエラーを表示します。web版にはこのボタンはありません。

アプリはビルド時に指定した1つのサーバーへRust（reqwest、rustls）から直接接続します。ログイン応答のセッションCookieを、Android Keystoreの取り出せないAES-GCM鍵で暗号化してアプリ専用領域（`getFilesDir()` の `session-cookies.enc`）に保存し、以後の要求に `Cookie` と、サーバーが要求する `Origin`（＝サーバーのOrigin）を付けます。保存したCookieはバックアップと端末間の移行から除外します。1.9.0までの平文のファイルは最初の起動で暗号化して消すため、更新後もログインしたままです。復号できないファイル（別の端末で復元したデータなど）はログアウトとして扱います。ログアウトや期限切れで削除されます（[docs/android-changes.md](docs/android-changes.md) 5.7）。サーバー側の変更はありません。

#### 通知

「通知」ページの「この端末に通知する」をONにすると、web版のWebhookと同じ個人の通知（ログインボーナスの結果、Cookieの失効、ログイン、樹脂・開拓力、派遣、参量物質変化器、デイリー・週間）を、種類ごとの選択（Webhookとは別。サーバーの `app_events`）に従ってこの端末に表示します。種類ごとの選択は、同じページの表で「この端末」と「Webhook」の列を並べて設定します。アプリはサーバーに端末を登録し、受信箱（`/me/notifications/app-devices/{id}/inbox`）をセッションCookieで15分ごと（JobScheduler。アプリの表示中は1分ごと）に読みます。UnifiedPushの配信アプリ（ntfyなど）が入っていれば、「UnifiedPushですぐに受け取る」をONにすると、受信箱に通知が入ったときにサーバーが暗号化した起床用のプッシュ（通知の内容は含まない）を送り、アプリはすぐに受信箱を読みます。FCMは使いません。配信アプリがない場合やプッシュが届かない場合は15分ごとの確認で届きます。タップすると通知を扱う画面（報酬・連携・セキュリティ・ダッシュボード）を開きます。管理者専用の通知は対象外です。一覧は [docs/notifications-inventory.md](docs/notifications-inventory.md) にあります。

これとは別に、樹脂・開拓力と洞天宝銭の上限到達、派遣完了、参量物質変化器は、取得した状態から予測した時刻にAndroidのローカルアラームでも通知できます（同じ内容のサーバーの通知が2時間以内に届いた場合は片方だけを表示）。アラームは不正確でDozeにより遅れる場合があり、予測の更新はアプリが状態を読み込んだときだけです。強制停止すると次にアプリを起動するまでアラームと受信箱の確認は止まります。`dx serve` のAPKにはreceiverとジョブが入らないため、通知には `build-android` のAPKを使います。

必要なもの：devenvの `android` プロファイル（Android SDK 34・build-tools 34.0.0・NDK・JDK 17。初回は数GBを取得し、Android SDKのライセンスに同意したものとして扱います）と、Gradleが依存を取得するためのネットワーク。Rustの `aarch64-linux-android`／`x86_64-linux-android` targetは `rust-toolchain.toml` で入ります。

```sh
# 実機（arm64）向けAPK。AHD_SERVER_URL はHTTPSのOrigin（パスなし）
AHD_SERVER_URL=https://ahd.example.com devenv --profile android shell -- build-android
# x86_64エミュレータ向け
AHD_SERVER_URL=https://ahd.example.com AHD_ANDROID_TARGET=x86_64-linux-android devenv --profile android shell -- build-android
# 静的検査（clippy、mobile feature、aarch64-linux-android）
devenv --profile android shell -- check-android
# ウィジェット用インジケーターの規則とレイアウトの検査（JVM、SDK不要）
devenv --profile android shell -- test-android-kotlin
```

APKは `target/dx/attos-dashboard-ui/debug/android/app/app/build/outputs/apk/debug/app-debug.apk` に出力されます（デバッグ署名。`--release` を付けても署名設定がなければデバッグ署名です）。`adb install -r <APK>` で導入します。`build-android` は `dx build --platform android` の後、`dashboard-ui/android/res/` のアイコン・アプリ名「AHD」・テーマで生成プロジェクトを上書きし、AndroidManifestの `configChanges` を広げ、バージョンから `versionCode` を設定してGradleを再実行します（`dx serve --platform android` ではDioxus既定のアイコン・名前になり、ダークモードや文字サイズの変更でアプリが終了します）。

アプリのタグ `vX.Y.Z`（`dashboard-ui/Cargo.toml` の版）をpushすると `.forgejo/workflows/android.yml` がarm64のAPKを `--release` でビルドし、そのタグのForgejoリリースに `ahd-android-X.Y.Z-arm64.apk` と `SHA256SUMS` を添付します。接続先はリポジトリ変数 `AHD_SERVER_URL`（未設定なら `https://hoyo.attodao.cc`）です。APKはリポジトリのsecret `ANDROID_KEYSTORE_BASE64`（base64のkeystore）・`ANDROID_KEYSTORE_PASSWORD`・`ANDROID_KEY_ALIAS` のリリース鍵で署名し直し、証明書が `dashboard-ui/android/release-signing-cert.sha256` と一致することを確認します（secretがないと正式リリースは失敗し、テスト用タグだけはデバッグ鍵で続けます）。`versionCode` は `X*10000+Y*100+Z`（Y・Zは100未満）を `build-android` が設定します。命名規則・タグとバージョンの対応・テスト用タグ・リリース手順は [docs/releases.md](docs/releases.md) にあります。

アプリは起動時にバックグラウンドで1回、`GET https://git.attodao.cc/api/v1/repos/AttODao/AttOsHoYoDashboard/releases/latest` を呼び、タグ（`vX.Y.Z`）をSemVerとしてアプリのバージョンと比べます。比べるのはアプリの版（`dashboard-ui/Cargo.toml`）と、アプリのタグ `vX.Y.Z` のリリースだけです。webのタグ `web-vX.Y.Z` はForgejoリリースを作らないので `releases/latest` に出ず、webだけを上げても更新は検知されません（[docs/versioning.md](docs/versioning.md)）。draft・prerelease（フラグまたはSemVerのプレリリースのタグ）は対象外です。新しいリリースにこの端末向けのAPK（`ahd-android-X.Y.Z-<arch>.apk`）がある場合だけ「更新あり」とし、最新バージョン・リリースノート・APKとチェックサム（`.sha256` か `SHA256SUMS`）のURLを返します。起動は待たず、オフラインやAPIエラーはlogcat（`adb logcat -s AHD`）に出すだけです。取得先はビルド時の `AHD_UPDATE_FORGEJO_URL`・`AHD_UPDATE_REPOSITORY`（`owner/repo`）で変えられ、非公開リポジトリには `AHD_UPDATE_TOKEN` を渡せます（APKに埋め込まれて取り出せるため、読み取り専用で他に権限のないトークンに限る）。更新ダイアログの「今すぐ更新」は、通知の許可を求め（更新後はアプリが閉じ、Androidは自動で開き直せないため、「更新しました」の通知と「もう一度開いてください」の案内で知らせます）、「不明なアプリのインストール」が未許可ならダウンロードの前に設定画面へ案内し、戻ってきたら続けます。取り消されたインストールは、検証済みのAPKのまま再試行します（[docs/android-changes.md](docs/android-changes.md) 5.5）。

- `AHD_SERVER_URL` は既定の接続先です。利用者はログイン画面の「接続先サーバー」で別のAHDサーバーのOriginに変えられます（`GET /health` で確認し、変えると古いサーバーのセッション・通知・ウィジェット・通知の予定・画面のデータを消します。詳細は [docs/android-changes.md](docs/android-changes.md) 5.11節）。更新の確認は接続先と関係なく `AHD_UPDATE_*` のForgejoリリースを使います。
- `AHD_SERVER_URL` と利用者が入力するサーバーは、HTTPS、またはlocalhostのHTTPのみ。未指定時は `http://localhost:8080` で、開発用サーバー（`PUBLIC_ORIGIN: http://localhost:8080`）に `adb reverse tcp:8080 tcp:8080` でつなぐ想定です。
- エミュレータは `devenv --profile android --profile android-emulator shell`（x86_64イメージ、KVMが必要）で入ります。
- 言語はメニューで選んだもの（保存）が優先し、保存がなければ端末のロケールで決まります。Android 13以降のアプリ別言語（設定の「アプリの言語」）、なければシステムの言語の一覧を先頭から見て、最初の日本語か英語、どちらもなければ英語です。web版も同じ規則で `navigator.languages` を使います。端末通知とホーム画面ウィジェットの文言も同じ言語です（[docs/android-changes.md](docs/android-changes.md) 5.10）。
- パッケージ名 `cc.attodao.ahd`、minSdk 24、targetSdk 34（`dashboard-ui/Dioxus.toml`）。権限はINTERNET（dx既定）、更新用の `REQUEST_INSTALL_PACKAGES`、通知用の `POST_NOTIFICATIONS` と `RECEIVE_BOOT_COMPLETED`（再起動後のアラームと受信箱ジョブの復元）です。
- tao/wryはActivityの破棄・再生成や二つ目の起動に対応しておらず、その場合プロセスが異常終了します。そのためActivityは1つだけ（`singleTask`）で、戻る操作は開いているダイアログがあればいちばん上のものを閉じ（更新のダウンロード中・インストール中は閉じない）、最初の画面ではアプリを閉じずにバックグラウンドへ移し（ホームと同じ）、ダークモード・文字サイズ・表示サイズ・言語・キーボードなどの変更はActivityを作り直さずにその場で反映します。最近使ったアプリから消すなどで破棄されたときはプロセスをそのまま終了し、次回は新しく起動します（`dashboard-ui/android/MainActivity.kt`）。テーマ「システム」は端末のダークモードに追従します。

web版からの変更点（除外した機能とその方法、画面ごとのレイアウト、追加機能の仕様、共通化と分岐、web版への影響、既知の制約）は [docs/android-changes.md](docs/android-changes.md) にまとめています。

### UIモジュール

| パス | 責務 |
| --- | --- |
| `dashboard-ui/src/app/navigation.rs` | 通常パス・History API・認証状態に応じた転送と戻る／進むの対応 |
| `dashboard-ui/src/app/home_widgets.rs` | 選択アカウント・状態とAndroidホーム画面ウィジェットの同期 |
| `dashboard-ui/android/kotlin/` | ウィジェットのモデル・パーサー・推定、保存・JNI・更新ジョブとゲーム別provider（表示値の判断は `GenshinWidgetModel.kt`／`StarRailWidgetModel.kt`、描画は `GenshinWidget.kt`／`StarRailWidget.kt`） |
| `dashboard-ui/android/test/` | ウィジェットモデルとセッションCookieの保存形式・移行判断（`AhdSessionFormatTest.kt`）のKotlin JVMテスト |
| `dashboard-ui/src/app/state.rs` | 画面状態、認証・初期設定ゲートと画面データ取得の起動 |
| `dashboard-ui/src/app/session.rs` | `/me`による起動・再確認、初期設定ゲートのその場での再読み込み、ログアウト |
| `dashboard-ui/src/app/actions.rs` | 操作の予約（busy）と完了・APIエラー処理。セッション世代とは独立。完了後、busyで失われたフォーカスを操作したボタンへ戻し、失敗時はエラー表示へ移す（web） |
| `dashboard-ui/src/app/data.rs` | セッション世代に結び付いた画面データの取得（未取得と空を区別、リソースごとに最新の要求の応答だけを反映）、全ゲームの手動受け取りとその後の報酬状態のGET追従 |
| `dashboard-ui/src/app/display.rs` | 数値・ブラウザのローカル時刻による簡潔な月日／24時間表示、データの経過時間、上限への近さ、サーバー時刻とブラウザ時刻の換算、エラーコードの表示文への対応付け |
| `dashboard-ui/src/pages/` | 画面、メニュー、設定、ユーザー管理、このアプリについて、アプリ更新ダイアログ（`pages/app_update.rs`）、設定の移行（`pages/transfer.rs`：書き出し・読み込みのプレビューと反映。web版だけ） |
| `dashboard-ui/src/app/settings_file.rs` | 設定の移行のブラウザ側：書き出したJSONのダウンロードと、選んだファイルの読み込み（16 KiBまで・JSONとして読めること）。web-sys、web版だけ |
| `dashboard-ui/src/app/swipe.rs` | タブの横スワイプの判定（距離・速度・縦横比・画面端の除外としきい値）と移動先のタブ。web版とアプリで共通、テストあり |
| `dashboard-ui/src/pages/swipe_tabs.rs` | タブのパネルを包む `SwipeableTabs`（`use_horizontal_swipe_tabs`）：左スワイプで次・右スワイプで前のタブ、端ではバウンス、切り替えアニメーション。web版のタッチ収集（web-sys） |
| `dashboard-ui/src/platform.rs` | web版とAndroidアプリの切り替え（`APP`）、両方で動くタイマーと時刻、web版を既定のブラウザで開く処理 |
| `dashboard-ui/src/mobile/` | Androidアプリ専用：サーバーへの直接接続とセッションCookieの読み込み・保存（`http.rs`・`session.rs`。暗号化はKotlinの `AhdSessionStore`）、接続先サーバーの入力の規則・`/health` の判定・サーバーごとのファイル（`server.rs`）、アプリ専用領域のファイル（`storage.rs`）、端末時刻（`time.rs`）、保存がない場合の言語に使う端末のロケール（`locale.rs`、Kotlin `AhdLocale`）、通知予測とJNIブリッジ（`reminders.rs`・`notify.rs`）、ウィジェットへ渡す内容とJNI（`widgets.rs`）、サーバーの受信箱への登録の判断（`inbox.rs`）、UnifiedPushの状態の表示（`push.rs`）、Forgejoリリースによる更新確認（`updates.rs`、インターフェースはファイル先頭のコメント）、APKのダウンロード・SHA-256とAPKの検証・PackageInstaller（`update_install.rs`）、logcatへの出力（`log.rs`）、戻るボタンと開いているダイアログのスタック（`back.rs`）、web版がweb-sysで行うDOM操作の `document::eval` 版（`dom.rs`）、タブの横スワイプのタッチ収集（`swipe.rs`） |
| `dashboard-ui/src/app/device_notifications.rs` | 端末通知の設定・予定の保存、状態変更時の再計算、サーバーの受信箱への端末登録と解除、表示中の確認、通知からの画面遷移 |
| `dashboard-ui/src/app/updates.rs` | アプリ起動時の更新確認（`mobile::updates::check` をバックグラウンドで1回実行し、結果を `Signal<Option<UpdateCheck>>` のcontextで渡す。確認失敗はログのみ）、更新ダイアログの状態・スキップ・通知と「不明なアプリのインストール」の許可の確認（設定から戻ると続行）・ダウンロードとキャンセル・取り消されたインストールの検証済みAPKの保持 |
| `dashboard-ui/src/pages/device_notifications.rs` | アプリの端末通知カード（全体のON/OFF・許可・登録状態・テスト） |
| `dashboard-ui/src/app/server.rs` | アプリの接続先サーバーの変更：アドレスの検査と `/health` の確認、古いサーバーからの退出（受信箱の登録解除・ログアウト）、サーバーごとの状態（Cookie・プッシュ・受信箱・通知・ウィジェット・予定・画面のデータ）の消去 |
| `dashboard-ui/src/pages/app_server.rs` | ログイン画面の「接続先サーバー」欄（アプリだけ） |
| `dashboard-ui/android/res/` | ランチャーと通知のアイコン、アプリ名、DayNightテーマ、開発用サーバー（localhost）へのHTTPを許可するnetwork security config、セッションCookieをバックアップから除外する規則（`xml/ahd_backup_rules.xml`・`xml/ahd_data_extraction_rules.xml`）（`build-android` が生成プロジェクトへ上書き） |
| `dashboard-ui/android/kotlin/` | `MainActivity.kt` の隣へ `build-android` がコピーするKotlin：ホーム画面ウィジェット用のバー型・丸型インジケーター（`AhdIndicators.kt`、規則は `AhdIndicatorModel.kt`）と、デバッグビルドだけのサンプル画面（`AhdIndicatorSamples.kt`）、UnifiedPushの接続（`AhdPush.kt`：配信アプリへの登録・receiver・前面化用service）と受信側のWeb Push復号（`AhdPushCrypto.kt`、RFC 8291。JVMテストは `android/test/`）。使い方は `dashboard-ui/android/README.md`、JVMの検査は `android/tests/` と `test-android-kotlin` |
| `dashboard-ui/android/MainActivity.kt` | AndroidのActivity（`Dioxus.toml` の `android_main_activity`）：ルートでの戻るをバックグラウンド化、設定変更時の文字サイズ反映、破棄時のプロセス終了、通知の権限要求・アラーム・receiver、セッションCookieをKeystoreで暗号化して保存する `AhdSessionStore`（形式と判断は `AhdSessionFormat`）、サーバーの受信箱を読む `AhdInbox` とそのジョブ（receiverとジョブの設定は `android/AndroidManifest.application.xml`、UnifiedPushの配信アプリを見つけるための `<queries>` は `android/AndroidManifest.queries.xml` を生成manifestへ挿入） |
| `dashboard-ui/src/pages/events.rs` | 「イベント」画面（`/events`）：ゲームごとのパネル（900px以下はゲームタブとスワイプ）、カレンダーの経過時間とweb版の更新ボタン（アプリは下に引っ張って更新）、読み込み中・取得失敗・表示するUIDなしの表示。カレンダーは各ゲームの表示UID（未選択なら最初の表示UID）を通して画面を開いたときに読み込む |
| `dashboard-ui/src/pages/calendar.rs` | イベントカレンダーの中身の判断：開催中（祈願／跳躍とイベント）と開催予定への振り分け、開催中・開催予定の件数と次の終了までの時間の要約、残り時間（共通の `Clock` で毎秒更新）、名前・種類・進捗、ピックアップ（公式画像CDNの画像と名前）、取得失敗・古いカレンダーの注記 |
| `dashboard-ui/src/pages/event_timeline.rs` | 広い画面（web版の601px以上）の横向きタイムライン：上に月と日の軸（今日に印）、祈願／跳躍・イベントの順の行ごとに開始から終了までのバーと、その上（短いバーでは空いている側）に名前・期間・残り時間。1日1列の固定レイアウトの表で `colspan` によって配置する（CSPのためインライン `style` を使わない）。表は `aria-hidden` で、読み上げは `event_list.rs` の一覧 |
| `dashboard-ui/src/pages/event_list.rs` | 縦向きタイムライン（600px以下とアプリ）：「現在」の印から開催中を終了の早い順に、開催予定を開始日ごとにカードで並べる。広い画面でも視覚的に隠して、読み上げ用の文の構造として残す |
| `dashboard-ui/src/pages/game_tabs.rs` | ダッシュボードとイベント画面が共有するゲームタブ（選択中のゲームは両画面で共通）、900px以下の判定、スワイプ時の切り替え |
| `dashboard-ui/src/pages/cap_history.rs` | ダッシュボードの主リソース（樹脂・開拓力）の下の「上限に達していた時間（直近7日間）」：合計時間、7日間のタイムライン（上限・回復中・不明、CSPに合わせてSVGの図形で描く）と凡例、スクリーンリーダー向けの内訳の文 |
| `dashboard-ui/src/pages/challenges.rs` | ダッシュボードの「高難度コンテンツ」欄：モードごとの到達状況とリセットまでの残り時間（共通の `Clock` で毎秒更新）、未挑戦・未開放・取得失敗の表示 |
| `src/dashboard/games/summary.rs` | 上流ゲーム状態から画面用の要約への写像 |

### バックエンドモジュール

| パス | 責務 |
| --- | --- |
| `src/dashboard/auth/` | ユーザーのライフサイクル、パスワード、セッション（一覧・個別／他端末の失効・最終利用）、APIトークン（`api_tokens.rs`。発行・失効・Bearer認証・スコープで通せるルート）、パスキー（名前変更・最終利用）（アカウントの有無を明かさないログインoptionsを含む）、削除要求と完了（再試行の待機を含む）、ユーザー名ごとのログイン試行制限、ユーザーごとの現在パスワード確認の試行制限 |
| `src/dashboard/auth/app_logins.rs` | ブラウザからアプリへのPKCE（S256）ワンタイムコード承認・交換 |
| `src/dashboard/http/app_login.rs` | アプリログイン承認・交換のHTTPペイロードとルート |
| `src/dashboard/http/` | ルート、DTO、Cookie・Bearer APIトークン、Origin・キャッシュの境界、UI配信、ヘルスチェック・メトリクス、報酬・削除のバックグラウンド処理の起動と保守 |
| `src/dashboard/db/` | `entities/` に保存モデル、`migrations/` にスキーマ変更 |
| `src/dashboard/hoyolab/` | Cookieの解析・暗号化保存・削除・鍵の再暗号化（`rekey`）、連携の検証・同期、ユーザー単位の操作ロック、管理者向けの連携の健全性と「要対応」の判定（`health.rs`、保存済みの行だけを読む） |
| `src/dashboard/games/` | ゲームUID一覧・表示設定・状態キャッシュ（`service.rs`）、高難度コンテンツの要約とそのキャッシュ（`challenges.rs`、上流JSONからの防御的な読み取りは `challenge_summary.rs`）、イベントカレンダーとそのキャッシュ（`calendar.rs`、読み取りは `calendar_summary.rs`）。両方の読み取りが使う型に依存しない補助関数は `upstream_json.rs`。樹脂・開拓力の履歴（`resource_history.rs`：状態取得の成功ごとのスナップショット、非表示・未検出UIDの行の削除、保持期間）と、上限に達していた時間を求める純粋関数（`cap_time.rs`） |
| `src/dashboard/notifications/` | ユーザー・管理者のWebhookの設定、条件評価（上限・派遣完了・デイリー・週間など）、送信、重複排除、Androidアプリの端末登録と受信箱（`app.rs`）、UnifiedPushの購読の保存と起床用プッシュの送信（`push.rs`、RFC 8291/8292） |
| `src/dashboard/rewards/` | 報酬設定・スケジュール・実行（定時と遅延実行の区別）、送信不確定時のガード、今月の報酬一覧のキャッシュ、ユーザー削除前の報酬確認 |
| `src/dashboard/settings_transfer/` | 本人の設定の書き出しと読み込み：`ahd-settings` 形式（版1）と項目ごとの検証（`document.rs`）、現在の設定との比較・プレビュー・1トランザクションでの反映（`service.rs`）。報酬の時刻変更は `rewards::service::reschedule_in`、通知の保存は `notifications::channel::PersonalPreferences` を共用 |
| `src/hoyolab/` | HoYoLAB上流APIの型付きクライアント（ダッシュボードの方針は含めない）。高難度コンテンツのGET（`challenge.rs`：モード・パス・クエリ）とイベントカレンダー（`HoyolabClient::event_calendar`）は未検証の形式のためJSONのまま返す |

## HoYoLAB連携

初回パスワード変更後、連携画面からHoYoLAB Cookieを登録し、検証を完了してください。Cookieは <https://www.hoyolab.com> にログインしたブラウザのApplication／Storageから取得します。

- 登録には `ltoken_v2`・`ltuid_v2`・`ltmid_v2` の3つが必須です。ブラウザの `Cookie:` ヘッダー行をそのまま貼り付けても構いません（他のCookieや空の要素は無視されます）。旧形式（`ltoken`／`ltuid`）は登録できません。
- 保存だけでは連携完了になりません。検証後にゲームアカウントを確認してください。
- **自動受け取りは初期OFF**。画面で時刻を指定して有効にします。時刻はサーバー時刻（`TIMEZONE_OFFSET`）で、ログインボーナスの日付はUTC+8の0時（日本時間1時）に切り替わります。
- 連携画面の「Cookieを削除」で保存済みCookieを消せます。UIDの紐付け・ゲーム表示・履歴は残り、自動受け取りはすべてOFFになります。再開するには同じHoYoLABアカウントのCookieを登録します。
- 状態取得にはHoYoLAB側の戦績・リアルタイムノートの公開設定も影響します。Cookie失効・追加認証・HoYoLABの仕様変更により利用できなくなる場合があります。

## 運用上の制約

- 永続データは `db-data` ボリュームに保存します。`POSTGRES_DB`・`POSTGRES_USER`・`POSTGRES_PASSWORD` は初回初期化用で、既存DBには `compose.yaml` の変更だけでは反映されません。
- **DBと対応する `HOYOLAB_COOKIE_KEY` をセットでバックアップ**してください。鍵を変更するときは、サーバーを止めてバックアップ後に `docker compose run --rm -e HOYOLAB_COOKIE_KEY_NEW=<新しい鍵> app rekey` で再暗号化し、`HOYOLAB_COOKIE_KEY` を新しい鍵に替えてから起動します。単に鍵を書き換えると保存済みCookieを復号できません。
- 更新はバックアップ後に `docker compose pull && docker compose up -d`。起動時のDB更新を伴うため、ロールバックにDB復元が必要になる場合があります。

## リリースイメージ

webのタグ `web-vX.Y.Z`（ルート `Cargo.toml` の版）のpushで `.forgejo/workflows/image.yml` がlinux/amd64イメージをビルドし、Forgejoのコンテナレジストリ（`<Forgejoホスト>/<owner>/<repo>`、小文字）へ公開します。webのタグではForgejoリリースを作らず、アプリのビルドも動きません。CIではテストを実行しないため、タグ付け前に `devenv test` を通してください。

アプリ（Android APK）は別の版・別のタグ `vX.Y.Z`（`dashboard-ui/Cargo.toml` の版）で `.forgejo/workflows/android.yml` から出します。v1.8.0 までの `v*` タグはイメージとAPKの両方を出していました。どちらを出すかの判断（`scripts/release-scope.sh`）・手順・テスト用タグは [docs/releases.md](docs/releases.md) と [docs/versioning.md](docs/versioning.md) を参照してください。

- `web-v1.2.3` → `1.2.3`・`1.2`・`latest`。`web-v1.2.3-rc.1` などはそのタグのみ
- runnerはラベル `ubuntu-latest`、ジョブから `docker`／`buildx` でDinDへ接続できること
- リポジトリSecret `REGISTRY_TOKEN`：タグをpushするユーザーの `write:package` 権限付きアクセストークン
- ビルドキャッシュは同じイメージの `buildcache` タグに保存
- ジョブごとに `ahd-<run id>-<attempt>` という名前のdocker-container builderを作り、終了時にstate volumeごと削除します。builderはhostのDocker上で動き、消さないとビルドごとに数GBのキャッシュが残ってrunnerのディスクを埋めるためです。中断されたジョブの残り（`buildx_buildkit_ahd-*`）は次のビルドの開始時に削除します（runnerは1ジョブずつ実行）。

## API概要

ベースパスは `/api/v1`。ブラウザは同一OriginのWebセッションを使い、外部ツールはユーザーごとのAPIトークンを使えます。OAuthは提供しません。詳細は [OpenAPI仕様](docs/dashboard-api.json)または稼働中の `/docs` を参照してください。

- セッションCookieはHTTPSで `__Host-attos_session`、localhost HTTPで `attos_session`。
- **Cookieで認証する状態変更リクエスト（ログインを含む）には、`PUBLIC_ORIGIN` と完全一致する `Origin` が必須**です。

### APIトークン

- 設定の「認証・セキュリティ」で発行・一覧・失効します（1ユーザー10個まで）。平文（`ahd_` ＋16進64文字）は発行時に1回だけ表示し、サーバーにはSHA-256だけを保存します。
- `Authorization: Bearer ahd_…` を付けて呼びます。Cookieを伴わないBearerの要求には `Origin` は不要です。トークンとセッションCookieを同時に送った要求は401です。
- スコープ `read` で使えるのは `GET /me`、`/server-settings`、`/me/hoyolab`、`/game-accounts`、`/game-accounts/{id}/status`、`/game-accounts/{id}/challenges`、`/game-accounts/{id}/calendar`、`/game-accounts/{id}/resource-history`、`/reward-settings`、`/reward-runs` です。`run` はこれに加えて `POST /reward-settings/{id}/run` を使えます。それ以外（トークン管理を含む）はセッション専用で、トークンでは403になります。
- 管理者によるパスワード再発行・無効化・削除要求で、そのユーザーのトークンはすべて失効します。本人のパスワード変更では失効しません。
- 初回／復旧後のパスワード変更前は操作が制限されます。ゲーム・報酬機能にはHoYoLAB連携検証の完了も必要です。

以下のパスは `/api/v1` からの相対パスです。

| 機能 | 主なエンドポイント |
| --- | --- |
| 認証 | `POST /auth/login`、`POST /auth/logout`、`GET /me`、`PUT /me/password` |
| パスキー | `GET /me/passkeys`、`POST /me/passkeys/options`・`/me/passkeys/verify`、`PATCH`・`DELETE /me/passkeys/{id}`、`POST /auth/passkey/options`・`/auth/passkey/verify` |
| アプリログイン | `POST /me/app-login`（セッション専用）、`POST /auth/app-login/exchange`（PKCE S256、120秒・1回限りのコードを新しいセッションに交換） |
| セッション | `GET /me/sessions`、`DELETE /me/sessions/{id}`、`POST /me/sessions/revoke-others` |
| APIトークン | `GET`・`POST /me/api-tokens`、`DELETE /me/api-tokens/{id}`（セッション専用） |
| HoYoLAB | `GET /me/hoyolab`、`PUT`・`DELETE /me/hoyolab/cookie`、`POST /me/hoyolab/verify`・`/me/hoyolab/sync` |
| ゲーム | `GET /game-accounts`、`PATCH /game-accounts/{id}`、`GET /game-accounts/{id}/status`、`GET /game-accounts/{id}/challenges`、`GET /game-accounts/{id}/calendar`、`GET /game-accounts/{id}/resource-history` |
| 報酬 | `GET /reward-settings`、`PATCH /reward-settings/{id}`、`POST /reward-settings/{id}/run`、`GET /reward-runs` |
| 設定の移行 | `GET /me/settings/export`、`POST /me/settings/import/preview`・`/me/settings/import`（セッション専用） |
| 通知 | `GET/PUT /me/notifications`、`POST /me/notifications/test`、`POST /me/notifications/app-devices`、`DELETE /me/notifications/app-devices/{id}`、`GET /me/notifications/app-devices/{id}/inbox`、`GET/PUT/DELETE /me/notifications/app-devices/{id}/push`、`GET/PUT /admin/notifications`、`POST /admin/notifications/test` |
| ユーザー管理 | `GET /admin/users`（連携状態・自動受け取り数・最終ログイン、`needs_attention` と `hoyolab`（本人確認日時・ゲーム状態の最終取得成功日時・ゲームごとの自動受け取りと直近に終わった実行の状態・日時・sanitized error code）を含む。HoYoLABは呼ばず、Cookie・UID・上流raw errorは含まない。セッション専用）、`POST /admin/users`、`PATCH /admin/users/{id}`（`active` または `is_admin`）、`PUT /admin/users/{id}/temporary-password`、`GET /admin/audit` |
| ユーザー削除 | `DELETE /me`、`DELETE /admin/users/{id}` |
| サーバー設定 | `GET /server-settings` |

- 設定の移行（設定の「設定の移行」タブ、web版だけ）は、ログインボーナスの自動受け取り（ON/OFF・時刻）、ゲームUIDの表示、通知の選択（Webhookとアプリの種類ごとの選択・形式・言語・樹脂の警告時間・リマインダー時刻）を `{"format":"ahd-settings","version":1,…}` のJSONにします。HoYoLAB Cookie・Webhook URL・パスワード・パスキー・セッション・APIトークンは含めず、Webhookは設定済みかどうか（`webhook_configured`）だけを書きます。読み込みは項目ごとに検証し（問題があれば項目のパスと種類を一覧にして何も反映しない）、プレビューで変わる設定を示してから1トランザクションで反映します。このアカウントにないUIDやゲームは追加せず、理由付きで飛ばします。報酬・UIDの反映にはHoYoLAB連携の検証済みが必要です。言語・テーマはブラウザの設定（サーバーに保存しない）なので対象外です。
- ゲーム種別は `genshin`／`starrail`。状態レスポンスの `stale` と取得日時を確認してください。
- `GET /game-accounts/{id}/challenges` は高難度コンテンツの現在の期間の要約です（原神：深境螺旋・幻想シアター・幽境の激戦、スターレイル：混沌の記憶・虚構叙事・末日の幻影・異相の仲裁）。モードごとに期間名・開始／終了日時・星・到達階層・幕・難易度・最速クリア秒数・戦闘回数を返し、キャラクターごとの戦績は返しません。上流のフィールド名は実応答で未検証のため、値はすべてnullになり得て、nullは「不明」を意味します（未開放や非公開とはみなしません）。一部のモードだけ失敗した場合はそのモードの `error_code` に理由が入ります。
- `GET /game-accounts/{id}/calendar` はイベントカレンダーです。原神はキャラクター・武器・集録の祈願とイベント、スターレイルはキャラクター・光円錐の跳躍とイベントを、終了の早い順に返します（取得時点で終了済みの項目と、高難度コンテンツ・模擬宇宙のような常設のモードは除きます）。項目ごとに名前・バージョン・開始／終了日時・ピックアップ（名前・レアリティ・公式画像CDNの画像URL）と、イベントの完了・未開放・進捗を返し、説明文や報酬一覧は返しません。`starts_at` が取得時刻より後の項目は開催予定です。上流のフィールド名は実応答で未検証のため、値はnullになり得て、nullは「不明」を意味します。原神の上流 `POST /act_calendar` は本文に `role_id`／`server` だけを送る読み取り専用の照会です。
- `GET /game-accounts/{id}/resource-history` は主リソース（原神は天然樹脂、スターレイルは開拓力）が直近7日間に上限に達していた時間です。HoYoLABは呼ばず、`/status` の取得（ダッシュボードと通知ワーカー）が成功するたびに保存した件数・上限・全回復時刻だけから計算します。スナップショットの間は回復速度（樹脂8分・開拓力6分で1）で何もしなかったと仮定して進め、次のスナップショットが90分以内でその見込みと1以内で一致したときだけ「上限」と数えます。見込みの上限到達より前は「回復中」（消費しても上限は遅れるだけ）、それより後で間隔が長いか消費が見えたときは「不明」、見込みより増えていた（アイテムでの回復や上限の変更）ときはその区間全体を「不明」にします。最後のスナップショットの後は90分までだけ見込みで進めてその先の上限は「不明」とし、窓の始まりから最初のスナップショットまでも「不明」です。`at_cap_seconds`・`unknown_seconds` と、窓全体を隙間なく覆う `segments`（`at_cap`／`below_cap`／`unknown`）を返します。非表示・未検出のUIDはスナップショットを持たないため窓全体が不明です。トークンは `read`／`run` のどちらでも読めます。
- 報酬操作の `{id}` は報酬設定のUUIDで、ゲームUIDではありません。検証・同期・手動受け取りのPOSTには `{}` を送ります。
- 画面の手動受け取りはダッシュボードの「ログインボーナスを今すぐ受け取る」1つにまとめ、本日分が未受け取りのゲームだけを実行します。APIは報酬設定ごとの非同期 `202` 受付です。結果は履歴で確認します。不確定な送信結果は再POSTせず、追加認証が必要ならHoYoLAB側で対応してください。
- `POST /auth/passkey/options` は `{"username":"..."}` でユーザー名指定、`{}` でユーザー名なしのdiscoverableログインを開始します。ユーザー名指定では存在しないユーザーやpasskey未登録のユーザーにも同じ形の応答を返し、ログインは `verify` で401になります（アカウントの有無を明かさないため）。ユーザー名なしの開始はアカウント検索・試行枠の消費をせず、verifyでuserHandleから特定したユーザーの枠を署名検証前に消費します。新規登録はresident keyをpreferredで要求します。
- 無効化したユーザーを再有効化しても旧セッションは復活せず、そのユーザーの自動受け取りはすべてOFF（次回予定も消去）になります。意図しない報酬POSTの再開を防ぐためで、本人が設定で再度有効にします。
- 削除も非同期で、受付時に利用停止・セッション失効となります。操作者の現在パスワードと対象ユーザー名の完全一致入力が必要です。最後の有効な管理者は削除できません。

## ライセンス

GNU General Public License v3.0 以降（`GPL-3.0-or-later`）。全文は [`LICENSE`](LICENSE) にあります。依存クレートとUIに同梱するフォントのライセンスは、アプリの「このアプリについて」画面と `Cargo.lock` で確認できます。
