# AttOsGameDashboard 実装計画・検証課題

[機能仕様](../spec/AttOsGameDashboard.md)と[技術設計](../design/AttOsGameDashboard.md)に対する作業計画。実装順序・調査状況・合意記録・残る未確定事項を管理し、要件の正本とは分ける。

## 1. 初期リリースの実装順序

ユーザーはgrilling後の最終合意を承認し、文書への反映と**GUI以外の実装着手**を指示した。基盤・認証のバックエンドを実装し、次は初回Cookie設定・1ユーザー1連携・Dashboardユーザー削除を含むHoYoLAB連携へ進む。GUIは今回の対象外。本計画全体を完了扱いにしない。新ライフサイクルと責務別テスト追加の承認は1.3節に記録する。

`src/main.rs`は`serve`・`migrate`・`bootstrap-admin`を提供し、空の`/openapi.json`・`/docs`は除去した。既存HoYoLABクライアントのエラーは型付き分類へ移行済みだが、連携の所有権検証や受け取りスケジューラーは未実装。

| フェーズ | 作業対象 | 受入観点 | 状態 |
|---|---|---|---|
| 1. 基盤・認証 | SeaORM＋PostgreSQL、`sea-orm-migration`、初期管理者CLI、直接作成、必須PW・任意パスキー、固定期限セッション、復旧・無効化 | 招待／自由登録なし、仮PW変更前の通常利用禁止、PW削除なし、7日固定期限、全セッション失効、復旧時パスキー削除・監査、所有権・CSRF | バックエンド実装済み。実端末WebAuthn・GUIは未確認 |
| 2. 初期設定・HoYoLAB連携・ユーザー削除 | 初回登録／仮登録後の初回ログインでCookie設定、検証・暗号化、UID検出・同期、同一UID更新・ユーザー削除の順序化 | PW変更＋本人UID検証完了前は通常利用不可、1ユーザー1連携・owner unique＋全ユーザーUID unique、異UID置換／追加連携／個別解除なし、新UID非表示・未検出取得なし・選択保持、全ユーザーデータ削除・一意制約解放、削除後は新規ユーザー／旧データ復元なし／全OFF、平文返却なし | 未実装。actor／最後の管理者保護・初期設定／削除API契約は未確定 |
| 3. 受け取り・状態取得 | ゲーム別設定、スケジューラー、GET→POST→GET、5日履歴、再試行、キャッシュ最新1件、型付き業務結果 | 初期OFF・06:00・共通UTC+09:00、手動独立、ON／更新直後は次回待機、catchup現在分のみ、追加最大3回、不明POST再送なし、停止範囲と確認記録の維持 | 未実装 |
| 4. Dashboard | Dioxus、スマホ・デスクトップWeb、主要ゲーム状態、古さ・エラー・予測表示 | 初期表示範囲だけ、推定不能値を増やさない、UID表示と受け取り独立、通知Dashboardのみ | 未実装・今回の実装着手からGUIを除外 |
| 5. 契約・配布・運用 | WebセッションAPIのOpenAPI、単一稼働・更新停止、OCI・NixOS向け運用案 | 外部API／PAT／Scope／別認証なし、所有権、停止後の安全な復旧、保持・削除・運用設定 | 未実装（契約・運用確認は各フェーズでも行う） |

受入観点と実施済み検証は区別する。従来の「新規テストファイルを追加せず一時環境で確認」は過去の方針であり、現在はユーザーの明示承認により各責務のmodule単位テストを新規追加する（1.3節）。実アカウントへのアクセスは読み取りも事前承認を要し、この承認には含まれない。

### 1.1. 基盤・認証の実装・検証記録

- SeaORM＋PostgreSQLのユーザー・セッション・監査・パスキーmigration、初期管理者CLI、直接作成／無効化／再有効化／復旧、Argon2id、固定期限セッション、パスキー登録・一覧・削除・ログインを実装。復旧は同一トランザクションで全セッション／キー失効と監査を行う。
- HTTPはOrigin完全一致・HttpOnly/SameSite・HTTPSの`__Host-`Cookie・no-store・入力上限を共用。ログイン入口は全体20リクエスト/分、暗号処理は待機列を増やさず実行枠満杯時503。登録challengeは元セッション・ownerに拘束し、全challengeは一回使用・5分・最大128件。ユーザーごとに最大20鍵。WebAuthnのため公開OriginはDNS名または`localhost`を使う。
- 初回の一時PostgreSQL／HTTP検証26項目、期限・並行パスワード変更・過負荷などの修正後検証18項目が成功。パスキー接続後は一時PostgreSQLと新規生成P256鍵のソフトウェアtranscriptで20項目が成功（登録・署名検証・再使用拒否・他人の削除拒否・復旧・旧challenge／セッション拒否・資格情報非返却）。追加11項目で別セッションの登録完了拒否、counter変更後の旧challenge拒否、同一資格情報を削除・再登録しても旧challengeを復活させないこと、署名済みでも別Originを拒否することを確認。DNS／localhost限定Origin設定の不正入力4項目も成功。環境は削除済み。
- 高思考の読み取り専用レビューでDB接続／エラー診断の秘密漏洩と期限・整合性・CPU待機列の指摘を修正。認証4修正の最終再確認・配線後パスキー・HoYoLABの型付きエラー移行のレビューには修正を要する指摘なし。
- 配線後の`cargo fmt --all -- --check`、`cargo clippy --locked --all-targets -- -D warnings`、`cargo build --locked --bins --examples`、`cargo test --locked --all-targets`が成功。既存テストはライブラリ3件＋example1件。新パスキーの実ブラウザー／物理認証器・5分経過後の実時間失効・運用TLS経路は未検証。ソフトウェア検証を実端末の確認とは扱わない。
- Cookie初期設定・本人UID検証／通常利用ゲート・Dashboardユーザー削除・HoYoLAB連携・受け取り・キャッシュ・再有効化時の自動受け取りOFF・OpenAPIは未実装。PW基盤・現パスキー機能の完了は新ライフサイクルの完了ではない。無効化時の新規ジョブ／再試行停止も対応モジュールと併せて実装する。

### 1.2. 責務別構造の整理

- ユーザーの追加指示に従い`auth/`・`auth/passkeys/`・`http/`・`db/entities/`・`db/migrations/`へ分割し、CLIを`cli.rs`へ移した。[モジュール地図](../../README.md#dashboardのモジュール地図)を参照。認証・DB側の57個の関数本体／型／entity／migrationを原文と機械比較し、可視性表記と空白以外を保持した。
- auth／DBとHTTP／CLIの二つの独立highレビューで、責務・公開範囲・認可・Cookie・トランザクション接合の修正を要する指摘なし。元のmigration名・順序・ルート契約を維持し、構造整理後の同じ一時PostgreSQL＋ソフトウェアP256検証31項目が再度成功した。
- `AGENTS.md`に増分ごとのhighレビュー、責務分割・狭い可視性・認可の共用・秘密非参照・モジュール地図更新を記載。旧パス互換の代理モジュール、汎用repositoryや単一実装interface、新規テストは当時追加していない。これは構造整理時点の履歴であり、現在のテスト追加禁止ではない。

### 1.3. 今回承認されたライフサイクル・テスト方針

- 「アカウント削除」はDashboardユーザー削除。1ユーザー1HoYoLABアカウントへ変更し、独立したunlink service／API／GUI・複数連携選択・異UIDを別連携として追加する経路を対象外にした。同一UIDのCookie更新は維持する。
- Cookieは初回登録時、仮登録ユーザーは初回ログインの初期設定時に入力する。PW変更とCookie設定／本人UID検証が完了するまで通常利用を禁止する。PW復旧は既存HoYoLAB UIDを変更する手段にしない。招待／自由登録禁止・既存の役割は変更しない。
- Cookie更新とユーザー削除を同じ連携通信・必要な結果確認／記録の終了待ちで順序化する。削除受付後は新規実行・再試行・未送信操作を止め、送信済み結果を必要なら記録した後、その記録を含め全ユーザーデータを削除する。再試行遅延まで待たない。
- 不可逆確認後にユーザー本体・Cookie・本人UID・ゲームUID・settings・cache・run／attempt履歴・PW・セッション・パスキー・challenge等を削除し、UID一意制約を解放する。削除後は新規ユーザーとして登録し、旧データ復元なし、表示／自動受け取り全OFF。削除actor／本人・管理者の権限／最後の管理者保護・登録時画面と親リソース／metadata・API契約は未確定のまま残す。
- 現在の利用可能な実HoYoLABアカウントは1個のみ。テストはローカル合成UID A/B・mock上流・ソフトウェアP256・隔離した専用一時PostgreSQLを使い、production DB／実Cookieは使わない。合成複数UIDは実APIのtoken-to-UID bindingを証明しない。
- 各責務（設定、永続化、アカウントライフサイクル、PW、セッション、パスキー、HTTP境界、上流プロトコル）のmodule近傍にテストを新規追加する方針を今回明示承認。既存認証の回帰と新仕様の受入テストを分け、1:1制約・同一UID更新／異UID拒否・未完了ゲート・復旧でのUID維持・削除競合／完全削除／新規全OFFを検証対象にする。未確定の権限は決定後にテスト契約へ反映する。
- 最新の責務別テスト86件（library 6＋Dashboard 78＋CLI実行入口2）が成功。実HoYoLAB GETテスト1件は明示承認を要するため通常はignoredで、今回は未実行。設定はプロセス環境を書き換えず同じparserへ入力し、DB制約／migration／アカウント／セッション／合成P256 ceremony／HTTPの本番ルート合成を確認する。fmt・clippy（警告拒否）・全targetテスト・bin buildが成功し、独立highレビューでblocking指摘なし。Cookie backend・初期設定／通常利用ゲート・ユーザー削除は依然未実装で、これらの結果を新ライフサイクル・実上流token bindingの完了証拠にしない。
- 確認用CLIは一度`examples/`から`src/bin/`へ移した後、追加指示により廃止して`src/hoyolab/client/live_tests.rs`へ集約した。テストは所有moduleへ置き、API関連は現在実装済みのエンドポイントだけを対象とする。架空の`/record`・`/daily`・`/redirect`・`/large`モックを撤去し、現行のHTTPSホスト／パス／query／method／ヘッダーをループバックTLSモックで検証する。受け取りPOSTはローカルだけ、実API確認はGETだけとし、本人確認候補の未実装APIを先回りしてテストしない。以前の89件（診断bin・CLI help・架空path検証を含む）と1.1節の`--bins --examples`は過去の記録として区別する。作業単位commit・要求されたpushの方針は`AGENTS.md`へ記載した。

## 2. 実装前に実証すること

既存調査では有効Cookieによるinfo／recordの成功応答を取得できておらず、受け取りPOSTも行っていない。この文書整合作業でも実アカウントを使用していない。既存コード・資料だけで外部APIの実挙動を保証しない。

| 実証対象 | 確認・記録すること |
|---|---|
| Cookie・roles | 認証済みHoYoLAB本人UIDの検証方法、必要Cookie／MID契約、同一principalのroles認証、ゲームUID／region検出、失効の識別、同一UID更新。rolesのゲームUIDを本人証明にしない |
| 日課・受け取り状態 | `is_sign`、累積日数、報酬情報の意味・同期遅延。月や`act_id`だけで日次期間を保証しない |
| 受け取り操作 | GET未受取→POST→GET確認。retcode 0でも追加認証の可能性を調べ、`requires_verification()`の`None`／`false`を成功証拠にしない |
| リセット境界・期間 | ゲーム別の境界・時間情報・反映遅延を観測し、期間キーと安全な再確認を設計。JST日付の保証は置かない |
| 障害・追加認証・非公開 | Cookie失効／ゲーム固有追加認証／通信障害の分類と停止範囲、ユーザー対応後の再検証、非公開設定の実応答 |
| ゲームレコード | 原神・HSRの初期対象フィールド、回復・派遣時間の単位／意味、欠落時の扱い |
| 延期項目 | 変化器のネスト構造、週間項目の正式な表示名・計数・リセット境界。詳細実証まで初期表示に入れない |

受け取りPOSTは実アカウントに作用する。読み取り疎通と区別し、対象・副作用の承認後に行う。秘密Cookieや未加工の秘密応答は記録・共有しない。

根拠資料：

- [既存クライアント](../../src/hoyolab/client.rs)・[型付きエラー](../../src/hoyolab/error.rs)：通信・HTTP・retcode・形式を分類し、上流messageを保持しない。POST内部再試行なし。失効／追加認証／不明送信などの業務分類は未実装。
- [genshin.py参照実装](https://github.com/seriaati/genshin.py/blob/b635085ad32b1d366b97dcb276b719d1083a9ff4/genshin/client/components/hoyolab.py#L155-L175)：海外版`get_hoyolab_user(None)`は`/community/painter/wapi/user/full`をUID指定なしで呼び、`data.user_info.uid`を扱う。自己取得の意図は確認したが、海外版のtoken／Cookie UID不一致への耐性は未実証。Cookie内のUIDや第三者プロフィール取得だけを所有権の証明にしない。
- [受け取り型](../../src/hoyolab/models/daily.rs)・[日次受け取り資料](../hoyolab/daily-checkin.md)：追加認証、GET確認の必要性。リセット境界は未確認。
- [ゲームレコード共通](../hoyolab-game-record/common.md)：retcode解釈は変更され得る。欠落値だけで原因を確定しない。
- [原神資料](../hoyolab-game-record/genshin.md)・[HSR資料](../hoyolab-game-record/starrail.md)：回復時間の実証、変化器／週間項目の不足。HSR予備開拓力の数値上限・回復時刻を作らない。
- [アカウント一覧](../hoyolab/account-roles.md)・[認証資料](../hoyolab/authentication.md)・[HoYoLAB API資料](../hoyolab-api.md)・[リポジトリREADME](../../README.md)：既存調査・手順（本作業では参照のみ）。

### 2.1. HoYoLAB本人UIDの検証候補と実装ゲート

公開ソース調査のみを実施し、実HoYoLAB通信・実アカウント・秘密Cookieの参照は行っていない。以下は**候補プロトコルであり、確認済みの上流認証契約ではない**。本番の検証済みUID登録／同一UID Cookie更新は、tokenと返却UIDの拘束を確認するまで実装・有効化しない。ローカルmockによる契約・方針・基盤・責務別テストの実装は進められ、実アカウントが2個ないことを全作業停止条件にしない。

| 候補 | 公開実装で確認できた形 | 未確認事項 |
|---|---|---|
| account API | `GET https://api-account-os.hoyolab.com/auth/api/getUserAccountInfoByLToken`、query/bodyなし、`data.account_id`はnullable string | 返却IDがtoken本人に拘束されること、community UIDとのnamespace一致、海外版の現行可用性 |
| UIDなしprofile | `GET https://bbs-api-os.hoyolab.com/community/painter/wapi/user/full`、UID queryなし、`data.user_info.uid` | Cookie内のclaimed UIDだけで第三者を返さないこと、roles APIと同じ認証者になること |

account APIの[参照実装](https://github.com/RXDCODX/MarchSeven/blob/01ff9a9218f0ca9af7eddcd9f8d7c3f767e12e71/MarchSeven/Models/Core/EndPoints/EndPoints.cs#L11-L15)は`ltoken_v2`・`ltmid_v2`・`ltuid_v2`の[3項目bundle](https://github.com/RXDCODX/MarchSeven/blob/01ff9a9218f0ca9af7eddcd9f8d7c3f767e12e71/MarchSeven/Models/Core/Cookie/CookieV2.cs#L6-L25)を送る。[wrapper](https://github.com/RXDCODX/MarchSeven/blob/01ff9a9218f0ca9af7eddcd9f8d7c3f767e12e71/MarchSeven/Models/Core/Wrapper.cs#L23-L58)はapp_version `1.5.0`・client_type `5`・language・UAを設定し、この呼出しのDSはOFF。これをserver-requiredな最小条件とみなさず、2項目で足りるかMIDが必須かを勝手に決めない。返却[account_idモデル](https://github.com/RXDCODX/MarchSeven/blob/01ff9a9218f0ca9af7eddcd9f8d7c3f767e12e71/MarchSeven/Models/HoYoLab/UserAccountInfo/AccountInfoData.cs#L5-L11)を確認したが、参照wrapperのcase-sensitive retcode解析は流用せず、既存Rustの明示retcode・data検査を維持する。

連携業務の安全境界：

- 本人UIDは新Cookieのfresh・uncachedな上流identity応答だけから取得する。明示retcode=0、data、有効IDを要求し、旧UID／cache／Cookie claimへfallbackしない。`roles`のgame UID一覧だけではHoYoLAB本人UIDを証明できない。
- 全文貼付は入力方式であり、全文をそのまま所有権検証・保存・roles通信へ流す契約ではない。確認済みの単一ltoken bundleをcanonical化して共用し、重複auth key・相反UID/MID・legacy/v2混在・別のcookie_token認証familyを排除する。必要MIDを勝手に落とさない。正規化だけでtoken bindingが証明されるとは扱わない。
- 既存汎用クライアントはpair存在検査後もCookie全文を送る。連携業務向けのidentity証明機能はなく、公開API側の挙動を保つため調査だけで既存全文送信を一律変更しない。
- 拘束確認後にだけUID一意性・更新時の既存UID完全一致を適用する。ID／トークン／個人情報を上流messageや未加工応答ごと保存・ログ・返却しない。

本番ゲートを解除するには権威ある契約資料、または**別途承認された**実通信によるtoken→UID拘束の証拠が必要。現在は実アカウント1個のみで、承認前のread-only実API確認も行わない。ローカル合成A/Bでは有効／無Cookie／失効token、token B＋UID A、UID/MID alias不一致・重複順序、legacy A/v2 Bと逆、cookie_token A/ltoken B、2項目／3項目／全文の方針を検証できるが、実上流の拘束の証拠にはならない。

将来、制御可能な実2アカウントA/Bと別途承認が得られれば上記交差入力を比較し、不一致では本人Bまたは拒否となること、account IDとcommunity UIDが一致し同じcanonical bundleのrolesも本人Bに属することを確認する。**live 2アカウント交差検証は未解消の検証gap**であり、2個ないこと自体はローカル実装・テストの停止条件ではない。実値や未加工応答を記録せず、受け取りPOSTはこの確認に含めない。今回の承認・調査報告は実通信の許可ではない。

## 3. 未確定事項と要確認点

承認済みの選択を未確定へ戻さない。以下は本当に残る実証・契約・実装細部であり、仕様に反する機能追加の候補ではない。

| 項目 | 残る確認・決定 |
|---|---|
| 初回Cookie設定の契約 | 登録時／仮登録後の初回ログイン初期設定という時期は確定。画面・親リソース・必要metadata、入力方式と上流確認後の必要MID、限定設定API／通常利用ゲートの正確な経路・応答は未確定 |
| Dashboardユーザー削除の契約 | 全ユーザーデータの不可逆削除・順序化・UID解放は確定。actor（本人／管理者）、最後の管理者保護、確認画面・method／path、削除待ち状態・既存userロックとの取得順序、関連監査／challengeの削除実装は未確定。既存管理者権限から新しい削除権限を推定しない |
| WebセッションAPI契約 | JSON型、入力検証、日時・古さ・未取得／失敗の表現、ページネーション、HTTPエラー、レート制限、待機／実行要求の応答方式、WebAuthn登録手順 |
| 外部APIの実挙動 | リセット境界、非公開、追加認証の応答と解除確認、GETの反映遅延、欠落値の意味、ゲーム別取得可能範囲 |
| 期間識別・不明送信の安全性 | 実証に基づく期間キー、送信可能性の永続化、確認GETだけで過去POSTを否定できる条件、5日履歴削除後にも不明操作を再送しない安全性の保持方式 |
| 再試行の細部 | 初回＋最大3回と指数バックオフは確定。残るのは基準秒数・上限秒数、分類した一時障害ごとの安全な再試行条件 |
| キャッシュ | 最新1件は確定。TTLは5分を推奨するが未確定。JSON版変更時の破棄・再取得、キャッシュなしの応答細部 |
| DB・保持実装 | 5日保持の起算点・削除タイミング、カラム型・制約・索引・マイグレーション順序、未完了確認との整合。未検出UIDはユーザー削除まで選択を保持する方針自体は確定 |
| サーバー運用 | 固定オフセットや実行時刻を変更したときの次回計算細部、初期管理者CLI・仮PWの安全な受け渡し、暗号鍵更新、バックアップ・復元・単一稼働の手順 |

再試行上限、初期時刻、タイムゾーンの範囲、catchup方針、復旧方法、外部APIの要否、履歴保持期間、初期表示範囲は合意済みであり、この表で再検討しない。

## 4. 合意・レビュー解消記録

grilling後の最終合意をユーザーが「反映して」と承認した。以下は今回のライフサイクル変更も含む現行決定であり、推奨案・確認待ちではない。[仕様](../spec/AttOsGameDashboard.md)と[設計](../design/AttOsGameDashboard.md)へ反映した決定記録。

| 論点 | 決定・レビュー解消内容 |
|---|---|
| DB・構成 | SeaORM＋PostgreSQL、`sea-orm-migration`。単一アプリ稼働・更新停止許容、外部キュー不要、既存HoYoLABクライアント再利用 |
| 利用開始 | 小規模な身内用。招待／自由登録なし、管理者が仮PWで直接作成。Cookie設定は初回登録時、仮登録ユーザーは初回ログインの初期設定時。PW変更・Cookie設定／本人UID検証完了前は通常利用不可 |
| 認証・復旧 | PW保持必須・単独PW削除機能なし、パスキー任意。復旧は管理者の仮PW再設定のみ、全セッション／パスキー削除・操作記録、リンク／メールなし。新HoYoLAB UID選択は許可しない |
| 管理者境界 | 通常Web/APIで他人のゲームデータを直接閲覧不可。ただし復旧権限を持つ信頼主体であり、完全な管理者排除ではない |
| セッション | 成功ログインごとに初期7日の固定期限、期間はサーバー設定。アクセス・別端末ログインで延長せず、設定変更は今後のログインのみ。ログアウト／無効化／復旧で失効 |
| 無効化・再有効化 | Web・新規実行・再試行禁止、データ保持。送信済みは確認記録まで継続。再有効化は旧セッション復活なし、自動全OFF、他設定・履歴・通常PW・パスキー保持 |
| 連携単位・Cookie同一性 | 1ユーザー1HoYoLABアカウント。owner uniqueと全ユーザーHoYoLAB UID unique。更新は同一UIDのみ、異UID置換・別追加連携を拒否し設定・履歴を維持 |
| アカウント削除・新規登録 | Dashboardユーザー削除のみで連携も削除、独立unlinkなし。不可逆確認後にユーザー本体・Cookie／本人UID／ゲームUID／設定／cache／実行・試行履歴／PW／セッション／キー等全ユーザーデータ削除・UID解放。他ユーザーも登録可能。削除後は新規ユーザー／旧データ復元なし／全OFF。actor・最後の管理者保護の詳細は未確定 |
| UIDライフサイクル | 新UID非表示。未検出はホームから外し管理で保持・取得停止。表示選択を保持し再検出で復元。受け取りはHoYoLABアカウント×ゲームでUID表示から独立 |
| 操作の競合 | Cookie更新・ユーザー削除を実行中だけで拒否しない。同じ連携の通信・必要な結果確認／記録を待つが再試行遅延には待たせず、別ユーザーを妨げない。削除待ちは新規実行・再試行禁止、送信済み確認／記録後に全削除 |
| スケジュール | ユーザーがゲーム別ON/OFF・時刻（初期06:00）を設定。タイムゾーンだけサーバー共通固定オフセット（初期UTC+09:00）、DSTなし。ON／Cookie更新直後は次回時刻まで待機、即時なら手動 |
| 自動・手動の関係 | 初期自動OFF、手動はON/OFFと独立。元資料の図が手動にもOFFを適用するように読めた矛盾は、仕様・シーケンス図とも自動要求だけに限定して解消 |
| 停止後の復旧 | 時刻を逃した場合は現在受け取れる分だけcatchup、過去補填なし。不明処理の無条件再送には使わない |
| 再試行・認証状態 | 初回とは別に最大3回、指数バックオフ。POST不明はGET確認優先・確認不能なら不明で打切り表示、無条件再送なし。Cookie失効はHoYoLAB全体、追加認証は該当ゲーム、通信障害は当該処理。認証状態と実行状態を分離 |
| 期間・実証の限界 | リセット／非公開／追加認証の実挙動は未検証。JST日付による期間キーの保証を置かない |
| 保持・表示 | 実行／試行履歴5日、キャッシュ最新1件。原神は樹脂・予測・派遣・洞天宝銭・デイリー任務、HSRは開拓力・予測・予備開拓力・依頼・デイリー訓練、両ゲーム報酬状態。変化器／週間項目は詳細実証まで延期 |
| UI・通知 | スマホ・デスクトップWeb、日時・古さ・未取得・失効・通信等を区別。推定不能数値を増やさない。通知Dashboardのみ |
| API・将来対応 | PAT／Scope／外部クライアントAPIは不要、WebセッションAPIのみ・別認証なし。スマホアプリ希望は記録するが初期ネイティブなし、UI／業務処理分離のみ維持 |
| セキュリティ | Cookie平文返却・秘密ログ禁止、暗号化保存、Argon2id／WebAuthn／CSRF／所有権検証を維持。Cookie claim／rolesは本人UIDの証明にしない |
| テスト方針の更新 | 過去の新規test非追加記録とは区別し、現在は責務別moduleテストの新規追加を明示承認。合成UID A/B・ソフトP256・専用一時PGを利用。実アカウント1個、未承認の実APIアクセスなし、live 2アカウント交差は検証gap |

所有者境界と管理単位、共通業務処理は維持する。解消済みの保守性・拡張性レビュー論点はこの記録に移し、未実証の外部挙動・契約細部だけを3章に残した。その他の初期版対象外は仕様9章を維持する。

## 5. 文書検証と原資料

文書更新の確認対象は本文・表・5つのMermaid図・ローカル相対リンク／見出しアンカー・Markdownコードフェンス・空白差分。**以前の更新時**にはMermaid全5図の構文検証（品質指摘なし）、README／docs索引を参照範囲に含む5文書・36ローカルリンクの存在／アンカー検査、フェンス・編集文書の末尾空白検査、`git diff --check`を通過した。これは今回のライフサイクル変更前の履歴である。**今回のライフサイクル変更後**もMermaid全5図を`validate`のみで構文検証し、品質指摘なし。3文書の28ローカル相対リンク／見出しアンカー、コードフェンス、末尾空白を検査して問題なし。編集3文書を指定した`git diff --check`も成功した（文書は未追跡のため、直接の全文検査を併用）。persistent diagramの作成・他ファイル編集・commit／pushは行っていない。

構文検証とリンク検査は外部APIの実証、GUI描画レビュー、実装の動作確認を代替しない。レンダリングの目視確認は行っていない。過去の検証結果を今回の変更後検証として流用しない。

- 原文書名：`AttOsGameDashboard — 図解仕様書 v1.0`
- 提供ファイル：`AttOsGameDashboard-visual-spec.md`
- 対象リポジトリ：[AttODao/AttOsGameAPI](https://forgejo.attodao.cc/AttODao/AttOsGameAPI)
- 原資料に記載された確認対象：`main` / `3133e77864`、2026-10-12

上記コミット・日時は原資料の記載を保存したもの。現在の実装状態や本書の検証基準を示すものではない。README・他の資料は本作業の書き込み範囲外であり変更しない。
