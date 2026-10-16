# AttOsGameAPI

原神・崩壊：スターレイルのHoYoLAB連携、天然樹脂・開拓力などの状態確認、ログインボーナスの手動／定期受け取りを提供するセルフホスト型ダッシュボード。パスワード／パスキー認証と管理者によるユーザー管理に対応します。

## 導入

Docker Compose v2を使用します。`compose.yaml` はアプリ＋PostgreSQLの構成で、アプリの公開先は `127.0.0.1:8080`、DBは非公開です。**アプリは1インスタンス限定**です。

```sh
cp .env.example .env
chmod 600 .env
```

`.env` を設定して起動します。

```sh
docker compose up -d --build
curl --fail http://127.0.0.1:8080/health
```

起動時にDBマイグレーションと初期管理者作成を行います。初期管理者はユーザーが0人のときだけ作成され、初回ログインでパスワード変更が必要です。作成後は `INITIAL_ADMIN_PASSWORD` を空にできます。既存ユーザーの復旧は管理画面から行います。

### 環境変数

| 変数 | 用途・既定値 |
| --- | --- |
| `DB_NAME` / `DB_USER` | アプリ用DB／非管理者ロール。ともに `attos_dashboard` |
| `DB_PASSWORD` | 必須。アプリ用DBパスワード |
| `DB_ADMIN_PASSWORD` | 必須。DB管理者用。アプリ用とは別の値。アプリには渡さない |
| `INITIAL_ADMIN_USERNAME` | 初期管理者名。`admin` |
| `INITIAL_ADMIN_PASSWORD` | 初回必須の仮パスワード。12〜256 UTF-8バイト |
| `HOYOLAB_COOKIE_KEY` | 必須。Cookie暗号化鍵。乱数32バイトの64桁hex（`openssl rand -hex 32`） |
| `PUBLIC_ORIGIN` | 公開Origin。例：`https://agd.example.com`。パスなし |
| `RP_ID` | パスキー用ドメイン。例：`agd.example.com` |
| `SESSION_TTL_DAYS` | 固定有効日数。正の整数、`7`。アクセスによる延長なし |
| `TIMEZONE_OFFSET` | 自動受け取りの固定UTCオフセット。`+09:00`。夏時間非対応 |

DB接続先・コンテナ内待受・UI配置先はCompose／イメージ側で設定済みです。HoYoLAB Cookieは `.env` ではなく画面から登録します。

### 公開先

HTTPSリバースプロキシから `http://127.0.0.1:8080` へパスを保持して転送します。UI・APIは同一Originで、サブパス設置は非対応です。

```dotenv
PUBLIC_ORIGIN="https://agd.example.com"
RP_ID="agd.example.com"
```

ローカル利用時は `PUBLIC_ORIGIN="http://localhost:8080"`、`RP_ID="localhost"` に変更します。IPリテラルやlocalhost以外のHTTPは使用できません。

| パス | 内容 |
| --- | --- |
| `/` | ダッシュボード |
| `/docs` / `/openapi.json` | API仕様の閲覧／OpenAPI JSON |
| `/health` | DB疎通確認。正常時 `{"status":"ok"}` |

## HoYoLAB連携

初回パスワード変更後、連携画面からHoYoLAB Cookieを登録し、検証を完了してください。Cookieは <https://www.hoyolab.com> にログインしたブラウザのApplication／Storageから取得します。

- 検証にはv2の `ltoken_v2` と `ltuid_v2`／`account_id_v2` が必要です。`ltmid_v2`／`account_mid_v2` があれば含めてください。旧形式や別アカウントの値との混在は不可です。
- 保存だけでは連携完了になりません。検証後にゲームアカウントを確認してください。
- **自動受け取りは初期OFF**。画面で時刻を指定して有効にします。
- 状態取得にはHoYoLAB側の戦績・リアルタイムノートの公開設定も影響します。Cookie失効・追加認証・HoYoLABの仕様変更により利用できなくなる場合があります。

## 運用上の制約

- 永続データは `db-data` ボリュームに保存します。`DB_NAME`・`DB_USER`・DBパスワードは初回初期化用で、既存DBには `.env` の変更だけでは反映されません。
- **DBと対応する `HOYOLAB_COOKIE_KEY` をセットでバックアップ**してください。鍵の変更・再暗号化ツールはありません。保存済みCookieがある状態で鍵を変更すると復号できません。
- 更新はバックアップ後に `docker compose up -d --build`。起動時のDB更新を伴うため、ロールバックにDB復元が必要になる場合があります。

## API概要

ベースパスは `/api/v1`。同一OriginのWebセッションAPIで、APIキー・Bearer・OAuthは提供しません。詳細は [OpenAPI仕様](docs/dashboard-api.json)または稼働中の `/docs` を参照してください。

- セッションCookieはHTTPSで `__Host-attos_session`、localhost HTTPで `attos_session`。
- **ログインを含む状態変更リクエストには、`PUBLIC_ORIGIN` と完全一致する `Origin` が必須**です。
- 初回／復旧後のパスワード変更前は操作が制限されます。ゲーム・報酬機能にはHoYoLAB連携検証の完了も必要です。

以下のパスは `/api/v1` からの相対パスです。

| 機能 | 主なエンドポイント |
| --- | --- |
| 認証 | `POST /auth/login`、`POST /auth/logout`、`GET /me`、`PUT /me/password` |
| パスキー | `GET /me/passkeys`、`POST /me/passkeys/options`・`/me/passkeys/verify`、`POST /auth/passkey/options`・`/auth/passkey/verify` |
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
