# AttOsGameAPI

原神・崩壊：スターレイルのHoYoLABアカウントを連携し、ゲームの状態確認とログインボーナスの受け取りを行うセルフホスト型ダッシュボードです。

- パスワード／パスキーによるログイン
- HoYoLAB連携とゲームアカウントの表示管理
- 天然樹脂・開拓力などの状態確認
- ログインボーナスの手動受け取り・時刻指定の自動受け取り
- 管理者によるユーザー作成・無効化・復旧・削除

自動受け取りは初期状態では無効です。利用者が連携を完了し、画面から有効にする必要があります。HoYoLABの仕様変更や追加認証により利用できなくなる場合があります。

## 1. 導入に必要なもの

- Docker EngineとDocker Compose v2（`docker compose`）
- 初回のイメージ作成に必要なインターネット接続
- PostgreSQLのデータを保存するディスク領域
- 外部公開する場合：ドメイン、HTTPS証明書、リバースプロキシ

アプリとPostgreSQLの2コンテナで動作します。HTTPSの終端は別途用意してください。アプリは**1インスタンス**で運用し、複数台・複数レプリカでは起動しないでください。

## 2. 設定ファイルを用意する

リポジトリを取得したディレクトリで実行します。

```sh
cp .env.example .env
chmod 600 .env
```

既存の `.env` がある場合は上書きせず、[.env.example](.env.example)と比較して必要な項目を設定してください。秘密値をGit・チャット・ログへ貼り付けないでください。

### 環境変数

`.env` は `compose.yaml` への設定入力です。コンテナにはComposeに記載した項目だけが渡されます。

| 変数 | 設定内容 |
| --- | --- |
| `DB_NAME` | アプリ用DB名。既定：`attos_dashboard` |
| `DB_USER` | アプリ用DBユーザー。既定：`attos_dashboard`。`postgres`などの管理者は指定しない |
| `DB_PASSWORD` | **必須**。アプリ用DBパスワード |
| `DB_ADMIN_PASSWORD` | **必須**。PostgreSQL管理者用パスワード。`DB_PASSWORD`とは異なる値にする。アプリには渡されない |
| `INITIAL_ADMIN_USERNAME` | 初期管理者名。既定：`admin` |
| `INITIAL_ADMIN_PASSWORD` | **初回起動時に必須**。初期管理者の仮パスワード。12〜256 UTF-8バイト |
| `HOYOLAB_COOKIE_KEY` | **必須**。HoYoLAB Cookie保存用の秘密鍵。暗号学的乱数32バイトを64桁の16進数で指定 |
| `PUBLIC_ORIGIN` | 利用者がブラウザで開くOrigin。例：`https://agd.example.com`。パス・クエリ・末尾の `/` は付けない |
| `RP_ID` | パスキー用のドメイン名。例：`agd.example.com`。スキーム・ポート・パスは付けない |
| `SESSION_TTL_DAYS` | セッション有効日数。正の整数、既定：`7`。アクセスによる延長なし |
| `TIMEZONE_OFFSET` | 自動受け取りの時刻に使う固定UTCオフセット。既定：`+09:00`。夏時間の自動調整なし |

パスワードはパスワードマネージャー等で個別に生成してください。暗号鍵は、例えば手元の安全な端末で `openssl rand -hex 32` により生成し、秘密として `.env` と別の安全な保管先に保存します。**保存済みCookieがある間は `HOYOLAB_COOKIE_KEY` を変更しないでください。** 鍵を失うと保存済みCookieを復号できません。

Composeは `DB_HOST=db`、`DB_PORT=5432`、コンテナ内の待受 `0.0.0.0:8080` を設定します。これらや `DATABASE_URL`、`BIND_ADDR`、`DASHBOARD_UI_DIR` を `.env` に追加する必要はありません。HoYoLABのCookieも `.env` には書かず、ログイン後の画面から登録します。

### 公開URLを選ぶ

**HTTPSで公開する場合**は、`.env` の2項目を実際のドメインへ置き換えます。

```dotenv
PUBLIC_ORIGIN="https://agd.example.com"
RP_ID="agd.example.com"
```

リバースプロキシで、そのドメインへの全リクエストを `http://127.0.0.1:8080` に**パスを変更せず**転送してください。UI・APIは同じOriginで提供します。`/api` などのサブパスへの設置はしないでください。

別コンテナのプロキシでは `127.0.0.1` はそのプロキシ自身を指します。その場合は `compose.yaml` で共有ネットワークを設定して `app:8080` へ転送するなど、到達経路を用意してください。DBを外部公開する必要はありません。

**同じPCからローカル利用する場合**は、必ず両方を変更します。

```dotenv
PUBLIC_ORIGIN="http://localhost:8080"
RP_ID="localhost"
```

ブラウザでは `http://localhost:8080` を開いてください。IPアドレスによる公開や、localhost以外のHTTP公開は使用できません。

## 3. 起動する

```sh
docker compose up -d --build
docker compose ps
curl --fail http://127.0.0.1:8080/health
```

正常時、ヘルスチェックは `{"status":"ok"}` を返します。アプリの起動時にDBの更新と、空のDBへの初期管理者作成が行われます。

| URL | 内容 |
| --- | --- |
| `/` | ダッシュボード |
| `/docs` | API仕様の閲覧画面 |
| `/openapi.json` | OpenAPI形式の詳細仕様 |
| `/health` | アプリ・DBの疎通確認 |

ホストへ公開するポートは、既定で `127.0.0.1:8080` だけです。DBのポートは公開しません。ポートを変える場合は `compose.yaml` の `app.ports` 左側を変更し、プロキシの転送先も合わせてください。ブラウザが使うURLのポートが変わる場合は `PUBLIC_ORIGIN` も合わせます。

```yaml
# 例：ホストの8081からコンテナの8080へ転送
ports:
  - "127.0.0.1:8081:8080"
```

### 初回ログインとHoYoLAB連携

1. 設定した初期管理者名・仮パスワードでログインし、パスワードを変更します。
2. ブラウザで <https://www.hoyolab.com> にログインします。
3. ブラウザの開発者ツールのApplication／StorageからHoYoLABのCookieを確認します。Cookieを表示・送信するコンソールスクリプトは実行しないでください。
4. ダッシュボードのHoYoLAB連携画面にCookieを登録し、連携検証を完了します。
5. 検出された原神・崩壊：スターレイルのアカウントを確認します。自動受け取りを使う場合は、時刻を指定して有効にします。

連携検証にはv2 Cookieの `ltoken_v2` と、UIDを示す `ltuid_v2`／`account_id_v2` が必要です。`ltmid_v2`／`account_mid_v2` がある場合も含めてください。異なるアカウントの値や旧形式のCookieを混在させないでください。Cookie保存だけでは連携完了になりません。

Cookieはパスワードと同様の秘密情報です。他人に渡さず、失効した場合は連携画面から更新してください。ゲームの状態を取得できない場合は、HoYoLAB側の戦績・リアルタイムノートの公開設定も確認してください。

初期管理者はユーザーが1人もいない場合だけ作成されます。既存ユーザーのパスワードは `.env` を変更してもリセットされません。作成後は `INITIAL_ADMIN_PASSWORD` を空にできます。追加ユーザーの作成・復旧は管理画面から行います。

## 4. 停止・更新・バックアップ

### 停止と再起動

```sh
# 停止（DBデータは保持）
docker compose down

# 再起動
docker compose up -d
```

**`docker compose down -v` はDBボリュームも削除します。通常の停止には使わないでください。**

DBデータはComposeの名前付きボリューム `db-data` に保存されます。作業ディレクトリ名やComposeのプロジェクト名を変更すると別のボリュームが選ばれる場合があるため、移設時は注意してください。

### 更新

1. DBと `HOYOLAB_COOKIE_KEY` をバックアップします。
2. 配布元から新しいバージョンを取得します。
3. 次を実行し、`/health` と画面を確認します。

```sh
docker compose up -d --build
```

設定変更も `docker compose up -d` で反映します。`docker compose restart` だけではコンテナの環境変数は更新されません。DBの更新が起動時に行われるため、以前のバージョンへ戻す際はバックアップからの復元が必要になる場合があります。

### バックアップ

書き込みを止めてDBのバックアップを取る例です。保存先のファイルにも機密情報が含まれるため、安全に保管してください。

```sh
umask 077
docker compose stop app
docker compose exec -T db sh -c 'pg_dump -U "$DB_USER" -d "$DB_NAME" -Fc' > attos-dashboard.dump
docker compose start app
```

`pg_dump` の終了コードが0であることを確認してください。**DBと、その時点の `HOYOLAB_COOKIE_KEY` の両方**を保管します。`.env` 自体を保管する場合もアクセス制限・暗号化を行ってください。

`DB_NAME`・`DB_USER`・DBパスワードは空のボリュームを初期化するときの設定です。既存DBでは `.env` を変更するだけでDB名・ユーザー・パスワードは変更されません。認証情報の変更にはDB側の変更も必要です。PostgreSQLのメジャーバージョンや配布イメージを変える場合は、既存ボリュームの流用を前提にせず、バックアップからの移行手順を用意してください。

## 5. APIの概要

ベースパスは `/api/v1` です。JSONを使用し、ダッシュボードと同じOriginからセッションCookieでアクセスします。APIキー・Bearerトークン・OAuth認証は提供していません。

### 認証とリクエストの共通ルール

- ログインで発行されるCookieを以降のリクエストに付けます。HTTPSでは `__Host-attos_session`、ローカルHTTPでは `attos_session` です。Cookieは `HttpOnly`・`SameSite=Strict`、HTTPSではさらに `Secure` です。
- POST・PATCH・DELETEなどの状態変更には、**ログインも含めて** `PUBLIC_ORIGIN` と完全一致する `Origin` ヘッダーが必要です。ブラウザは同一Originで利用してください。
- JSON送信時は `Content-Type: application/json` を指定します。定義にないJSONフィールドは拒否されます。
- 初回・復旧直後はパスワード変更が必要です。HoYoLAB連携が完了するまで利用できない機能もあります。
- パスワード変更で他のセッションは失効します。現在のセッションの有効期限は延長されません。

ログインの形式（値は説明用のプレースホルダー）：

```http
POST /api/v1/auth/login
Origin: https://agd.example.com
Content-Type: application/json

{"username":"admin","password":"<password>"}
```

成功時はユーザー情報をJSONで返し、`Set-Cookie` でセッションを発行します。その後 `GET /api/v1/me` でユーザー情報・初期設定の進行状態・利用可能な機能を確認できます。

### 主なエンドポイント

以下はすべてベースパス `/api/v1` からの相対パスです。`{id}` などの識別子は一覧APIで取得します。通常のユーザーは自分のデータだけを操作でき、管理機能には管理者権限が必要です。

| 分類 | メソッド・パス | 用途 |
| --- | --- | --- |
| 認証 | `POST /auth/login`、`POST /auth/logout` | ログイン・ログアウト |
| 本人情報 | `GET /me`、`PUT /me/password` | 本人情報の取得・パスワード変更 |
| パスキー | `GET /me/passkeys`、`DELETE /me/passkeys/{id}` | 登録済みパスキーの一覧・削除 |
| パスキー登録 | `POST /me/passkeys/options`、`POST /me/passkeys/verify` | 登録手続きの開始・完了 |
| パスキー認証 | `POST /auth/passkey/options`、`POST /auth/passkey/verify` | パスキーでログイン |
| HoYoLAB | `GET /me/hoyolab`、`PUT /me/hoyolab/cookie` | 連携状態の確認・Cookie登録／更新 |
| 連携検証 | `POST /me/hoyolab/verify`、`POST /me/hoyolab/sync` | 連携検証・ゲームアカウントの同期 |
| ゲーム | `GET /game-accounts`、`PATCH /game-accounts/{id}` | 検出されたアカウントの一覧・表示設定 |
| ゲーム状態 | `GET /game-accounts/{id}/status` | ゲーム状態の取得 |
| 報酬設定 | `GET /reward-settings`、`PATCH /reward-settings/{id}` | ゲームごとの自動受け取り設定 |
| 手動受け取り | `POST /reward-settings/{id}/run` | 受け取り処理の受付 |
| 受け取り履歴 | `GET /reward-runs` | 履歴・実行状態の確認 |
| ユーザー管理 | `GET /admin/users`、`POST /admin/users`、`PATCH /admin/users/{id}` | ユーザー一覧・作成・権限／有効状態の変更 |
| パスワード再発行 | `PUT /admin/users/{id}/temporary-password` | 管理者による仮パスワード再発行 |
| ユーザー削除 | `DELETE /me`、`DELETE /admin/users/{id}` | 自分／管理対象ユーザーの削除受付 |
| サーバー設定 | `GET /server-settings` | タイムゾーン・セッション有効日数の確認 |

ゲーム種別 `game_kind` は `genshin` または `starrail` です。ゲーム状態には取得日時や `stale`（古い状態かどうか）が含まれるため、表示値が最新かどうかも確認してください。報酬設定の `{id}` は `GET /reward-settings` で取得する設定のUUIDであり、ゲームUIDやゲームアカウントのUUIDではありません。

パスワード変更は `{"current_password":"<current password>","new_password":"<new password>"}` を送ります。初回・仮パスワード再発行直後の必須変更時のみ `current_password` を省略できます。連携検証・同期・手動受け取りのPOSTには空のJSON `{}` を送ります。

手動受け取りは非同期です。`202 Accepted` は受け取り完了を意味しません。履歴APIで結果を確認してください。結果が不確定な場合は、重複受け取りを避けるためにむやみに再実行しないでください。追加認証が必要な場合はHoYoLAB側で対応します。

ユーザー削除も非同期で、受付時に利用停止・セッション失効となり、処理完了後に関連データが削除されます。本人確認用の現在パスワードと、削除対象のユーザー名の完全一致入力が必要です。最後の有効な管理者は削除できません。

### エラー

認証・権限・入力・上流通信などのエラーは、原則として次の形式です。

```json
{"error":"unauthorized","message":"invalid credentials or expired session"}
```

| HTTPステータス | 主な意味 |
| --- | --- |
| `400` | 入力形式・値が不正 |
| `401` | 未ログイン、セッション失効 |
| `403` | 権限不足、Origin不一致、パスワード変更／初期設定が必要 |
| `404` | 対象が見つからない |
| `409` | 状態の競合、重複など |
| `429` | リクエスト頻度の制限 |
| `502` | HoYoLABとの通信・応答のエラー |
| `503` | 一時的に処理できない、必要な機能が利用できない |

全エンドポイントの入力項目・レスポンス・必要な権限は、起動後の `/docs`・`/openapi.json`、または [OpenAPI仕様](docs/dashboard-api.json)を参照してください。

## 6. 困ったとき

| 症状 | 確認すること |
| --- | --- |
| Composeが設定不足で停止する | `.env` の `DB_PASSWORD`・`DB_ADMIN_PASSWORD`・`HOYOLAB_COOKIE_KEY` が空でないか |
| 初回起動で管理者を作成できない | 初期管理者名・仮パスワードが設定されているか。仮パスワードは12〜256 UTF-8バイト |
| ログインや更新操作が `403` になる | ブラウザのURLと `PUBLIC_ORIGIN` が一致するか。HTTP／HTTPS・ホスト名・ポートも確認 |
| パスキーを登録・利用できない | HTTPS（またはlocalhost）か。`RP_ID` が公開ドメインと一致するか |
| DBへ接続できない | 既存ボリュームの初期化後に `.env` のDB認証情報を変更していないか |
| 保存済みHoYoLAB連携が使えない | Cookieの失効や公開設定を確認。DB復元後なら元の暗号鍵を使用しているか |
| 報酬が自動で受け取られない | 連携検証、自動受け取りの有効化、設定時刻と `TIMEZONE_OFFSET`、実行履歴を確認 |

状況確認には `docker compose ps` や `docker compose logs --tail=100 app` を使えます。診断のために `.env` やCookieを表示しないでください。ログを共有する場合も、個人情報・秘密情報が含まれていないことを確認してください。
