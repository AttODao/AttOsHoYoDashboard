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

HoYoLAB Cookieは環境変数ではなく画面から登録します。

### 公開先

HTTPSリバースプロキシから `http://127.0.0.1:8080` へパスを保持して転送します。UI・APIは同一Originで、サブパス設置は非対応です。

ローカル利用時は `PUBLIC_ORIGIN: http://localhost:8080`、`RP_ID: localhost` に変更します。IPリテラルやlocalhost以外のHTTPは使用できません。

| パス | 内容 |
| --- | --- |
| `/` | セッション確認後、未ログインなら `/login`、ログイン済みなら `/dashboard` |
| `/login` / `/dashboard` | ログイン／ダッシュボード。初期設定未完了時は設定ゲートを表示 |
| `/settings` | `/settings/games` へ308転送 |
| `/settings/games` / `/settings/bonus` / `/settings/link` / `/settings/security` / `/settings/delete` | 各設定画面（認証必須） |
| `/admin` | 管理者画面（管理者権限必須） |
| `/docs` / `/openapi.json` | API仕様の閲覧／OpenAPI JSON |
| `/health` | DB疎通確認。正常時 `{"status":"ok"}` |

## 開発とUI設計

ビルド・検証は `devenv shell -- …` 経由で実行します。UI回帰は `devenv shell -- test-ui`。

Impeccableは `.pi/skills/impeccable/` にGit管理したプロジェクト専用スキルです。ルートの [PRODUCT.md](PRODUCT.md) と [DESIGN.md](DESIGN.md) が本実装のコンテキストです。

```sh
.pi/skills/impeccable/scripts/impeccable context --target dashboard-ui/src
# ランチャーの隔離検証（外部通信なし）
devenv shell -- node .pi/skills/impeccable/scripts/launcher.test.mjs
```

エンジンは初回に公開releaseからSHA-256を確認して `.impeccable/cache/` へ取得します（未追跡）。curl／wgetがない場合は既存のNodeを使用します。ユーザー共通設定・スキルは変更しません。Pi向け自動detector hookはないため、UI変更の最後にスキルの指示に従い手動検査します。

### UIモジュール

| パス | 責務 |
| --- | --- |
| `dashboard-ui/src/app/navigation.rs` | 通常パス・History API・認証状態に応じた転送と戻る／進むの対応 |
| `dashboard-ui/src/app/state.rs` | 画面状態、認証・初期設定ゲートと画面データ取得の起動 |
| `dashboard-ui/src/app/session.rs` | `/me`による起動・再確認、初期設定ゲートのその場での再読み込み、ログアウト |
| `dashboard-ui/src/app/actions.rs` | 操作の予約（busy）と完了・APIエラー処理。セッション世代とは独立 |
| `dashboard-ui/src/app/data.rs` | セッション世代に結び付いた画面データの取得（未取得と空を区別） |
| `dashboard-ui/src/app/display.rs` | 数値・ブラウザのローカル時刻による簡潔な月日／24時間表示 |
| `dashboard-ui/src/pages/` | 画面、メニュー、設定、ユーザー管理 |
| `src/dashboard/games/summary.rs` | 上流ゲーム状態から画面用の要約への写像 |

### バックエンドモジュール

| パス | 責務 |
| --- | --- |
| `src/dashboard/auth/` | ユーザーのライフサイクル、パスワード、セッション、パスキー、削除要求と完了（再試行の待機を含む）、ユーザー名ごとのログイン試行制限 |
| `src/dashboard/http/` | ルート、DTO、Cookie、Origin・キャッシュの境界、UI配信、報酬・削除のバックグラウンド処理の起動 |
| `src/dashboard/db/` | `entities/` に保存モデル、`migrations/` にスキーマ変更 |
| `src/dashboard/hoyolab/` | Cookieの解析・暗号化保存、連携の検証・同期、ユーザー単位の操作ロック |
| `src/dashboard/games/` | ゲームUID一覧・表示設定・状態キャッシュ |
| `src/dashboard/rewards/` | 報酬設定・スケジュール・実行、送信不確定時のガード、ユーザー削除前の報酬確認 |
| `src/hoyolab/` | HoYoLAB上流APIの型付きクライアント（ダッシュボードの方針は含めない） |

## HoYoLAB連携

初回パスワード変更後、連携画面からHoYoLAB Cookieを登録し、検証を完了してください。Cookieは <https://www.hoyolab.com> にログインしたブラウザのApplication／Storageから取得します。

- 登録には `ltoken_v2`・`ltuid_v2`・`ltmid_v2` の3つが必須です。ブラウザの `Cookie:` ヘッダー行をそのまま貼り付けても構いません（他のCookieや空の要素は無視されます）。旧形式（`ltoken`／`ltuid`）は登録できません。
- 保存だけでは連携完了になりません。検証後にゲームアカウントを確認してください。
- **自動受け取りは初期OFF**。画面で時刻を指定して有効にします。
- 状態取得にはHoYoLAB側の戦績・リアルタイムノートの公開設定も影響します。Cookie失効・追加認証・HoYoLABの仕様変更により利用できなくなる場合があります。

## 運用上の制約

- 永続データは `db-data` ボリュームに保存します。`POSTGRES_DB`・`POSTGRES_USER`・`POSTGRES_PASSWORD` は初回初期化用で、既存DBには `compose.yaml` の変更だけでは反映されません。
- **DBと対応する `HOYOLAB_COOKIE_KEY` をセットでバックアップ**してください。鍵の変更・再暗号化ツールはありません。保存済みCookieがある状態で鍵を変更すると復号できません。
- 更新はバックアップ後に `docker compose pull && docker compose up -d`。起動時のDB更新を伴うため、ロールバックにDB復元が必要になる場合があります。

## リリースイメージ

`v*` タグのpushで `.forgejo/workflows/image.yml` がlinux/amd64イメージをビルドし、Forgejoのコンテナレジストリ（`<Forgejoホスト>/<owner>/<repo>`、小文字）へ公開します。CIではテストを実行しないため、タグ付け前に `devenv test` を通してください。

- `v1.2.3` → `1.2.3`・`1.2`・`latest`。`v1.2.3-rc.1` などはそのタグのみ
- runnerはラベル `ubuntu-latest`、ジョブから `docker`／`buildx` でDinDへ接続できること
- リポジトリSecret `REGISTRY_TOKEN`：タグをpushするユーザーの `write:package` 権限付きアクセストークン
- ビルドキャッシュは同じイメージの `buildcache` タグに保存

## API概要

ベースパスは `/api/v1`。同一OriginのWebセッションAPIで、APIキー・Bearer・OAuthは提供しません。詳細は [OpenAPI仕様](docs/dashboard-api.json)または稼働中の `/docs` を参照してください。

- セッションCookieはHTTPSで `__Host-attos_session`、localhost HTTPで `attos_session`。
- **ログインを含む状態変更リクエストには、`PUBLIC_ORIGIN` と完全一致する `Origin` が必須**です。
- 初回／復旧後のパスワード変更前は操作が制限されます。ゲーム・報酬機能にはHoYoLAB連携検証の完了も必要です。

以下のパスは `/api/v1` からの相対パスです。

| 機能 | 主なエンドポイント |
| --- | --- |
| 認証 | `POST /auth/login`、`POST /auth/logout`、`GET /me`、`PUT /me/password` |
| パスキー | `GET /me/passkeys`、`POST /me/passkeys/options`・`/me/passkeys/verify`、`DELETE /me/passkeys/{id}`、`POST /auth/passkey/options`・`/auth/passkey/verify` |
| HoYoLAB | `GET /me/hoyolab`、`PUT /me/hoyolab/cookie`、`POST /me/hoyolab/verify`・`/me/hoyolab/sync` |
| ゲーム | `GET /game-accounts`、`PATCH /game-accounts/{id}`、`GET /game-accounts/{id}/status` |
| 報酬 | `GET /reward-settings`、`PATCH /reward-settings/{id}`、`POST /reward-settings/{id}/run`、`GET /reward-runs` |
| ユーザー管理 | `GET /admin/users`、`POST /admin/users`、`PATCH /admin/users/{id}`、`PUT /admin/users/{id}/temporary-password` |
| ユーザー削除 | `DELETE /me`、`DELETE /admin/users/{id}` |
| サーバー設定 | `GET /server-settings` |

- ゲーム種別は `genshin`／`starrail`。状態レスポンスの `stale` と取得日時を確認してください。
- 報酬操作の `{id}` は報酬設定のUUIDで、ゲームUIDではありません。検証・同期・手動受け取りのPOSTには `{}` を送ります。
- 手動受け取りは非同期の `202` 受付です。結果は履歴で確認します。不確定な送信結果は再POSTせず、追加認証が必要ならHoYoLAB側で対応してください。
- 削除も非同期で、受付時に利用停止・セッション失効となります。操作者の現在パスワードと対象ユーザー名の完全一致入力が必要です。最後の有効な管理者は削除できません。
