# AttOsGameDashboard 実装計画・検証課題

## 1. 現在の実装状態

この文書は実装済み範囲、未解消の上流実証、検証gapを追跡する。実アカウントGETやreward POSTの承認ではない。

| 領域 | 状態 |
|---|---|
| 基盤 | PostgreSQL migration、CLI `serve`/`migrate`/`bootstrap-admin`、安全な設定読み込み |
| 認証 | password login/logout、固定期限session、password rotation、管理者user管理、passkey login/registration/list/delete |
| UI | Dioxus 0.7.10 Web/WASM RSX。standalone `dashboard-ui` / `attos-dashboard-ui` crate、official Dioxus Components由来Button/Cardをsource同梱 |
| HoYoLAB Cookie | legacy/v2 parse normalization、AES-GCM保存、pending status、same-UID proof path実装 |
| Proof-gated verification | account/community identity候補、role sync、version recheck、same-UID update preservationを実装・mock検証済み。本番gate false |
| Game status | owned UID list/visibility、curated cache version 1、5分TTL、failure keeps old payload |
| Rewards | settings/list/update/manual run/history、worker、durable may-have-sent guard、GET/POST/GET flowを実装。本番reward gate false |
| Deletion | self/admin-delete-other、actor password、typed username、last active admin保護、inactive+revoke+background completion |
| OpenAPI/operations docs | manual OpenAPI 3.1 と運用手順を追加 |

`/` と `/docs` は同じcompiled Dioxus `index.html` を返し、`/docs` は認証なしread-only OpenAPI viewerとして動作する。`/openapi.json` はstandalone contract。`/assets/{...}` は `DASHBOARD_UI_DIR/assets` のcompiled hash asset専用で、旧native `/assets/app.js`・`/assets/style.css`・`/assets/docs.js` はAPI pathではない。

## 2. 本番rollout blocker

最重要blockerは実装量ではなく、HoYoLAB上流契約の証拠不足である。

- `account_identity()` と `community_identity()` は候補GETとして実装されているが、返却UIDがCookie token本人に拘束されることを証明していない。
- roles APIが同じprincipalに紐づくことも本番では未証明。
- 実アカウント1個では token B + claim UID A gap を閉じられない。
- reward POST contract（期間、反映遅延、final GETだけでの安全確認、unknown後の再送禁止条件）は未承認。
- そのため `OWNER_BINDING_ESTABLISHED=false` と reward contract gate false を保つ。設定やGUIでoverrideしない。

この状態ではreal account onboarding completionもreward postingもできない。UI onboardingの完成は、本番proof gate解除なしには実運用可能とは言えない。

## 3. 実証が必要なこと

| 対象 | 必要な証拠 |
|---|---|
| owner binding | fresh account identityとcommunity identityがtoken本人UIDを返すこと。Cookie claimや第三者profileでないこと |
| roles principal | identityのUIDとrolesの認証主体が同じであること |
| Cookie bundle | v2にMIDが必須か、legacyで検証可能か、2項目で十分か。必要情報を勝手に落とさない |
| UID collision | 別ユーザーの既存UIDを開示せず拒否できること |
| reward POST | GET未受取→POST→final GETの成功条件、追加認証、反映遅延、unknown時の安全停止 |
| reset boundary | 日次期間・月・`act_id`・timezoneが冪等keyとして使えるか。JST日付だけに依存しない |
| game record | 欠落・非公開・retcode 10102・cookie invalidの実応答と停止範囲 |
| browser/security | real authenticator WebAuthn、production TLS/reverse proxy、secure cookie prefix behavior |

実通信は別途承認が必要。受け取りPOSTは特に副作用があるため、read-only確認と分けて承認する。

## 4. 残る実装・運用課題

- OpenAPI viewerはDioxus RSX read-only pageとして維持する。try-it mutation、CDN、Swagger UI依存、native JS fallbackは追加しない。
- Cookie key rotation/re-encryption toolingは未実装。運用手順では「変更しない」を契約にする。
- Single-process前提。HA、multi-instance scheduling、distributed lockは未設計。
- docs examplesにsecret-like token/password/cookieを入れない。
- 実認証器、production TLS/reverse proxy、実配置path、live HoYoLAB契約は未検証。合成fixtureの成功で本番proof gateを解除しない。

## 5. 検証方針

ローカル合成UID、mock gateway、software passkey、isolated PostgreSQLを使う。production DB、real Cookie、real reward POSTは使わない。関連する確認コマンドの例:

```console
cargo fmt --all -- --check
rustup target add wasm32-unknown-unknown
cargo check --locked
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=2
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo check --locked -p attos-dashboard-ui --target wasm32-unknown-unknown
cargo clippy --locked -p attos-dashboard-ui --target wasm32-unknown-unknown -- -D warnings
dx build --package attos-dashboard-ui --platform web --release
node dashboard-ui/tests/browser.mjs
node -e "const fs=require('fs'); const api=JSON.parse(fs.readFileSync('docs/dashboard-api.json','utf8')); console.log(Object.keys(api.paths).length, Object.values(api.paths).reduce((n,p)=>n+Object.keys(p).length,0))"
```

最終ローカル検証（モジュール分割後）: workspace 184 tests passed / 1 ignored（未承認のlive GETのみ）。native/WASM Clippy `-D warnings`、backend build、DX 0.7.10 release buildは成功。public出力は `target/dx/attos-dashboard-ui/release/web/public` を確認済み。Chromiumで実WASM bundleとexternal module loader用CSP（`wasm-unsafe-eval`のみ、inline/一般evalなし）を検証した。ローカル合成API fixtureで初期設定、管理・本人削除へのアクセス、Cookie入力消去、virtual WebAuthn登録/assertion、未使用のmutation成功bodyをdecodeしないack処理、public docsのOpenAPI GETが1回・認証API呼び出しなし、UID filtering、同時認証の拒否・HttpOnly Cookieと表示owner一致、logout失敗の警告/reload/retry、mobile docsを確認。Playwright/Chromiumは別途必要で、不足時はfailする。実認証器やbackend認可、上流token bindingの証明ではない。highレビューの3指摘（認証single-flight、派遣durationのstring/integer両対応、logout失敗の正確な表示）を修正済み。JSON/OpenAPI/Markdown/Mermaidの確認も行う。旧native Node UI checksを新UIの検証に流用しない。

既存ライブラリ簡素化を実施済み: SeaORM cache/role upsert（事前SELECT削除）、derive_moreのstatic allowlist Debug、Rand/web-sys typed API、action終了処理とdocs取得ownerの集約。cache nullable overwrite/retention、role ID/visibility・消失/再出現、全11 entityのcompact/pretty redactionを回帰検証した。独立highレビューで具体的な残存regressionは見つからなかった。API/UI構成の評価と見送った置換は[設計 §1.1–1.2](../design/AttOsGameDashboard.md#11-dioxusとapiui境界)に記録する。

### 5.1. 責務別モジュール分割の検証記録

事前調査の行数と実装開始時の対象は一致していた。処理本体を移動し、参照・imports・必要最小限のvisibilityを調整した。API仕様・DBスキーマ・migration・本番proof/reward gateは変更していない。旧パス互換re-exportや新しい状態管理抽象化は導入していない。

| 対象 | 分割前 → 分割後（行） | 抽出先 |
|---|---|---|
| UI pages/mod.rs | 918 → 11 | auth 153、shell 127、dashboard 152、hoyolab 71、accounts 70、rewards 124、security 91、admin 101、deletion 48、docs 75、common 11 |
| UI app.rs | 850 → 72 | state 314、data 245、session 235、actions 81、display 50 |
| HoYoLAB credentials.rs | 352 → 24 | parser 216、crypto 124 |
| credentials_tests.rs | 261 → 旧ファイル削除 | parser/tests 118、crypto/tests 159（独立した合成v1暗号文の互換テスト1件追加） |
| rewards/service.rs | 395 → 294 | service/cache 123、共通認可recheckはserviceに維持 |
| rewards/worker.rs | 614 → 旧ファイル削除 | service/worker/mod 10、scheduler 113、attempt 229、state 247、deletion 87 |
| rewards/service_tests.rs | 838 → 158 | service/cache_tests 198、worker/execution_tests 315、共通test_support 199 |
| rewards/worker_tests.rs | 334 → 旧ファイル削除 | service/worker/tests 343 |
| auth/accounts_tests.rs | 711 → 478 | accounts_tests/password_rotation 248 |
| auth/deletion_tests.rs | 741 → 451 | deletion_tests/completion 304 |

UI・credentials・rewards・authテスト整理を独立したread-only highレビューで確認した。既存関数・テスト本体の比較で意図しない挙動/assertion変更は見つからず、generation/busy・owner切替・logout失敗・初期設定制限・60秒更新、暗号化v1/AAD、owner/DBロック・durable送信状態・不確定POST再送禁止を維持する。UIにはactionの古いgeneration拒否と表示整形のfocused testを追加した。

検証: fmt、workspace all-target check/test/Clippy（`-D warnings`）、UI WASM check/Clippy、Dioxus 0.7.10 release build、Chromium browser fixtureは成功。DBは既存TestDatabaseのisolated PostgreSQL 18.4一時クラスタを使用し、production DATABASE_URLはunsetした。Dioxus CLIとPlaywrightはPATH/通常module解決外の既存インストールを明示指定し、Chromium共有ライブラリは既存Nixライブラリを実行環境だけで指定した。新しい依存関係・生成物は追加していない。60秒timerの実時間ブラウザ検証はなく、既存実装の構造比較による確認に留まる。

見送り: auth/accounts.rsはlifecycle/session/audit transactionと共通認可、games/service.rsはowner lock下のstatus/cache/error効果が密結合のため維持。HoYoLAB client/mod.rs・client/test_support.rsとUI webauthn.rsはそれぞれtransport・TLS mock server・ブラウザadapterとしてまとまっている。games/service_tests.rsはfixture共有の増加に見合う独立責務がなく、db/game_tests.rsも今回の業務分割と独立したschema/migration検証として保持する。api.rs・models.rs・verification.rs・既存migrationは指定どおり維持する。

## 6. 合意済み方針

- 1 user : 1 HoYoLAB account。追加連携・別UID置換・independent unlinkなし。
- 削除はDashboard user deletion。selfとadmin-delete-otherを許可し、actor current passwordとtyped target usernameを要求する。
- 最後のactive adminは削除不可。inactive adminは別active adminが削除可能。
- Disable blocks new access/jobs and revokes sessions。Re-enable never revives old sessions and turns automatic rewards OFF。
- Reward defaults: OFF、06:00、shared fixed +09:00、history 5 days、initial + 3 retries max、unknown no unconditional POST retry。
- Game cache latest only、version 1、TTL 5min、failure keeps old payload/fetched_at and records separate sanitized error/error_at。
- Secrets are never logged/returned。Connection subscriber output is redacted; passkey library traces are suppressed around sensitive work。
