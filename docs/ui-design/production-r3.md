# AGD R3 本番RSX統合

2026-10-15。承認済みR3を`dashboard-ui`へ反映した作業記録。Penpot／合成プレビューの保存記録とは分ける。

## 実装範囲

- 共通ユーザー名のログイン、必須設定の新パスワード＋確認、通常変更の現在パスワード、全パスワード欄の目。秘密・表示状態は保存しない。
- 304pxの右drawer、管理者限定項目、メニュー内の日本語／英語・dark／light。保存するのは`agd-language`と`agd-theme`だけ。
- desktopの2ゲーム同時表示、mobileのゲームタブ。表示UID・取得状態・automationはゲームごと。設定は5タブ、時／分selectとサーバーのtimezone。
- 管理者作成／再発行はWeb Cryptoで24文字・144bitの仮パスワードを生成。既存の作成201／再発行204を使用し、成功時だけ一時表示。clipboardの完了前に成功を表示しない。自分の行は通常のパスワード変更へ誘導し、自己再発行によるsession失効で生成結果を受け取れなくなる事態を避ける。本UIの管理画面から自己無効化・自己削除は送信しない（自身の削除は設定から既存の業務ルールに従う）。
- 必須変更APIは最新の必須設定アカウントに限り現在値を省略可能。通常変更の確認、lock下の再認証、固定期限・失効、実際の連携readinessは維持。再発行の理由flagやCookie迂回は追加しない。

`pages/common.rs`は共通入力・アイコン・native dialogのDOM lifecycle、`app/preferences.rs`は非機密設定、`app/credentials.rs`は生成・clipboard境界、`app/data.rs`はゲーム別取得と設定保存の共有処理を持つ。

## 表示文のi18n分離

本番RSXの対訳・固定表示文を`dashboard-ui/locales/ja.json`／`en.json`へ移した。`src/i18n.rs`が埋め込みカタログの参照・英語fallback・名前付き補間を持ち、`app/preferences.rs`は言語／テーマ選択だけを扱う。API／WebAuthnエラーも安全なキーを保持し、UIで翻訳する。通知・取得エラーは言語変更時に再描画され、パスワード／UID選択／認証境界は維持する。read-only docsの見出し・表ラベルも翻訳対象にし、APIの提供データはそのまま表示する。

検証：devenvで整形、workspace／WASM Clippy、UI unit 38件、release build、通常／reduced-motionのブラウザ回帰を通過。カタログ一致・placeholder・fallback・未知キーの秘匿、言語変更時の通知再翻訳とフォーム／ARIA状態維持を含む。実API・実アカウントにはアクセスしていない。

## 配信・フォント

既存CSPを維持し、`unsafe-eval`／`unsafe-inline`は追加しない。ブラウザ操作は直接DOM APIを使い、Dioxusの`document::eval`に依存しない。フォントはGoogle Fonts配布のNoto Sans JP／Orbitron WOFF2 subsetを`assets/fonts/`へ同梱し、OFL本文も同梱。外部CDNやinline font stylesheetは使わない。日本語subsetは初回UIソースの文字集合、Orbitronはwordmark／数字を対象とする。現在の表示文の正本は`locales/`で、追加文言の未収録字形や動的な固有名には通常のsans-serif fallbackも使う。

### ブランドアイコン

ユーザー提供の`image.png`（730×399）の中央360×360（左185・上24）から図柄を抽出し、灰色背景`#333`と輪郭への背景色の混入を除去。透過PNGの`dashboard-ui/assets/dashboard-icon.png`（256×256）と`favicon.png`（32×32）を同梱する。前者は共通HeaderのAGD左隣に36px・間隔10pxで配置し、隣のブランド名と読み上げが重複しないよう`alt=""`にする。後者はAppの`rel="icon"`から参照し、既存CSP・同一Origin配信を維持する。元画像は変更しない。

devenvで整形・native／WASM Clippy・UI unit 38件・release build・通常／reduced-motionのブラウザ回帰に成功。配信された両PNGの寸法、四隅の完全透過、中央の不透明、輪郭の半透明、ブランド名との位置関係とmobile幅の収まりを検証。darkのdesktop／mobile Headerをcapture・目視確認し、切り抜き素材をdark／light背景で確認した。

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
