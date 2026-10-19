# AGD R3 本番RSX統合

2026-10-15。承認済みR3を`dashboard-ui`へ反映した作業記録。Penpot／合成プレビューの保存記録とは分ける。

## 実装範囲

- 共通ユーザー名のログイン、必須設定の新パスワード＋確認、通常変更の現在パスワード、全パスワード欄の目。秘密・表示状態は保存しない。
- 304pxの右drawer、管理者限定項目、メニュー内の日本語／英語・dark／light。保存するのは`agd-language`と`agd-theme`だけ。
- desktopの2ゲーム同時表示、mobileのゲームタブ。表示UID・取得状態・automationはゲームごと。設定は5タブ、時／分selectとサーバーのtimezone。
- 管理者作成／再発行はWeb Cryptoで24文字・144bitの仮パスワードを生成。既存の作成201／再発行204を使用し、成功時だけ一時表示。clipboardの完了前に成功を表示しない。自分の行は通常のパスワード変更へ誘導し、自己再発行によるsession失効で生成結果を受け取れなくなる事態を避ける。本UIの管理画面から自己無効化・自己削除は送信しない（自身の削除は設定から既存の業務ルールに従う）。
- 必須変更APIは最新の必須設定アカウントに限り現在値を省略可能。通常変更の確認、lock下の再認証、固定期限・失効、実際の連携readinessは維持。再発行の理由flagやCookie迂回は追加しない。

`pages/common.rs`は共通入力・アイコン・native dialogのDOM lifecycle、`app/preferences.rs`は非機密設定、`app/credentials.rs`は生成・clipboard境界、`app/data.rs`はゲーム別取得と設定保存の共有処理を持つ。

## 2026-10-16 可読性・操作の修正

- 全回復と宝銭上限の見込みを意味付き`time`で表示し、選択言語・ブラウザのローカル時刻／タイムゾーンに整形。全回復だけOrbitron、宝銭は本文フォント。未取得・不正な日時は従来の未取得表示とする。
- 派遣の明示的な名称／場所名と空リストを要約に保持。原神の通常の派遣データには名前がないため、URLから推測せず「派遣 1」等の番号を使う。空配列は「派遣なし」、配列自体がない場合は未取得を区別する。Star Railの実通信での仕様差は確認していない。
- メニューの連携画面ショートカットを外部HoYoLABリンクへ置換。連携設定タブは維持。言語はnative select。ログアウトの仕切り線と内側フォーカス枠を分離し、フォーカス時も配置を保つ。
- ロゴはDashboardへのリンクで、初期設定中はゲートを維持。`app/navigation.rs`が画面ごとのURLハッシュと戻る／進む・再読み込みを扱い、`app/state.rs`は共有の画面遷移と取得起動を担う。
- ログインボーナス設定と実行履歴の間に区切り線。ユーザー管理は横スクロール領域内の意味付きtableとし、最後の有効な管理者の警告は操作時にのみ表示する。業務層・DB側の保護は変更しない。

検証：devenv経由の整形・workspace全target／WASM Clippy、UI単体42件、ゲーム要約7件、release buildと通常／reduced-motionのブラウザ回帰を通過。日時の言語／タイムゾーン・フォント、派遣の欠落／空配列、履歴・再読み込み・初期設定ゲート、外部リンク、selectの翻訳／保存、ログアウトのフォーカス位置、table／操作時の管理者保護を検査。Desktop／mobileをcaptureして確認。Impeccableの手動検査で既存タブの3px下線が角丸カードと誤検知されたため、`border-radius:0`の実装を根拠にCSSファイルだけの例外を`.impeccable/config.json`へ記録した。実アカウント／実HoYoLAB通信は未実施。

## 同日の追加修正：日時・派遣画像・操作位置

- 見込み日時は言語にかかわらず`10/17 21:38 JST`形式。月日・24時間・ブラウザのローカルタイムゾーンを明示し、全回復のOrbitron／宝銭の本文フォントを維持する。
- 派遣の名前欄を原神の`avatar_side_icon`／Star Railの`item_url`へ変更。名称はalt／titleとして維持し、画像の欠落・失敗時はラベル付き代替アイコンを出す。Refererを送らず、CSPの画像読込先だけを`upload-os-bbs.hoyolab.com`／`act-upload.hoyolab.com`へ限定追加する。script/style/connectの同一Origin制限は維持する。
- ゲーム要約のcacheをversion 2へ更新し、古い派遣fieldのないcacheを再取得する。Star Railの非空派遣→item icon表示を合成fixtureで確認。実ユーザーの上流応答が空／欠落となる原因は実通信なしでは未確定で、修復済みとは断定しない。
- ユーザー管理の3ボタンを固定幅の同じ列に配置し、長いラベルの折返しを防止。ログアウトは区切り線を持つgroup内に上下同じpaddingで配置する。

検証：devenv経由で整形・workspace全target／WASM Clippy、UI42件、ゲーム18件、HTTP UI境界3件、release build、通常／reduced-motionのブラウザ回帰を通過。画像読込は公式host URLへのリクエストをローカル画像で応答し、外部通信を禁止したまま本番CSPを検査。JSTの日付繰越・言語切替後の形式・画像失敗時の代替・ボタン列位置／高さ・ログアウト上下余白を確認。Impeccable手動検査は指摘なし。Desktop／mobileの表示もcaptureして確認した。

## 通常パス・認証転送と派遣確認

- URLをハッシュから`/login`／`/dashboard`／`/settings/games`等の通常パスへ移行。ルートはセッション確認後に未ログインなら`/login`、ログイン済みなら`/dashboard`、明示的なログイン成功後は`/dashboard`へreplaceStateする。認証済みの設定deep linkは再読み込み・戻る／進むでも維持。`/settings`はHTTP308で`/settings/games`へ転送する。公開`/docs`は認証APIを呼ばず、API／未知のパスはSPAへfallbackしない。
- ユーザーの別途承認により、ローカルdevenvの保存済みCookieをメモリ内だけで使用し、各ゲームのnote GETを確認。DBはread-only transactionで読み、Cookie／key／UID／未編集レスポンスは出力しない。確認時、原神は進行中5件、Star RailはAPI自身が`accepted_epedition_num:0`／空配列を返した。本人確認の追加実証や報酬POSTは実施していない。
- 原神の実応答が使う画像hostは`act-webstatic.hoyoverse.com`だったため、CSPのimg-srcにそのhostだけを追加。合成URLへのローカル画像応答で本番CSP下の読込を再検証した。ユーザー指定によりStar Railの派遣欄は削除し、原神だけ残す。上流の派遣取得／API契約は変更しない。
- `games/live_tests.rs`は明示ignore・追加承認flag・devenv socket限定・単一verified link・DB read-only・GET限定の検査として残す。既定の全テストでは実行しない。設定不足やAPI失敗はskipせず失敗させる。

検証：devenv経由の整形・workspace全target／WASM Clippy、非live Rustテスト234件成功（live2件は既定ignore）、release build、通常／reduced-motionのブラウザ回帰、Impeccable検査を通過。全体実行は時間上限に達したがRust全件とreduced-motionは完了しており、通常motionを単独再実行して成功を確認した。ルートの認証転送・明示ログイン・private deep link・通常パスでの履歴／再読込・settings308・公開docs・非空データでもStar Railの派遣欄を出さないことを確認した。

## 表示文のi18n分離

本番RSXの対訳・固定表示文を`dashboard-ui/locales/ja.json`／`en.json`へ移した。`src/i18n.rs`が埋め込みカタログの参照・英語fallback・名前付き補間を持ち、`app/preferences.rs`は言語／テーマ選択だけを扱う。API／WebAuthnエラーも安全なキーを保持し、UIで翻訳する。通知・取得エラーは言語変更時に再描画され、パスワード／UID選択／認証境界は維持する。read-only docsの見出し・表ラベルも翻訳対象にし、APIの提供データはそのまま表示する。

検証：devenvで整形、workspace／WASM Clippy、UI unit 38件、release build、通常／reduced-motionのブラウザ回帰を通過。カタログ一致・placeholder・fallback・未知キーの秘匿、言語変更時の通知再翻訳とフォーム／ARIA状態維持を含む。実API・実アカウントにはアクセスしていない。

## 配信・フォント

既存CSPを維持し、`unsafe-eval`／`unsafe-inline`は追加しない。ブラウザ操作は直接DOM APIを使い、Dioxusの`document::eval`に依存しない。フォントはGoogle Fonts配布のNoto Sans JP／Orbitron WOFF2 subsetを`assets/fonts/`へ同梱し、OFL本文も同梱。外部CDNやinline font stylesheetは使わない。日本語subsetは初回UIソースの文字集合、Orbitronはwordmark／数字を対象とする。現在の表示文の正本は`locales/`で、追加文言の未収録字形や動的な固有名には通常のsans-serif fallbackも使う。

### ブランドアイコン

ユーザー提供の`image.png`（730×399）の中央360×360（左185・上24）から図柄を抽出し、灰色背景`#333`と輪郭への背景色の混入を除去。透過PNGの`dashboard-ui/assets/dashboard-icon.png`（256×256）と`favicon.png`（32×32）を同梱する。前者は共通HeaderのAGD左隣に36px・間隔10pxで配置し、隣のブランド名と読み上げが重複しないよう`alt=""`にする。後者はAppの`rel="icon"`から参照し、既存CSP・同一Origin配信を維持する。元画像は変更しない。

devenvで整形・native／WASM Clippy・UI unit 38件・release build・通常／reduced-motionのブラウザ回帰に成功。配信された両PNGの寸法、四隅の完全透過、中央の不透明、輪郭の半透明、ブランド名との位置関係とmobile幅の収まりを検証。darkのdesktop／mobile Headerをcapture・目視確認し、切り抜き素材をdark／light背景で確認した。

## 2026-10-19 設定画面の操作性

- 設定の確認・次回・前回実行・実行履歴・HoYoLAB更新・パスキー登録・UID検出の日時を`app/display.rs`の共通形式（`10/17 21:38 JST`）へ統一。不正・欠落は「未取得」で、ISO文字列を出さない。
- `error_code`（`network`／`http:*`／`upstream:*`／`upstream_data`／`action_required`）と未知の状態・ゲーム種別を翻訳文へ対応付け、未知値は汎用文言。生のコードは折り畳んだ「取得情報」だけに残す。
- HoYoLAB連携：必要なCookie（`ltoken_v2`／`ltuid_v2`／`ltmid_v2`）・取得場所・ヘッダー行の貼り付け・旧形式不可を入力欄の説明として`aria-describedby`で結ぶ。状態の説明文を置き、保存済み・未確認では本人確認をprimaryにする。UID同期が無効な理由を表示。
- 手動実行の受付後、カード内に前回実行の状態を表示し、3秒間隔・最大1分のGETで追従。完了・画面離脱・セッション変更・新しい追従で停止し、完了をlive regionで通知する。POSTは再送しない。
- パスワードの12〜256バイト規則（全角の数え方を含む）を事前に表示し、長さ・確認不一致・現在パスワード未入力を欄の直下に表示。無効な送信は入力を保持し、最初の問題欄へフォーカスする。
- パスキー削除は共有`ModalDialog`で確認（Cancel／Escape／フォーカス復帰）。自身の削除はdanger-zoneと結果の説明、管理者の削除dialogにも同等の説明を追加。
- 取得失敗かつデータなしでは失敗だけを表示し、古いデータは「◯分／時間／日前のデータです」と理由を表示。未取得の表記を1キーに統一し、設定タブ「セキュリティ」をmobileでも同じ名前にした。

検証：devenvでUI unit 58件、WASM／native Clippy、release build、通常／reduced-motionのブラウザ回帰（上記をsynthetic fixtureで検査）を通過。Impeccable手動検査は指摘なし。ja／en・dark／light・375／800／1280をcaptureして確認した。既知の差：同梱のNoto Sans JP subsetには既存文言27字・今回の文言26字（例：貼「」旧形式／）が未収録で、OSのsans-serifへfallbackする（CJKフォントのない検証環境では豆腐表示）。subsetの再生成は未実施。

## 検証状況

本番release bundleとlocal mockによるブラウザ検証を完了。

- 追加のworkspace全targetテストも成功：合計209件成功、失敗0件。実HoYoLAB GETの1件だけは、明示承認が必要なため既定のignoreを維持した。
- backend：隔離PostgreSQL 18、`DATABASE_URL`を除外してDashboardテスト165件成功。現在値省略・通常変更拒否・再利用・期限・再発行・lock競合を含む。
- UI：native unit 34件成功。workspace全targetのClippyとWASM Clippyを`-D warnings`で通過。`dx build --package attos-dashboard-ui --platform web --release`成功。
- Chromiumで本番bundleを通常motion／reduced-motionの双方で実行して回帰検証成功。ログイン・必須設定／検証済み再設定・401時のsession再確認・2ゲームのUIDとautomation分離・設定タブと時刻・drawer閉鎖／focus復帰・非機密設定永続化・WebAuthn・生成失敗／clipboardの待機と失敗／削除受付・owner切替・read-only docsとCSPを検査した。管理者が2人いるfixtureでも自分の行から再発行・無効化・削除を送信しない。
- darkの代表8画像（login en1280／ja390、setup en1280、dashboard en1440／390、menu／settings／admin en390）をcapture・目視確認。CSS Moduleのhash名には依存せず`data-style`／`data-size`／`data-slot`で既存部品をskinする。データ取得の技術情報は折り畳み、既存fieldを保持する。
- 本番ブラウザの回帰は`dashboard-ui/tests/browser.mjs`。実装済みendpointだけをlocal mockし、本番CSPを維持する。秘密を診断出力しない。
- ライトの画像確認はユーザー指定により省略。テーマ切替と永続化はDOMで検証した。
- 実アカウント、実認証器、実HoYoLAB GET／報酬POST、production DBは使用しない。管理者ログ取得APIは追加しない。

## 再実行

ツールは[開発環境](../../README.md#devenv推奨)にまとめた。devenv環境でも全体チェックの成功を確認済み。

```sh
devenv test --clean
# UIだけ再検証する場合（build＋通常／reduced-motion）
devenv shell -- test-ui
```

このmock検証は上流token-to-UID binding・報酬契約・実認証器の実証ではない。
