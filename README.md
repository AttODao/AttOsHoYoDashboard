# AttOsGameAPI

原神・崩壊：スターレイルのHoYoLAB APIを呼ぶ型付きRustクライアントと、実装中のDashboardバックエンドです。現在のバックエンドはSeaORM＋PostgreSQL、パスワード／パスキー認証・セッション・管理者のユーザー管理に対応しています。GUI・HoYoLAB連携・自動受け取りのWeb機能は未実装です。

Dashboardの機能仕様・実装設計案・実装計画は[ドキュメント一覧](docs/README.md)を参照してください。

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

`daily_sign(Genshin)`／`daily_sign(Starrail)`だけは実際に報酬を受け取るPOSTです。自動再試行はしません。成功retcodeでも追加認証が必要な場合があるため、戻り値の`requires_verification()`と認証情報を確認してください。`None`は受取成功を意味せず、受取の確定には`daily_info`の`is_sign`を再確認します。テストはローカルのモックAPIだけを使用し、本番の受け取り操作は行いません。

クライアントのエラーは`hoyolab::Error`です。通信・HTTP・retcode・レスポンス形式を型で区別でき、文字列診断には`to_string()`を使います。上流の`message`は秘密情報を含み得るため保持・表示しません。`Result<T, String>`境界の`?`変換は提供しますが、String固有の操作は明示的な変換が必要です。

`GameKind`は`Genshin(Genshin)`／`Starrail(Starrail)`を持ち、JSONでは`"genshin"`／`"starrail"`として読み書きします。実行時にゲームを選ぶ場合は、このenumを`match`して型付きメソッドを呼びます。新しいゲームはstruct、`Game`実装、対応モデル、`GameKind`への追加で対応できます。HTTPサーバー側のゲームendpointは未実装のままです。

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
├── cli.rs                      # コマンド配線・標準入力
├── config.rs / error.rs        # 設定検証・秘匿された業務エラー
├── auth/
│   ├── accounts.rs             # 作成・復旧・無効化・利用可否・監査
│   ├── password.rs             # Argon2idと有界CPU処理
│   ├── sessions.rs             # 発行・固定期限・認証・失効
│   └── passkeys/
│       ├── mod.rs              # WebAuthn構築・CPU枠・鍵一覧／削除
│       ├── registration.rs     # 元セッション拘束の登録
│       ├── authentication.rs   # 現在の資格情報照合・原子的ログイン
│       ├── challenge.rs        # 五分・一回使用・有界pending状態
│       └── credential.rs       # 保存JSONの読出し・codec・fingerprint
├── hoyolab/
│   ├── credentials.rs          # Cookie正規化・所有者に束縛したAES-GCM
│   └── store.rs                # 再認証・userロック・未検証Cookie保存
├── http/
│   ├── mod.rs                  # ルート合成・HTTPサーバーの生存期間
│   ├── auth.rs / users.rs / passkeys.rs  # 機能別DTO・HTTPハンドラー
│   ├── middleware.rs           # 共通Origin境界・セッション抽出
│   ├── session.rs              # Cookie・ログイン応答
│   └── response.rs             # 安全なHTTPエラー変換
└── db/
    ├── mod.rs                  # 接続・migration実行
    ├── entities/               # user・session・audit・passkey・hoyolab_accountの保存モデル
    └── migrations/             # 順序レジストリ・初期account・passkeyスキーマ
```

HTTPは輸送と入力型、`auth/`は認証・ユーザーライフサイクル、`db/`は保存モデルとスキーマを担当します。認可や復旧ルールをハンドラーに複製せず、DBトランザクション・userロックは業務側で一か所に維持します。HoYoLAB連携業務は`src/dashboard/hoyolab/`へ分離し、既存`src/hoyolab/`は上流プロトコルのみを扱います。構造変更・認証・永続化の増分ごとにhighレビューする方針は`AGENTS.md`に記載しています。

## ビルド・テスト

Rustのstableツールチェーン（edition 2024対応）を使用します。WebAuthnのビルドには`pkg-config`とOpenSSLの開発ヘッダー／ライブラリが必要です。PostgreSQLは実行時に別途用意します。

```console
cargo build --locked --bins
cargo test --locked --all-targets
```

Dashboardテストは`initdb`・`pg_ctl`を使い、テストごとに専用PostgreSQLクラスタを作成・破棄します。**非rootユーザー**で実行してください。実環境の`DATABASE_URL`やCookieを使用せず、ツール不足時は明示的に失敗します。ツールをPATHに置けない場合：

```console
ATTOS_TEST_PG_BIN=/path/to/postgresql/bin cargo test --locked --all-targets
```

テストは設定・CLI・DB制約・アカウント・パスワード・セッション・パスキー・HTTP・HoYoLABプロトコルごとに所有モジュールへ配置しています。例：`cargo test --locked --bin attosgameapi dashboard::auth::sessions`。APIテストは現在実装済みのエンドポイントだけを対象とし、HoYoLABモックは実際のHTTPSホスト名・パスをループバックへ解決します。実APIテスト1件は外部通信の明示承認を要するため通常実行では`ignored`です。DBテストのツール不足や実APIテストの明示実行時の設定不足は失敗させます。合成UID・ソフトウェア鍵・モックは実APIの本人UID拘束・実認証器の確認の代わりではありません。

バックエンドの起動手順は次節を参照してください。従来の空の`/docs`・`/openapi.json`は削除しました。ゲーム操作APIとOpenAPI契約は後続の実装対象です。

詳細なリクエスト仕様は [docs/hoyolab-api.md](docs/hoyolab-api.md) を参照してください。

ゲームレコードAPIのエンドポイント一覧とレスポンス構造は [docs/hoyolab-game-record/README.md](docs/hoyolab-game-record/README.md) を参照してください。

## Dashboardバックエンド（基盤・認証・Cookie受付）

新たに確定したCookie仕様は「1ユーザー1HoYoLAB連携、アカウント登録時（仮登録では初回ログイン時）にCookie設定、解除はDashboardユーザー削除時のみ、同一UIDのCookie更新は可能」です。Cookie入力の正規化・暗号化した未検証保存まで実装しました。**本人UID検証・Cookie初期設定完了後の通常利用ゲート・ユーザー削除は未実装**です。未検証保存を連携成功と扱いません。

PostgreSQLの専用DBを用意し、`.env.example`から作った`.env`に`DATABASE_URL`を設定します。環境変数が`.env`より優先されます。設定ファイルはGUIを使わず編集できます。

| 設定 | 初期値・用途 |
|---|---|
| `DATABASE_URL` | 必須。`postgres://...`または`postgresql://...` |
| `BIND_ADDR` | `127.0.0.1:8080` |
| `PUBLIC_ORIGIN` | `http://localhost:8080`。公開時はHTTPSのDNS名。WebAuthnのためIPリテラルではなくDNS名または`localhost`を使う |
| `RP_ID` | `PUBLIC_ORIGIN`のホスト名。必要な場合だけ指定し、WebAuthnがOriginとの関係を検証 |
| `SESSION_TTL_DAYS` | `7`。ログイン時に固定期限を保存し、アクセスでは延長しない |
| `TIMEZONE_OFFSET` | `+09:00`。固定UTCオフセット。スケジューラー自体は未実装 |
| `HOYOLAB_COOKIE_KEY` | Cookie保存には必須。32バイトの秘密鍵を64桁hexで設定。未指定ならCookie APIは503、不正値なら設定読込失敗。DBと別に管理し、再暗号化なしに変更しない |

```console
cargo run -- migrate
```

初期管理者はユーザーテーブルが空のときだけ作成できます。仮パスワードは標準入力から渡し、コマンド引数や履歴に秘密値を残さないでください。Bashでの入力例：

```bash
read -r -s -p 'Temporary password: ' temporary_password
printf '\n'
printf '%s' "$temporary_password" | cargo run -- bootstrap-admin admin
unset temporary_password
cargo run -- serve
```

ユーザー名は1〜64文字のASCII英数字・`.`・`_`・`-`（大文字小文字を区別）、パスワードは12〜256 UTF-8バイトです。初期管理者にも仮パスワード変更が必要です。管理者が自分自身を無効化する操作は、管理不能を避けるため拒否します。

現在のAPI（GUIは未実装）：

| Method | Path | 用途 |
|---|---|---|
| GET | `/health` | DB疎通確認 |
| POST | `/api/v1/auth/login` | `username`・`password`でログイン |
| POST | `/api/v1/auth/logout` | 当該セッションの失効 |
| GET | `/api/v1/me` | 本人情報とPW変更必須状態 |
| GET | `/api/v1/me/hoyolab` | Cookie設定状態のみ。Cookie・UID・暗号文は返さない |
| PUT | `/api/v1/me/hoyolab/cookie` | `cookie`を正規化・暗号化して未検証保存。PW変更後のみ。202／`pending_verification`／`setup_complete:false` |
| PUT | `/api/v1/me/password` | `current_password`・`new_password`。他のセッションも失効し、元の期限を維持して秘密値を更新 |
| POST | `/api/v1/auth/passkey/options` | `username`でパスキーログイン開始 |
| POST | `/api/v1/auth/passkey/verify` | `challenge_id`・`response`でログイン完了 |
| GET | `/api/v1/me/passkeys` | 本人の鍵のID・ラベル・作成日時 |
| POST | `/api/v1/me/passkeys/options` | `label`で登録開始 |
| POST | `/api/v1/me/passkeys/verify` | `challenge_id`・`response`で登録完了 |
| DELETE | `/api/v1/me/passkeys/{id}` | 本人の鍵を削除 |
| GET | `/api/v1/server-settings` | 共通オフセットとセッション保持期間 |
| GET, POST | `/api/v1/admin/users` | 一覧／`username`・`temporary_password`で一般ユーザー作成 |
| PATCH | `/api/v1/admin/users/{id}` | `active`で無効化・再有効化 |
| PUT | `/api/v1/admin/users/{id}/temporary-password` | `temporary_password`で認証復旧 |

状態変更リクエストには`PUBLIC_ORIGIN`と完全一致する`Origin`ヘッダーが必要です。JSON入力を使用し、ログイン後はセッションCookieを送信します。HTTPS時は`__Host-` Cookie・Secure・HttpOnly・SameSite=Strictを使い、秘密値を応答JSON・SQLログ・エラー診断へ返しません。ログイン入口（パスワード／パスキー開始・完了）はアプリ全体で毎分20リクエストに制限しています。PW／パスキーのCPU処理枠が満杯なら待機列を増やさず503を返します。

仮PW変更前は本人情報・Cookie設定状態・PW変更・ログアウトを利用でき、Cookie送信などの通常機能は拒否します。復旧操作は操作者・対象・日時をDBへ記録します。復旧時の全パスキー削除は全セッション失効と同じトランザクションで実行します。パスキーchallengeは5分・一回使用・単一プロセスのメモリ保持（最大128件）で、再起動時はceremonyを最初から行います。鍵はユーザーごとに最大20件、ラベルは1〜100 UTF-8バイト。レスポンスの`options`とブラウザーのWebAuthn結果を往復させるGUIは未実装です。再有効化時の自動受け取りOFFは、受け取りモジュール追加時に同じ業務処理へ組み込みます。

### Cookie保存の現状

`HOYOLAB_COOKIE_KEY`には暗号学的乱数で生成した32バイト鍵を設定し、秘密の設定管理へ保存してください。鍵・CookieをGitや公開ログへ書き込まないでください。暗号文はAES-256-GCMで所有ユーザー・検証済みUID（未検証ではnull）へ束縛し、鍵違い・改竄・行の取り違えは復号失敗になります。

Cookie内のUIDは**自己申告**です。受け付けたCookieは`hoyolab_uid=null`・`pending_verification`のまま保存し、本人確認APIやroles通信はまだ実行しません。保存済みの検証済みUIDがある行は、本人検証が配線されるまで更新を503で拒否し、別UIDにも同一UIDにも黙って書き換えません。ユーザーごとの行は一意、検証済みUIDも全ユーザーで一意です。既存の認証基盤の利用条件は維持しており、Cookie完了ゲートを含む新仕様全体は未完了です。

## HoYoLAB認証に必要な情報

海外版HoYoLABでは、今回確認するログインボーナスとゲームレコードのGET APIに次のCookieを使用します。

- `ltuid_v2`: HoYoLABアカウントID
- `ltoken_v2`: HoYoLAB認証トークン

ゲームレコードにはゲームUID (`game_uid`) とサーバー識別子 (`region`) も必要ですが、これはCookieではありません。`getUserGameRolesByCookie`から自動取得します。

`cookie_token_v2` / `account_id_v2` はコード交換など別のAPIで必要になることがありますが、今回の読み取り確認には不要です。`stoken`は期限切れCookieの再発行を実装するときに役立ちますが、このテストには保存しません。

HoYoLAB APIは公式の公開APIではなく、エンドポイント、署名用salt、必要Cookieが予告なく変わる可能性があります。Cookieはパスワード同様の秘密情報として扱ってください。

## 調査したリクエスト仕様

2026-10-12（環境の日時）に`genshin.py` master (`ab289440508ef81a9672a198a53b9a31f9365c4a`) と実エンドポイントへの無効Cookie GETを使って再確認しました。以前の`cf675f3`と比べ、以下の移植対象のURL・Cookie利用・DS署名・ヘッダー値に変更はありません。原神には`GET /gcg/matchList`が追加されていますが、既存の疎通プログラムの範囲には含めません。有効Cookieでの成功や全レスポンスフィールドの不変性は未確認です。詳しい結果と根拠は[再確認記録](docs/hoyolab-api.md#以前の仕様との差分と現行値)を参照してください。

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
3. `ltuid_v2` と `ltoken_v2` の値をコピーする。コンソールにCookieを表示するスクリプトは実行しない。
4. 次のようにローカル設定を作る。

```console
cp .env.example .env
```

`.env`を編集します。

```dotenv
HOYOLAB_COOKIE="ltuid_v2=123456789; ltoken_v2=v2_xxxxxxxxxxxxxxxxx"
```

`.env`はルートの`.gitignore`で除外しています。実値を`.env.example`やソースコードに書かないでください。

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

これは読み取り専用の確認です。ログインボーナスを実際に受け取る`POST .../sign`や公開設定の変更は行いません。リアルタイム状態が`retcode=10102`になる場合は、HoYoLABの戦績画面でリアルタイムノートの公開設定を確認してください。

調査元は[genshin.py](https://github.com/seriaati/genshin.py)の`routes.py`、`daily.py`、原神／スターレイルのBattle Chronicle実装です。
