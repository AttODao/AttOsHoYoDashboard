# AttOsGameAPI

原神・崩壊：スターレイルのHoYoLAB APIを呼ぶ型付きRustクライアントと、身内用Dashboard WebセッションAPIです。DashboardはSeaORM＋PostgreSQL、パスワード／パスキー認証、Dioxus Web/WASM RSX UI、HoYoLAB Cookie保存、ゲーム状態cache、報酬worker、ユーザー削除workflowを持ちます。

HoYoLAB連携と手動・定期の報酬受け取りは、ユーザーが実アクセス・受け取りと趣味用途での利用を承認した通常機能です。`HOYOLAB_COOKIE_KEY`による暗号化を設定し、既存の連携検証に成功すると利用できます。自動受け取りは初期OFFで、ユーザーが既存UIから有効にします。この利用承認は、実上流のtoken-to-UID bindingやreward POST契約全体の実証を意味しません。

Dashboardの仕様・設計・計画・OpenAPI・運用手順は[ドキュメント一覧](docs/README.md)を参照してください。

## 型付きAPIクライアント

```rust,no_run
use attosgameapi::{
    game::{Genshin, Starrail},
    hoyolab::{HoyolabClient, RecordAccount},
};

# async fn example() -> Result<(), String> {
let cookie = std::env::var("HOYOLAB_COOKIE").map_err(|_| "Cookieが未設定です")?;
let client = HoyolabClient::new(&cookie)?;
let account = RecordAccount {
    uid: "123456789".into(),
    server: "os_asia".into(),
};
let index = client.index(Genshin, &account).await?;
let note = client.daily_note(Genshin, &account).await?;
let info = client.daily_info(Starrail).await?;
let home = client.daily_home(Starrail).await?;
# Ok(())
# }
```

`index`と`daily_note`には、対象ゲームのUIDとserverが必要です。serverはアカウント一覧から取得した値を渡してください。`daily_info`、`daily_home`、`daily_sign`はCookieのアカウントを対象とします。戻り値は検証済みエンベロープの内側の型付き`data`です。

`daily_sign(Genshin)`／`daily_sign(Starrail)`だけは実際に報酬を受け取るPOSTです。自動再試行はしません。成功retcodeでも追加認証が必要な場合があるため、戻り値の`requires_verification()`と認証情報を確認してください。`None`は受取成功を意味せず、受取の確定には`daily_info`の`is_sign`を再確認します。通常のテストはローカルのモックAPIを使用し、明示実行のlive GETテストも受け取りPOSTは行いません。

クライアントのエラーは`hoyolab::Error`です。通信・HTTP・retcode・レスポンス形式を型で区別でき、文字列診断には`to_string()`を使います。上流の`message`は秘密情報を含み得るため保持・表示しません。`Result<T, String>`境界の`?`変換は提供しますが、String固有の操作は明示的な変換が必要です。

`GameKind`は`Genshin(Genshin)`／`Starrail(Starrail)`を持ち、JSONでは`"genshin"`／`"starrail"`として読み書きします。実行時にゲームを選ぶ場合は、このenumを`match`して型付きメソッドを呼びます。新しいゲームはstruct、`Game`実装、対応モデル、`GameKind`への追加で対応できます。Dashboardのゲーム状態APIは、この型付きクライアントを所有者scopeの業務処理から呼びます。

## 責務別の配置

- `src/hoyolab/mod.rs`: HoYoLABクライアントの公開窓口。
- `src/hoyolab/client/mod.rs`: Cookie認証、DS署名、HTTP通信、エンベロープ検証。
- `src/hoyolab/game.rs`: ゲームのマーカー型、`Game` trait、エンドポイント定数。
- `src/hoyolab/models/`: 共通・ゲーム別の型付きレスポンスモデル。
- `src/lib.rs`: ライブラリの入口。既存の`game`・`models`はルートからも再exportします。
- `src/main.rs`: 環境設定の読み込みと実行入口。
- `src/dashboard/`: 非GUIの業務・HTTP・永続化を次の責務で分離します。
- `src/hoyolab/client/{endpoint_tests,protocol_tests,validation_tests,live_tests}.rs`: 現行エンドポイントのモック・プロトコル・入力検証と明示実行の実APIテスト。
- `tests/fixtures/hoyolab/*.json`: モック検証用の合成fixture（認証済み実データではありません）。
- ルートの`.env`・`.env.example`: ローカル認証設定と秘密情報を含まない設定例。

### Dashboardのモジュール地図

```text
src/dashboard/
├── mod.rs                      # モジュール宣言・実行入口
├── cli.rs                      # serve / migrate / bootstrap-admin
├── config.rs / error.rs        # 設定検証・秘匿された業務エラー
├── auth/
│   ├── accounts.rs             # 作成・復旧・無効化・利用可否・監査
│   ├── deletion.rs             # 削除受付・last active admin保護・完了workflow
│   ├── password.rs             # Argon2idと有界CPU処理
│   ├── sessions.rs             # 発行・固定期限・認証・失効
│   └── passkeys/               # WebAuthn登録・認証・challenge・credential codec
├── hoyolab/
│   ├── credentials.rs          # Credentials / CookieKeyの窓口
│   ├── credentials/
│   │   ├── parser.rs           # legacy/v2 Cookie解析・正規化
│   │   └── crypto.rs           # v1 AES-256-GCM保存・owner/UID AAD
│   ├── store.rs                # owner lock・running gate・status/submit
│   ├── verification.rs         # identity候補・roles sync・same-UID update
│   ├── operations.rs           # owner単位の非global順序化
│   └── gateway.rs              # Dashboard用のtyped upstream boundary
├── games/
│   ├── service.rs              # UID visibility・status cache
│   └── summary.rs              # curated daily-note projection
├── rewards/
│   ├── service.rs              # settings・manual enqueue・history・共通認可再検査
│   ├── service/
│   │   ├── cache.rs            # status cache更新・TTL・payload保持
│   │   ├── test_support.rs     # service/worker共通の合成gateway・fixture
│   │   └── worker/
│   │       ├── mod.rs          # serviceの子workerを構成
│   │       ├── scheduler.rs    # tick・due schedule・history cleanup
│   │       ├── attempt.rs      # prepare・POST・GET確認
│   │       ├── state.rs        # durable送信状態・guard・完了transaction
│   │       └── deletion.rs     # 削除時の未確定報酬GET確認
│   └── schedule.rs             # fixed-offset next tick
├── http/
│   ├── mod.rs                  # route合成・HTTPサーバー生存期間
│   ├── auth.rs / users.rs / passkeys.rs
│   ├── hoyolab.rs / games.rs / rewards.rs
│   ├── middleware.rs           # Origin境界・session抽出
│   ├── session.rs              # Cookie・login response
│   ├── ui.rs                   # compiled Dioxus UI / docs / OpenAPI配信
│   └── response.rs             # safe error DTO
└── db/
    ├── mod.rs                  # 接続・migration実行
    ├── entities/               # 保存モデル
    └── migrations/             # スキーマ変更
```

HTTPは輸送と入力型、`auth/`は認証・ユーザーライフサイクル、`hoyolab/`はCookie保存と連携検証、`games/`と`rewards/`は所有者scopeの業務処理、`db/`は保存モデルとスキーマを担当します。認可や復旧ルールをハンドラーに複製せず、DBトランザクション・userロック・owner operation lockは業務側で一か所に維持します。変更単位ごとの検証・コミット方針は`AGENTS.md`に記載しています。

### Dioxus UIのモジュール地図

承認済みAGD R3の本番RSX・検証と限界は[本番R3記録](docs/ui-design/production-r3.md)を参照。

- `dashboard-ui/src/app.rs`: App合成・起動・60秒周期のDashboard更新。
- `dashboard-ui/src/app/state.rs`: AppState、Page/Phase、generation・busy・owner切替・画面アクセス制御と関連テスト。
- `dashboard-ui/src/app/data.rs`: 画面別のAPIデータ取得、ゲームごとのUID状態更新、ボーナス設定保存の共有処理。
- `dashboard-ui/src/app/preferences.rs`: 日本語／英語・dark／light。localStorageには非機密の選択だけを保存。
- `dashboard-ui/src/app/credentials.rs`: 管理者用仮パスワードのWeb Crypto生成・clipboard境界。生成結果は成功時だけメモリ内表示。
- `dashboard-ui/src/app/session.rs`: bootstrap、fresh me適用、signed-out・logout失敗時の状態。
- `dashboard-ui/src/app/actions.rs`: 非同期操作のsingle-flightと完了・エラー制御。
- `dashboard-ui/src/app/display.rs`: nullableな数値ペアの表示整形。
- `dashboard-ui/src/pages/mod.rs`: モジュール宣言のみ。`auth`、`shell`、`dashboard`、`hoyolab`、`accounts`、`rewards`、`security`、`admin`、`deletion`、`docs`に画面責務を分け、`common`に共通部品を置く。
- `dashboard-ui/src/api.rs` / `models.rs` / `webauthn.rs`: 共通HTTP・DTO・ブラウザ認証器境界（維持）。

## ビルド・テスト

### devenv（推奨）

プロジェクトrootで実行します。環境は`devenv.nix`／`devenv.yaml`に定義し、`devenv.lock`をcommitしてツールの組を固定します。

```console
devenv shell
devenv test
```

`devenv shell`にはRust（`rust-toolchain.toml`のversion・rustfmt・Clippy・WASM target）、Dioxus CLI・wasm-bindgen CLI・Binaryen、PostgreSQLのテスト用ツール、OpenSSL／pkg-config、Node.js、対応するPlaywright Core／Chromiumが入ります。ブラウザはNixの実行環境を使い、`/tmp`のツール配置や個人のnpm／ブラウザcacheには依存しません。

Dioxusは0.7.10を使用し、wasm-bindgen CLIはCargo.lockの要求（0.2.129）に合わせています。devenv標準のnixpkgsに同梱される版では古いため、このツールだけ`wasm-tools` inputから供給し、そのrevisionもlockしています。Cargo.lockでwasm-bindgenを更新する際はCLIの一致も確認してください。

`devenv test`はRustの整形・Clippy（native／WASM）、workspace全targetテスト、UI release build、合成APIでのブラウザ回帰（通常／reduced-motion）を実行します。個別実行は以下です。

```console
devenv shell -- check-rust
devenv shell -- test-rust
devenv shell -- build-ui
devenv shell -- test-ui
```

通常のshell／testでは`.env`を自動読込せず、共用PostgreSQL serviceも起動しません。`test-rust`は`DATABASE_URL`を除外し、テスト自身が専用clusterを作成・破棄します。実HoYoLAB GETテストは既定のignoreを維持し、実行には別途承認が必要です。Nix定義やlockへ秘密値を入れないでください。アプリケーションを手動起動する際の設定は後述の運用手順に従います。

### プロジェクト専用skills

`nix/project-skills.nix`が配布元（`devenv.yaml`の`skill-rust`・`skill-ui`・`skill-impeccable`・`impeccable-cli`）を`devenv.lock`のrevisionで固定します。UI系7件は配布元の`uipro init --ai codex`と同じ公式生成処理をNix store内で実行してCodex版を用意します。Rustは共通版、ImpeccableはPi版を使い、Claude用ディレクトリは参照しません。`devenv shell`で生成する`.pi/settings.json`の`skills`に、それぞれのNix storeパスを指定します。skill本文をリポジトリへ複製せず、プロジェクト内の`.agents/skills/`・`.pi/skills/`リンクも生成しません。ユーザー全体の設定・skillsは変更せず、Pi標準の設定マージにより個人のskillsも維持します。Piは`devenv shell`内で起動してください。設定の再読込は`/reload`で行えますが、環境外で起動したPiへ新しい環境変数は引き継がれません。

ImpeccableのネイティブengineもNixで用意し、`IMPECCABLE_BIN`で配布元のlauncherへ渡します。Pythonもdevenvで用意します。生成されたPi設定は直接編集せず、プロジェクト設定は`nix/project-skills.nix`へ統合してください。

既存ローカル版の退避先`.devenv/state/skills-before-devenv/`と`.impeccable/`のプロジェクト状態は維持します。

### SSH越しのテスト用サーバー

**devcon側**のプロジェクトrootで実行します。

```console
devenv shell -- test-server
```

UIとAPIをbuildし、devenv標準のプロセス管理でテスト専用PostgreSQLとHTTPサーバーを起動します。HTTP待受開始後に表示される次のコマンドを**ローカルPC**で実行し、`http://localhost:8080`を開きます。

```console
ssh -N -T -o ExitOnForwardFailure=yes -L 127.0.0.1:8080:127.0.0.1:8080 devcon
```

devcon側の起動コマンドを**Ctrl+C**で終了すると、HTTPとPostgreSQLの両方を停止します。ローカル側のCtrl+CはSSH転送だけを停止します。テストDBは`.devenv/`以下に保持されます。初回ログインは公開テスト用の`test-admin`／`public-test-only-password`（変更必須）。変更後はそのパスワードを使います。

`test-server` profileのDBは専用Unix socketだけ、HTTPは`127.0.0.1:8080`だけで待ち受けます。実DBの接続先・Cookie鍵は引き継がず、空の`.env`を置いた専用作業directoryから起動します。HoYoLAB連携・報酬受け取りは利用不可で、初期設定ゲートも迂回しません。

### 手動セットアップ（devenvを使わない場合）

Rustのstableツールチェーン（edition 2024対応）を使用します。WebAuthnのビルドには`pkg-config`とOpenSSLの開発ヘッダー／ライブラリが必要です。PostgreSQLは実行時に別途用意します。

```console
cargo build --locked --bins
cargo test --locked --all-targets
```

ルートworkspaceは `default-members = ["."]` のため、通常の `cargo check --locked` はバックエンド中心です。Dioxus UI crateも含める確認では、wasm targetを追加してworkspace指定で実行します。

```console
rustup target add wasm32-unknown-unknown
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo check --locked -p attos-dashboard-ui --target wasm32-unknown-unknown
```

Dashboardテストは`initdb`・`pg_ctl`を使い、テストごとに専用PostgreSQLクラスタを作成・破棄します。**非rootユーザー**で実行してください。実環境の`DATABASE_URL`やCookieを使用せず、ツール不足時は明示的に失敗します。ツールをPATHに置けない場合：

```console
ATTOS_TEST_PG_BIN=/path/to/postgresql/bin cargo test --locked --all-targets
```

テストは設定・CLI・DB制約・アカウント・パスワード・セッション・パスキー・HTTP・HoYoLABプロトコルごとに所有モジュールへ配置しています。アカウントのpassword rotationは`auth/accounts_tests/password_rotation.rs`、削除完了・queueは`auth/deletion_tests/completion.rs`へ分離し、元の親テストにpolicy/requestシナリオと小さな共通fixtureを残します。reward cacheテストは`rewards/service/cache_tests.rs`、reward実行シナリオは`rewards/service/worker/execution_tests.rs`・`tests.rs`に配置し、共通MockGateway/fixtureは`rewards/service/test_support.rs`から参照します。例：`cargo test --locked --bin attosgameapi dashboard::auth::sessions`。APIテストは現在実装済みのエンドポイントだけを対象とし、HoYoLABモックは実際のHTTPSホスト名・パスをループバックへ解決します。実APIテスト1件は外部通信の明示承認を要するため通常実行では`ignored`です。DBテストのツール不足や実APIテストの明示実行時の設定不足は失敗させます。合成UID・ソフトウェア鍵・モックは実APIの本人UID拘束・実認証器の確認の代わりではありません。

バックエンドの起動手順は次節を参照してください。Dashboard API契約は手書きのOpenAPI 3.1として管理します。

詳細なリクエスト仕様は [docs/hoyolab-api.md](docs/hoyolab-api.md) を参照してください。

ゲームレコードAPIのエンドポイント一覧とレスポンス構造は [docs/hoyolab-game-record/README.md](docs/hoyolab-game-record/README.md) を参照してください。

## Dashboardバックエンド / UI / API

DashboardはDioxus 0.7.10 Web/WASM RSX UIとWebセッションAPIを同じRustプロセスで提供します。UIはworkspace crate `dashboard-ui` / package `attos-dashboard-ui` で作成し、公式Dioxus Components由来のButton/Card/Card parts（DioxusLabs/components `9a758255ea26e2b20c8cecf4c1c946feb9e71da7`, MIT）をsource templateとして同梱します。SSR/fullstack server、native app、CDN、PAT/Scope、外部クライアント向け認証はありません。

このWeb/REST構成はDioxusで有効な選択ですが、同一RustのUI専用backendではFullstack server functionsも型・transport重複を減らす選択です。現構成の理由、移行時にも必要な安全境界、既存ライブラリによる簡素化は [設計 §1.1–1.2](docs/design/AttOsGameDashboard.md#11-dioxusとapiui境界) を参照してください。

公開route:

| Method | Path | 用途 |
|---|---|---|
| GET | `/health` | DB疎通確認 |
| GET | `/` | compiled Dioxus Dashboard UI index |
| GET | `/docs` | 同じDioxus index内のread-only OpenAPI viewer（try-itなし） |
| GET | `/openapi.json` | OpenAPI 3.1 JSON contract |

`/assets/{...}` はDioxus buildが生成したhash付き内部assetだけを `DASHBOARD_UI_DIR/assets` から配信します。`/assets/app.js` や `/assets/style.css` を安定APIとして文書化しません。

Web APIの正本は [docs/dashboard-api.json](docs/dashboard-api.json)、運用手順は [docs/dashboard-operations.md](docs/dashboard-operations.md) です。現在のAPIは `/api/v1/auth/*`、`/api/v1/me*`、passkeys、admin users、HoYoLAB status/cookie/verify/sync、game accounts/status、reward settings/runs です。架空のHoYoLAB proxy routeはありません。

### 設定と起動

PostgreSQLの専用DBを用意し、秘密値はprocess environmentまたはprivate secret storeで渡します。実値を`.env.example`や文書例へ書かないでください。

| 設定 | 初期値・用途 |
|---|---|
| `DATABASE_URL` | 必須。PostgreSQL URL |
| `BIND_ADDR` | `127.0.0.1:8080` |
| `PUBLIC_ORIGIN` | `http://localhost:8080`。公開時はHTTPS DNS origin。mutationの`Origin`完全一致とWebAuthn originに使用 |
| `RP_ID` | 既定は`PUBLIC_ORIGIN`のhost。WebAuthn用DNS RP ID |
| `SESSION_TTL_DAYS` | `7`。固定期限session。アクセスで延長しない |
| `TIMEZONE_OFFSET` | `+09:00`。報酬scheduler用の固定UTC offset |
| `HOYOLAB_COOKIE_KEY` | Cookie保存に必要な32-byte/64-hex秘密鍵。DBと別にbackupし、再暗号化toolなしに変更しない |
| `DASHBOARD_UI_DIR` | compiled Dioxus public directory。既定は `target/dx/attos-dashboard-ui/release/web/public` |

```console
cargo install dioxus-cli --version 0.7.10 --locked   # または同versionの公式prebuiltをchecksum確認して使用
rustup target add wasm32-unknown-unknown
dx build --package attos-dashboard-ui --platform web --release
cargo run -- migrate
cargo run -- serve
```

既定のDioxus public出力は `target/dx/attos-dashboard-ui/release/web/public` です。配備時はこのpublic directoryの中身だけを配置し、bundleや `target/dx` 生成物はcommitしません。サーバーは `DASHBOARD_UI_DIR/index.html` が無ければ起動に失敗します。

初期管理者はusers tableが空のときだけ作成できます。仮パスワードは標準入力から渡します。

```bash
read -r -s -p 'Temporary password: ' temporary_password
printf '\n'
printf '%s' "$temporary_password" | cargo run -- bootstrap-admin admin
unset temporary_password
```

### セキュリティ境界

- Session cookieはHTTPSで`__Host-attos_session; Secure; HttpOnly; SameSite=Strict`、localhost HTTPでは`attos_session`。
- 状態変更requestは`PUBLIC_ORIGIN`と完全一致する`Origin`が必要です。Dioxus browser clientはsame-origin credentialsを使い、Cookie/Origin headerを手動で偽装しません。
- JSON mutation DTOは未知fieldを拒否します。エラーDTOはsafe/static/numeric codeだけで、Cookie・password・token・上流raw messageを返しません。
- Dioxus HTML shellには、DX 0.7.10が生成するexternal module loaderだけを許可するCSPを付与します。`script-src 'self' 'wasm-unsafe-eval'` のみで、`'unsafe-inline'` や一般の `'unsafe-eval'` は使わず、`style-src 'self'`, `connect-src 'self'`, `object-src 'none'`, `frame-ancestors 'none'`, `base-uri 'none'` を維持します（Chromiumでrelease bundleのCSP・WASM動作を合成fixtureで確認済み）。
- Password変更は他sessionを失効し、現在sessionの期限は延長しません。Logout・disable・recovery・deleteも該当session/passkeyを失効します。

### HoYoLAB連携の現在地

Cookie受付・暗号化保存、owner lock、account/community identityとCookie claimの整合性検査、roles sync、same-UID update、game UID visibility/cache、手動・定期報酬と削除時確認を提供します。Cookie UIDだけを本人確認の証拠とはせず、verified link、認証・認可・アカウント利用可否の再検査を維持します。報酬POST前のdurable may-have-sent記録とGET確認を維持し、不確定なPOSTは再送しません。ローカルmockや合成UIDは実上流token bindingの証拠ではありません。

削除は本人または管理者が実行でき、actorの現在パスワードと対象usernameの完全一致入力が必要です。最後のactive admin削除は拒否します。受付時に対象をinactive化しsession/passkeyを失効、backgroundで送信済みreward POSTのGET-only確認を有界に行ってから全対象データを削除し、verified UID uniquenessを解放します。

## HoYoLAB認証に必要な情報

海外版HoYoLABの読み取り・ログインボーナスAPIはCookie認証を使います。Dashboardの保存処理はlegacy `ltoken` familyまたはv2 `ltoken_v2` familyを正規化し、UID aliasの矛盾、legacy/v2混在、placeholder、不正文字を拒否します。

v2では `ltuid_v2` / `account_id_v2` と `ltoken_v2` を扱い、`ltmid_v2` / `account_mid_v2` があれば保持します。MIDが不要であることは未実証なので、最小2項目だけで十分とは扱いません。連携検証は現在v2 ltoken credentialを要求し、legacy保存対応は本番owner proofの証拠ではありません。

ゲームレコードにはゲームUID (`game_uid`) とサーバー識別子 (`region`) も必要ですが、これはCookieではありません。検証後のroles syncで取得します。HoYoLAB APIは公式の公開APIではなく、エンドポイント、署名用salt、必要Cookieが予告なく変わる可能性があります。Cookieはパスワード同様の秘密情報として扱ってください。

## 調査したリクエスト仕様

2026-10-12（環境の日時）に`genshin.py` master (`ab289440508ef81a9672a198a53b9a31f9365c4a`) と実エンドポイントへの無効Cookie GETを使って再確認しました。以前の`cf675f3`と比べ、以下の移植対象のURL・Cookie利用・DS署名・ヘッダー値に変更はありません。原神には`GET /gcg/matchList`が追加されていますが、既存の疎通プログラムの範囲には含めません。この再確認時点では有効Cookieでの成功や全レスポンスフィールドの不変性は未確認でした。詳しい結果と根拠は[再確認記録](docs/hoyolab-api.md#以前の仕様との差分と現行値)を参照してください。

アカウント一覧:

- `GET https://api-os-takumi.mihoyo.com/binding/api/getUserGameRolesByCookie`
- Cookie認証と`DS`ヘッダーを使用する。
- 戻り値の`game_biz`、`game_uid`、`region`から対象ゲーム・UID・サーバーを特定する。

ログインボーナス:

- 原神: `https://sg-hk4e-api.hoyolab.com/event/sol`
  - `act_id=e202102251931481`
  - `x-rpc-signgame: hk4e`
- 崩壊：スターレイル: `https://sg-public-api.hoyolab.com/event/luna/hkrpg/os`
  - `act_id=e202303301540311`
  - `x-rpc-signgame: hkrpg`
- 共通して`GET /info`が当日の受け取り状態、`GET /home`が月間報酬一覧、`POST /sign`が受け取り操作。
- 海外版では`lang`、`Referer: https://act.hoyolab.com/`、Cookie、User-Agentを送る。

ゲームレコード:

- 原神: `https://sg-public-api.hoyolab.com/event/game_record/genshin/api`
  - `GET /index`: 基本戦績
  - `GET /dailyNote`: 天然樹脂、探索派遣などのリアルタイム状態
- 崩壊：スターレイル: `https://bbs-api-os.hoyolab.com/game_record/hkrpg/api`
  - `GET /index`: 基本戦績
  - `GET /note`: 開拓力、依頼などのリアルタイム状態
- 共通クエリは`role_id=<game_uid>&server=<region>`。
- Cookie、User-Agent、`x-rpc-app_version`、`x-rpc-client_type`、言語、`DS`署名を送る。

海外版の`DS`は、現在の参照実装と同じくUnix秒、ランダム英字6文字、固定saltをMD5した値です。これは暗号学的な用途ではなく、HoYoLABクライアント識別用のプロトコル値です。

## Cookieの用意

1. ブラウザで <https://www.hoyolab.com> にログインする。
2. 開発者ツールの Application（Firefoxでは Storage）から、`https://www.hoyolab.com` のCookiesを開く。
3. 必要なcredential familyのCookie項目を秘密としてコピーする。コンソールにCookieを表示するスクリプトは実行しない。
4. Dashboardでは暗号化鍵を設定した上で既存UIからCookieを登録し、連携検証を行う。クライアントのlive GETテストでは、実行プロセスの環境変数へ `HOYOLAB_COOKIE` を設定する。`.env`を読ませる前提にせず、値をshell history・文書・ログ・commitへ残さない。

Cookie保存だけではverified linkになりません。account/community identity・Cookie claim・rolesの既存整合性検査に成功する必要があります。

## 現行エンドポイントのテスト

通常のモック検証（外部通信・実Cookieなし）：

```console
cargo test --locked --lib hoyolab::client
```

読み取り専用の実API確認は`src/hoyolab/client/live_tests.rs`へ集約しています。**実通信を承認した場合のみ**、`HOYOLAB_COOKIE`を実行プロセスの環境変数へ設定して次を実行してください。`.env`は読みません。Cookieをコマンド履歴へ残さないよう注意してください。

```console
cargo test --locked --lib hoyolab::client::live_tests::current_read_only_endpoints -- --ignored --exact
```

リダイレクトは追跡せず、各リクエスト20秒・全体90秒・レスポンス4MiBに制限します。Cookie値・UID・ニックネーム・生レスポンスは出力せず、JSONのraw出力機能もありません。`client.accounts()`で連携済みアカウント一覧を取得し、両ゲームの`daily_info`・`daily_home`・`index`・`daily_note`を型付きモデルで検証します。設定不足・対象ゲーム未連携・APIエラーは失敗とし、黙ってスキップしません。対象のGETは次の通りです。

| 対象 | 原神 | 崩壊：スターレイル |
| --- | --- | --- |
| ログインボーナス状態 | `/event/sol/info` | `/event/luna/hkrpg/os/info` |
| 月間報酬一覧 | `/event/sol/home` | `/event/luna/hkrpg/os/home` |
| 基本ゲームレコード | `/event/game_record/genshin/api/index` | `/game_record/hkrpg/api/index` |
| リアルタイム状態 | `/event/game_record/genshin/api/dailyNote` | `/game_record/hkrpg/api/note` |

ユーザー提供の実行結果は **1 passed / 0 failed / 0 ignored（3.72s）** でした。利用可能な実アカウントは1個で、このGET suiteの成功はアクセスを確認するものであり、`account_identity` / `community_identity`やtoken-to-UID binding、reward POST契約全体の証明ではありません。実POST結果は報告されていません。

これは読み取り専用の確認です。ログインボーナスを実際に受け取る`POST .../sign`や公開設定の変更は行いません。リアルタイム状態が`retcode=10102`になる場合は、HoYoLABの戦績画面でリアルタイムノートの公開設定を確認してください。

調査元は[genshin.py](https://github.com/seriaati/genshin.py)の`routes.py`、`daily.py`、原神／スターレイルのBattle Chronicle実装です。
