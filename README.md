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
| `REWARD_MAX_ATTEMPTS` / `REWARD_RETRY_DELAY_SECONDS` | 一時的な失敗の試行回数（1〜4、既定 `4`）／再試行までの秒数（1〜600、既定 `10`） |

HoYoLAB Cookieは環境変数ではなく画面から登録します。

### 公開先

HTTPSリバースプロキシから `http://127.0.0.1:8080` へパスを保持して転送します。UI・APIは同一Originで、サブパス設置は非対応です。

ローカル利用時は `PUBLIC_ORIGIN: http://localhost:8080`、`RP_ID: localhost` に変更します。IPリテラルやlocalhost以外のHTTPは使用できません。

| パス | 内容 |
| --- | --- |
| `/` | セッション確認後、未ログインなら `/login`、ログイン済みなら `/dashboard` |
| `/login` / `/dashboard` | ログイン／ダッシュボード。初期設定未完了時は設定ゲートを表示 |
| `/settings` | `/settings/games` へ308転送 |
| `/settings/games` / `/settings/bonus` / `/settings/link` / `/settings/security` / `/settings/notifications` / `/settings/delete` | 各設定画面（認証必須） |
| `/admin` | 管理者画面（管理者権限必須） |
| `/docs` / `/openapi.json` | API仕様の閲覧／OpenAPI JSON |
| `/manifest.webmanifest` | ホーム画面追加用のWebマニフェスト |
| `/health` | DB疎通と報酬ワーカーの生存確認。正常時 `{"status":"ok","worker":"ok"}`、ワーカー停止時は503 |
| `/metrics` | `METRICS_TOKEN` 設定時のみ。Prometheus形式の件数・ワーカー状態・直近24時間に終わった報酬実行の結果・APIトークン数 |

## 開発とUI設計

ビルド・検証は `devenv shell -- …` 経由で実行します。UI回帰は `devenv shell -- test-ui`。

Impeccableは `.pi/skills/impeccable/` にGit管理したプロジェクト専用スキルです。ルートの [PRODUCT.md](PRODUCT.md) と [DESIGN.md](DESIGN.md) が本実装のコンテキストです。

```sh
.pi/skills/impeccable/scripts/impeccable context --target dashboard-ui/src
# ランチャーの隔離検証（外部通信なし）
devenv shell -- node .pi/skills/impeccable/scripts/launcher.test.mjs
```

同梱のNoto Sans JPは表示文の文字だけを含むsubsetです。`dashboard-ui/locales/` の文言に新しい文字を加えると、UI unit testが未収録の文字を示して失敗します。その場合は `scripts/subset-fonts.sh`（取得元URL・SHA-256・実行方法を記載）で再生成し、出力されたファイル名を参照箇所へ反映します。

エンジンは初回に公開releaseからSHA-256を確認して `.impeccable/cache/` へ取得します（未追跡）。curl／wgetがない場合は既存のNodeを使用します。ユーザー共通設定・スキルは変更しません。Pi向け自動detector hookはないため、UI変更の最後にスキルの指示に従い手動検査します。

### Androidアプリ

`dashboard-ui` はDioxusのmobile機能でAndroidアプリとしてもビルドできます（Cargo feature `mobile`。web版は既定の `web` featureのまま）。画面構成はweb版とほぼ同じですが、次の機能はアプリに含めず、web版でのみ使えます。

- 管理者機能（ユーザー管理・管理者向け通知設定）、APIドキュメント（`/docs`）、ユーザー削除
- 認証・セキュリティのAPIトークンとパスワード変更（初回／復旧後のパスワード変更もブラウザで行い、アプリでは「設定状況を更新」を選びます）
- WebAuthnとWeb Pushはブラウザ専用のため、パスキーでのログイン・登録とブラウザ通知もアプリにはありません（登録済みパスキーの一覧・名前変更・削除は可能）

アプリのメニューの「ブラウザで開く」は、表示中の画面に対応するweb版のページ（`AHD_SERVER_URL` のオリジン＋同じパス）を端末の既定のブラウザで開きます（Androidの `Intent.ACTION_VIEW`、`webbrowser` クレート経由）。初回設定のパスワード変更画面と「セキュリティ」設定にも同じボタンがあります。ブラウザを開けなかったときはエラーを表示します。web版にはこのボタンはありません。

アプリはビルド時に指定した1つのサーバーへRust（reqwest、rustls）から直接接続します。ログイン応答のセッションCookieをアプリ専用領域（`getFilesDir()`、他アプリから読めない）に保存し、以後の要求に `Cookie` と、サーバーが要求する `Origin`（＝サーバーのOrigin）を付けます。ログアウトや期限切れで削除されます。サーバー側の変更はありません。

#### 通知

「通知」ページで、この端末の樹脂・開拓力と洞天宝銭の上限到達、派遣完了、参量物質変化器の通知を設定できます。サーバー送信ではなく、取得した状態から予測した時刻にAndroidのローカルアラームで通知します。アラームは不正確でDozeにより遅れる場合があり、予測の更新はアプリが状態を読み込んだときだけです。強制停止すると次にアプリを起動するまでアラームは解除されます。`dx serve` のAPKにはreceiverが入らないため、通知には `build-android` のAPKを使います。

必要なもの：devenvの `android` プロファイル（Android SDK 34・build-tools 34.0.0・NDK・JDK 17。初回は数GBを取得し、Android SDKのライセンスに同意したものとして扱います）と、Gradleが依存を取得するためのネットワーク。Rustの `aarch64-linux-android`／`x86_64-linux-android` targetは `rust-toolchain.toml` で入ります。

```sh
# 実機（arm64）向けAPK。AHD_SERVER_URL はHTTPSのOrigin（パスなし）
AHD_SERVER_URL=https://ahd.example.com devenv --profile android shell -- build-android
# x86_64エミュレータ向け
AHD_SERVER_URL=https://ahd.example.com AHD_ANDROID_TARGET=x86_64-linux-android devenv --profile android shell -- build-android
# 静的検査（clippy、mobile feature、aarch64-linux-android）
devenv --profile android shell -- check-android
```

APKは `target/dx/attos-dashboard-ui/debug/android/app/app/build/outputs/apk/debug/app-debug.apk` に出力されます（デバッグ署名。`--release` を付けても署名設定がなければデバッグ署名です）。`adb install -r <APK>` で導入します。`build-android` は `dx build --platform android` の後、`dashboard-ui/android/res/` のアイコン・アプリ名「AHD」・テーマで生成プロジェクトを上書きし、AndroidManifestの `configChanges` を広げ、バージョンから `versionCode` を設定してGradleを再実行します（`dx serve --platform android` ではDioxus既定のアイコン・名前になり、ダークモードや文字サイズの変更でアプリが終了します）。

`v*` タグをpushすると `.forgejo/workflows/android.yml` がarm64のAPKを `--release` でビルドし、そのタグのForgejoリリースに `ahd-android-X.Y.Z-arm64.apk` と `.sha256` を添付します。接続先はリポジトリ変数 `AHD_SERVER_URL`（未設定なら `https://hoyo.attodao.cc`）です。リポジトリのsecret `ANDROID_KEYSTORE_BASE64`（base64のkeystore）・`ANDROID_KEYSTORE_PASSWORD`・`ANDROID_KEY_ALIAS` があれば同じ鍵で署名し直して更新インストールできます。なければビルドごとのデバッグ鍵になり、上書き更新できません。`versionCode` は `X*10000+Y*100+Z`（Y・Zは100未満）を `build-android` が設定します。

- `AHD_SERVER_URL` はHTTPS、またはlocalhostのHTTPのみ。未指定時は `http://localhost:8080` で、開発用サーバー（`PUBLIC_ORIGIN: http://localhost:8080`）に `adb reverse tcp:8080 tcp:8080` でつなぐ想定です。
- エミュレータは `devenv --profile android --profile android-emulator shell`（x86_64イメージ、KVMが必要）で入ります。
- パッケージ名 `cc.attodao.ahd`、minSdk 24、targetSdk 34（`dashboard-ui/Dioxus.toml`）。権限はINTERNET（dx既定）、通知用の `POST_NOTIFICATIONS` と `RECEIVE_BOOT_COMPLETED` です。
- tao/wryはActivityの破棄・再生成や二つ目の起動に対応しておらず、その場合プロセスが異常終了します。そのためActivityは1つだけ（`singleTask`）で、最初の画面での戻る操作はアプリを閉じずにバックグラウンドへ移し（ホームと同じ）、ダークモード・文字サイズ・表示サイズ・言語・キーボードなどの変更はActivityを作り直さずにその場で反映します。最近使ったアプリから消すなどで破棄されたときはプロセスをそのまま終了し、次回は新しく起動します（`dashboard-ui/android/MainActivity.kt`）。テーマ「システム」は端末のダークモードに追従します。

web版からの変更点（除外した機能とその方法、画面ごとのレイアウト、追加機能の仕様、共通化と分岐、web版への影響、既知の制約）は [docs/android-changes.md](docs/android-changes.md) にまとめています。

### UIモジュール

| パス | 責務 |
| --- | --- |
| `dashboard-ui/src/app/navigation.rs` | 通常パス・History API・認証状態に応じた転送と戻る／進むの対応 |
| `dashboard-ui/src/app/state.rs` | 画面状態、認証・初期設定ゲートと画面データ取得の起動 |
| `dashboard-ui/src/app/session.rs` | `/me`による起動・再確認、初期設定ゲートのその場での再読み込み、ログアウト |
| `dashboard-ui/src/app/actions.rs` | 操作の予約（busy）と完了・APIエラー処理。セッション世代とは独立 |
| `dashboard-ui/src/app/data.rs` | セッション世代に結び付いた画面データの取得（未取得と空を区別、リソースごとに最新の要求の応答だけを反映）、全ゲームの手動受け取りとその後の報酬状態のGET追従 |
| `dashboard-ui/src/app/display.rs` | 数値・ブラウザのローカル時刻による簡潔な月日／24時間表示、データの経過時間、上限への近さ、サーバー時刻とブラウザ時刻の換算、エラーコードの表示文への対応付け |
| `dashboard-ui/src/pages/` | 画面、メニュー、設定、ユーザー管理 |
| `dashboard-ui/src/platform.rs` | web版とAndroidアプリの切り替え（`APP`）、両方で動くタイマーと時刻、web版を既定のブラウザで開く処理 |
| `dashboard-ui/src/mobile/` | Androidアプリ専用：サーバーへの直接接続とセッションCookieの保存（`http.rs`・`session.rs`）、アプリ専用領域のファイル（`storage.rs`）、端末時刻（`time.rs`）、通知予測とJNIブリッジ（`reminders.rs`・`notify.rs`）、web版がweb-sysで行うDOM操作の `document::eval` 版（`dom.rs`） |
| `dashboard-ui/src/app/device_notifications.rs` | 端末通知の設定・予定の保存、状態変更時の再計算、通知からの画面遷移 |
| `dashboard-ui/src/pages/device_notifications.rs` | アプリの端末通知設定カード |
| `dashboard-ui/android/res/` | ランチャーと通知のアイコン、アプリ名、DayNightテーマ（`build-android` が生成プロジェクトへ上書き） |
| `dashboard-ui/android/MainActivity.kt` | AndroidのActivity（`Dioxus.toml` の `android_main_activity`）：ルートでの戻るをバックグラウンド化、設定変更時の文字サイズ反映、破棄時のプロセス終了、通知の権限要求・アラーム・receiver（receiver設定は `android/AndroidManifest.application.xml` を生成manifestへ挿入） |
| `src/dashboard/games/summary.rs` | 上流ゲーム状態から画面用の要約への写像 |

### バックエンドモジュール

| パス | 責務 |
| --- | --- |
| `src/dashboard/auth/` | ユーザーのライフサイクル、パスワード、セッション（一覧・個別／他端末の失効・最終利用）、APIトークン（`api_tokens.rs`。発行・失効・Bearer認証・スコープで通せるルート）、パスキー（名前変更・最終利用）（アカウントの有無を明かさないログインoptionsを含む）、削除要求と完了（再試行の待機を含む）、ユーザー名ごとのログイン試行制限、ユーザーごとの現在パスワード確認の試行制限 |
| `src/dashboard/http/` | ルート、DTO、Cookie・Bearer APIトークン、Origin・キャッシュの境界、UI配信、ヘルスチェック・メトリクス、報酬・削除のバックグラウンド処理の起動と保守 |
| `src/dashboard/db/` | `entities/` に保存モデル、`migrations/` にスキーマ変更 |
| `src/dashboard/hoyolab/` | Cookieの解析・暗号化保存・削除・鍵の再暗号化（`rekey`）、連携の検証・同期、ユーザー単位の操作ロック |
| `src/dashboard/games/` | ゲームUID一覧・表示設定・状態キャッシュ |
| `src/dashboard/notifications/` | ユーザー・管理者のWebhook／ブラウザ通知の設定、条件評価（上限・派遣完了・デイリー・週間など）、送信、重複排除 |
| `src/dashboard/rewards/` | 報酬設定・スケジュール・実行（定時と遅延実行の区別）、送信不確定時のガード、今月の報酬一覧のキャッシュ、ユーザー削除前の報酬確認 |
| `src/hoyolab/` | HoYoLAB上流APIの型付きクライアント（ダッシュボードの方針は含めない） |

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

`v*` タグのpushで `.forgejo/workflows/image.yml` がlinux/amd64イメージをビルドし、Forgejoのコンテナレジストリ（`<Forgejoホスト>/<owner>/<repo>`、小文字）へ公開します。CIではテストを実行しないため、タグ付け前に `devenv test` を通してください。

同じタグで `.forgejo/workflows/android.yml` も起動し、Android APKとチェックサムをForgejoリリースに添付します。

- `v1.2.3` → `1.2.3`・`1.2`・`latest`。`v1.2.3-rc.1` などはそのタグのみ
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
- スコープ `read` で使えるのは `GET /me`、`/server-settings`、`/me/hoyolab`、`/game-accounts`、`/game-accounts/{id}/status`、`/reward-settings`、`/reward-runs` です。`run` はこれに加えて `POST /reward-settings/{id}/run` を使えます。それ以外（トークン管理を含む）はセッション専用で、トークンでは403になります。
- 管理者によるパスワード再発行・無効化・削除要求で、そのユーザーのトークンはすべて失効します。本人のパスワード変更では失効しません。
- 初回／復旧後のパスワード変更前は操作が制限されます。ゲーム・報酬機能にはHoYoLAB連携検証の完了も必要です。

以下のパスは `/api/v1` からの相対パスです。

| 機能 | 主なエンドポイント |
| --- | --- |
| 認証 | `POST /auth/login`、`POST /auth/logout`、`GET /me`、`PUT /me/password` |
| パスキー | `GET /me/passkeys`、`POST /me/passkeys/options`・`/me/passkeys/verify`、`PATCH`・`DELETE /me/passkeys/{id}`、`POST /auth/passkey/options`・`/auth/passkey/verify` |
| セッション | `GET /me/sessions`、`DELETE /me/sessions/{id}`、`POST /me/sessions/revoke-others` |
| APIトークン | `GET`・`POST /me/api-tokens`、`DELETE /me/api-tokens/{id}`（セッション専用） |
| HoYoLAB | `GET /me/hoyolab`、`PUT`・`DELETE /me/hoyolab/cookie`、`POST /me/hoyolab/verify`・`/me/hoyolab/sync` |
| ゲーム | `GET /game-accounts`、`PATCH /game-accounts/{id}`、`GET /game-accounts/{id}/status` |
| 報酬 | `GET /reward-settings`、`PATCH /reward-settings/{id}`、`POST /reward-settings/{id}/run`、`GET /reward-runs` |
| 通知 | `GET/PUT /me/notifications`、`POST /me/notifications/test`、`POST/DELETE /me/notifications/push`、`GET/PUT /admin/notifications`、`POST /admin/notifications/test` |
| ユーザー管理 | `GET /admin/users`（連携状態・自動受け取り数・最終ログインを含む）、`POST /admin/users`、`PATCH /admin/users/{id}`（`active` または `is_admin`）、`PUT /admin/users/{id}/temporary-password`、`GET /admin/audit` |
| ユーザー削除 | `DELETE /me`、`DELETE /admin/users/{id}` |
| サーバー設定 | `GET /server-settings` |

- ゲーム種別は `genshin`／`starrail`。状態レスポンスの `stale` と取得日時を確認してください。
- 報酬操作の `{id}` は報酬設定のUUIDで、ゲームUIDではありません。検証・同期・手動受け取りのPOSTには `{}` を送ります。
- 画面の手動受け取りはダッシュボードの「ログインボーナスを今すぐ受け取る」1つにまとめ、本日分が未受け取りのゲームだけを実行します。APIは報酬設定ごとの非同期 `202` 受付です。結果は履歴で確認します。不確定な送信結果は再POSTせず、追加認証が必要ならHoYoLAB側で対応してください。
- `POST /auth/passkey/options` は存在しないユーザーやpasskey未登録のユーザーにも同じ形の応答を返し、ログインは `verify` で401になります（アカウントの有無を明かさないため）。
- 無効化したユーザーを再有効化しても旧セッションは復活せず、そのユーザーの自動受け取りはすべてOFF（次回予定も消去）になります。意図しない報酬POSTの再開を防ぐためで、本人が設定で再度有効にします。
- 削除も非同期で、受付時に利用停止・セッション失効となります。操作者の現在パスワードと対象ユーザー名の完全一致入力が必要です。最後の有効な管理者は削除できません。
