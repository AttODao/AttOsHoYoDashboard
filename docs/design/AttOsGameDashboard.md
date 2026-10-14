# AttOsGameDashboard 実装設計案

## 1. 構成

現在のDashboardは単一Rustプロセスで、Axum HTTP、Tokio runtime、SeaORM + PostgreSQL、Argon2id、WebAuthn、Dioxus 0.7.10 Web/WASM RSX UIを使う。UIはstandalone workspace crate `dashboard-ui` / package `attos-dashboard-ui` で構築し、`dx build --package attos-dashboard-ui --platform web --release` のpublic出力を `DASHBOARD_UI_DIR` から配信する。公式Dioxus Components由来のButton/Card/Card partsをDioxusLabs/components `9a758255ea26e2b20c8cecf4c1c946feb9e71da7`（MIT）からsource templateとして同梱し、`dioxus-primitives` も同じimmutable SHAにpinする。SSR/fullstack server、native app、CDN、危険なHTML injectionや`document::eval`は使わない。

```mermaid
flowchart TB
    browser["Dioxus Web/WASM RSX UI"] --> api["Axum Web Session API"]
    api --> auth["auth: accounts/sessions/passkeys"]
    api --> hl["HoYoLAB proof gates"]
    api --> games["games: role-owned status cache"]
    api --> rewards["rewards: settings/runs/worker"]
    rewards --> hl
    games --> hl
    hl --> client["typed HoYoLAB client"]
    auth --> db[(PostgreSQL)]
    hl --> db
    games --> db
    rewards --> db
    client -. gated live calls .-> upstream["HoYoLAB upstream"]
```

責務:

- `dashboard-ui/src/app.rs`: App合成・起動・60秒周期のDashboard更新。`app/state.rs`はAppState・Page/Phaseと状態操作、`data.rs`は画面データ取得、`session.rs`はbootstrap・fresh me・logout、`actions.rs`は非同期操作のadmission/完了/エラー、`display.rs`は表示整形を持つ。generationによる古い応答の破棄、busyによる重複抑制、owner切替の初期化・初期設定の画面制限を維持する。
- `dashboard-ui/src/pages/`: `auth`・`shell`・`dashboard`・`hoyolab`・`accounts`・`rewards`・`security`・`admin`・`deletion`・`docs`に画面責務を分け、`common`にField/Metricなどを置く。Signals/Hooks・イベント・状態共有は既存のまま。`api.rs`・`models.rs`・`webauthn.rs`はHTTP・DTO・ブラウザ認証器境界を持ち、localStorage secrets、manual Cookie/Origin header、SSR/fullstack serverは持たない。
- `dashboard-ui/src/components/`: official Dioxus Components由来Button/Card/Card partsとMIT/provenance。
- `src/dashboard/http/`: route composition、DTO、Origin/session boundary、safe error mapping、compiled Dioxus UI、`/docs` index、`/openapi.json`。
- `src/dashboard/auth/`: user lifecycle、password/session、passkey registration/authentication、deletion request。
- `src/dashboard/hoyolab/`: owner operation lock、proof-gated verification/sync。`credentials.rs`がCredentials/CookieKeyの窓口を持ち、`credentials/parser.rs`がCookie解析・正規化、`credentials/crypto.rs`がAES-256-GCM保存を担当する。Cookie claimは本人確認の証拠にせず、暗号化v1のnonce・AAD・保存形式は維持する。
- `src/dashboard/games/`: owned game UID list、visibility、curated status cache。
- `src/dashboard/rewards/service.rs`: settings・manual enqueue・historyと共通のactor/link/credential version再検査。`service/cache.rs`がrefresh・TTL・cache upsertを持つ。認可再検査はcache専用ではないためserviceに残す。
- `src/dashboard/rewards/service/worker/`: serviceの子workerを維持し、`scheduler.rs`にtick・due schedule・history cleanup、`attempt.rs`にprepare・POST・GET確認、`state.rs`にdurable may-have-sent・guard・完了transaction、`deletion.rs`に削除時のGET-only確認を置く。owner operation lock、DB transaction/row lock、POST前のdurable記録と不確定時の再送禁止は移動前と同じ境界を保つ。fixed-offset計算は`rewards/schedule.rs`。
- `src/hoyolab/client/`: typed upstream protocol client。Dashboard policyをここへ混ぜない。

APIとschedulerは同じDB/業務処理を使う。単一process/instance前提で、Redisや分散queueはない。更新時down timeは許容する。

### 1.1. DioxusとAPI/UI境界

現構成はDioxus Webのclient-side rendering + Axum REST APIであり、別origin・別deployの「完全分離」ではない。同じprocessがcompiled UIとAPIを同じoriginで配信する。Dioxusの[Resource公式API例](https://docs.rs/dioxus/0.7.10/dioxus/prelude/struct.Resource.html)も通常のHTTP取得を扱っており、この構成はDioxusに反しない。ただし「同一RustのUI専用backendで常に最良」とはしない。

[Dioxus 0.7 Fullstackのserver functions](https://dioxuslabs.com/learn/0.7/essentials/fullstack/server_functions/)はAxum互換endpointと型付きclient呼び出しを生成する。新規のUI専用Rustアプリなら、手書きURL/JSON transportとDTOの二重管理を減らす有力な選択である。業務serviceやAxumを捨てる必要はない。一方、認証・Origin/Cookie・fresh authorization・mutation single-flight・owner切替・safe error・POST不確実性の処理は自動的には代替されない。

現時点ではSSR/SEO要件はなく、既存REST/OpenAPIと安全境界を維持してWeb構成を継続する。server functionsへの全面移行や新しい共有contract crateは今回導入しない。共有型を検討する場合も、backend entityをWASMへ持ち込まず、HTTP DTOのserde flatten/default/nullability・日時表現とUIのcurated型の差を確認する。全response modelが同じ宣言の単純重複ではない。

### 1.2. 既存ライブラリによる簡素化

- SeaORMの明示的`ON CONFLICT`でgame/reward cacheとrole保存の事前SELECT・insert/update分岐を削除。cacheはnullableを含む6列を更新し、roleは検出flag/時刻のみ更新する。既存ID・visibility、失敗時payload保持、transaction/owner lockは維持する。
- 既存lock内の`derive_more 2.1.1`をrootの直接依存として`debug` featureのみ宣言し、11 entityの手書きDebugをstatic container formatへ置換。表示fieldのallowlist方式であり、新fieldを自動公開するdefault Debug/secret denylistではない。compact/pretty両形式を検証する。pretty表示はflatとなる。生成ActiveModelは引き続きログ禁止。
- Randの`Alphabetic.sample_string`でDS用6文字ASCII nonceを生成。web-sysのtyped credential gettersを利用し、独自reflectionを削除。action終了処理は既存の同期admission/root taskへ集約し、docs取得はDocsPageが単独所有する。
- Cookie保存・admin user作成はclientが成功2xxをackとして扱い、未使用の成功JSONをdecodeしない。後続の型付きmetadata/user list取得を正本とする。backend/OpenAPIのJSON応答契約は変更しない。成功bodyの検証・消費・size capを行うという主張はしない。
- Cookie parserをCookieJarへ、native WebAuthn bridgeを`webauthn-rs-proto 0.5.5`のWASM変換へ置換しない。前者は重複Cookieの証拠・空白等の現policyを保持できず、後者はextension console logging、null userHandle、transports消失に相違がある。認証/session/cron/repositoryの新frameworkも追加しない。

## 2. HTTP境界

- Protected routeはsession cookieをDB上のhashへ照合する。HTTPS時のcookie名は `__Host-attos_session`、localhost非secure時は `attos_session`。
- Safe method以外は `Origin` headerが `PUBLIC_ORIGIN` と完全一致する必要がある。
- Responseには `Cache-Control: no-store` と `X-Content-Type-Options: nosniff` を付与する。Dioxus HTML shellにはCSPも付与する。DX 0.7.10のexternal module loaderを許可し、`script-src 'self' 'wasm-unsafe-eval'`、`style-src 'self'`、`connect-src 'self'`、`object-src 'none'`、`frame-ancestors 'none'`、`base-uri 'none'` を使う。`'unsafe-inline'` と一般の `'unsafe-eval'` は使わない。release bundleのCSP・WASM動作はChromiumと合成fixtureで検証済み。
- Body limitは16KiB。JSON DTOは `serde(deny_unknown_fields)` を使う。
- Error DTOは `{ "error": code, "message": sanitized }`。上流message、driver source、秘密値は返さない。

## 3. データモデルと保持

| Entity | 役割 |
|---|---|
| `users` | username, admin flag, active, must_change_password, password hash |
| `sessions` | token hash primary key, fixed expiry |
| `passkeys` | WebAuthn credential JSON, credential id, label |
| `audit` | lifecycle audit; secret/game dataは含めない |
| `hoyolab_accounts` | owner 1:1、nullable verified UID unique、encrypted credentials、credential_version、auth_status |
| `game_accounts` | link配下のgame_kind/game_uid/region、visible、detected、last_detected_at |
| `game_status_cache` | UIDごとのlatest curated payload version 1、fetched/error timestamps |
| `reward_settings` | link × game、enabled、local_time、auth_status、next_run_at |
| `reward_runs` / `reward_attempts` | durable reward execution and attempts, 5-day history |
| `reward_guard` | POST may-have-sent durable guard |
| `deletion_requests` | inactive target deletion completion queue |

CookieはAES-256-GCMで保存し、AADに用途/version/owner/verified UID有無と値を含める。keyはDB外の `HOYOLAB_COOKIE_KEY`。key rotation/re-encryption toolingは未実装。

## 4. HoYoLAB proof gate

`CookieStore` には二つの本番gateがある。

| Gate | 現在 | 理由 |
|---|---|---|
| owner binding | hardcoded false | account/community identity候補がtoken-to-UID bindingとroles principal一致の本番証拠ではない |
| reward contract | hardcoded false | 実POSTの期間・確認・再送安全性が未承認 |

実装済みの確認処理は、v2 ltoken credential、fresh account identity、fresh community identity、Cookie claimed UID一致、既存UID一致、roles validation、credential version再確認、same-UID update preservationを要求する。これらはmockで検証済みだが、本番gate解除の証拠ではない。

## 5. Owner operation ordering

```mermaid
sequenceDiagram
    autonumber
    participant R as Request or Worker
    participant O as Owner operation lock
    participant D as PostgreSQL
    participant H as HoYoLAB Gateway
    R->>O: acquire owner lock
    R->>D: re-authenticate actor / load link version
    alt proof gate false or setup incomplete
        D-->>R: 503 or initial_setup_required
    else live operation allowed
        R->>H: bounded GET/POST/GET as required
        R->>D: recheck actor/link/version before storing
        D-->>R: persist sanitized status
    end
    R->>O: release lock
```

Lockはowner単位で、別ユーザーはglobal serializeしない。DB table lockはbootstrap empty-table check以外に長時間使わず、admin lifecycleはPostgreSQL advisory transaction lockで短く保護する。パスワードCPU処理はSQL transaction外で行う。

## 6. Game status cache

- `Games::status` は ready verified link と owned game account を要求する。
- hidden/undetectedならfetchせずcache projectionを返す。
- fresh cache条件は payload version 1、payloadあり、`0 <= now - fetched_at <= 5min`。
- upstream successではcurated summaryだけを保存する。failureでは旧payload/fetched_atを保持し、error_code/error_atを更新する。
- retcode 10102相当は該当game reward settingを `action_required` にし、cookie invalid classはlinkを `cookie_invalid` にする。

## 7. Reward worker

```mermaid
stateDiagram-v2
    [*] --> pending
    pending --> running
    running --> already_claimed: preGET is_sign
    running --> success: POST then finalGET confirms
    running --> retry_wait: transient preGET before POST
    retry_wait --> running: max 4 attempts total
    running --> unknown: may-have-sent cannot confirm
    running --> action_required: upstream verification required
    running --> failed: retry limit/nontransient prePOST
    running --> stopped: disabled/version/auth mismatch before POST
    already_claimed --> [*]
    success --> [*]
    unknown --> [*]
    action_required --> [*]
    failed --> [*]
    stopped --> [*]
```

Worker tick is independent of page views. It first cleans completed 5-day history while preserving interrupted active work for confirmation, enqueues due enabled ready settings, then processes pending/running/retry_wait runs up to a small batch. `mark_may_have_sent` persists the guard before POST. Unknown/may-have-sent paths never unconditional POST retry; they use GET-only confirmation and otherwise remain unknown. In production the worker exits early because verification and reward gates are false.

## 8. Deletion workflow

Deletion request verifies actor password, reauthenticates current session, locks actor/target consistently, rejects last active admin, marks target inactive, revokes sessions/passkeys, persists `deletion_request`, and records audit. Background completion waits for owner operations, confirms may-have-sent reward runs by bounded GET-only logic, then deletes target-owned data and user row. Target audit rows are removed; other rows where target was actor may be preserved with actor set null.

## 9. API contract

Manual OpenAPI 3.1 is maintained at [`../dashboard-api.json`](../dashboard-api.json). It documents Dashboard routes: auth, passkeys, admin/me, HoYoLAB status/cookie/verify/sync, game accounts/status, reward settings/runs, health, Dioxus UI index, read-only `/docs`, and `/openapi.json`. The compiled `/assets/{...}` subtree is served from `DASHBOARD_UI_DIR/assets` for browser-internal hashed assets only and is not an OpenAPI operation.
