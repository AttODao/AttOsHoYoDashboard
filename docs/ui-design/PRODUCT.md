# Product context for design drafts

<!-- impeccable:product-schema 1 -->

本書はドラフト制作時点のproduct context。[後続の本番R3実装・検証](production-r3.md)とは記録を分ける。

## Platform
web

## Users and purpose
ユーザーの依頼で確定：原神・崩壊：スターレイルを中心にゲーム状態、UID表示、HoYoLAB連携、ログインボーナスを管理する利用者、およびDashboardユーザーを管理する管理者。

## Capabilities and constraints
仕様の正本は `docs/spec/`、設計補足は `docs/design/`、HTTP境界は `docs/dashboard-api.json`。本番は Rust + Dioxus Web/WASM + RSX。API・認証・DB・製品UIの実装を変更しない。最終決定は人間がPenpotで行う。旧案・R2を保存したうえで、現在はユーザーの修正指定をR3のローカルプレビューとNative Penpotへ反映する。MCPは再開許可済み。今回の目アイコン・英語・メニュー内言語／テーマ操作は同じR3内の小改修で、R4にはしない。合成パスワードは空・read-only、実際の表示切替・motion・設定保存はデザイン用ブラウザ資料だけ。保存は`agd-preview-lang`／`agd-preview-theme`のみで、認証情報・表示状態は永続化しない。ログイン／初期設定にはメニューを出さず保存済み設定を継承し、初期設定完了前のログアウトのみのゲートを変えない。
Cookie保存と本人確認完了を区別する。HoYoLAB連携・手動／定期報酬は通常機能で、暗号化設定・パスワード変更・本人確認済み連携など既存の利用条件を満たす場合に利用可能。利用承認と実上流契約の実証は別である。実在するCookie・認証情報は使わず、全例は合成データにする。架空の機能・API・実績を追加しない。追加要望の管理者ログ確認は場所を用意するが、現行の取得APIがない点を明示する。

## Brand commitments
AttOsGameDashboard、画面のブランド表記はAGD。ユーザー指定の https://www.uupm.cc/demo/nft-web3 の黒い面・マゼンタ／シアン・表示書体を基準にする。常設サイドバーなし、上部ハンバーガー1つ（右全高drawer・明示X）、日本語／英語とdark／lightの操作はメニュー内、Dashboard以外を設定へ集約、ログインのユーザー名は共通、仮登録の初期設定は隔離する。ゲーム別の区別、明確な情報階層、自然なスマートフォン操作を保つ。過剰な装飾やアニメーション、公式UIの模倣を避ける。

## Evidence on hand
`dashboard-ui/src/pages/`、`dashboard-ui/src/app/`、`dashboard-ui/assets/`。既存assetsはCSSのみでキャラクター画像・ロゴはない。接続済みPenpotファイルは開始時点で空。R1の編集可能ボードは保存済みだが最終描画の検証が未完了。R2の50場面・共通部品20・Guideは保存済み。当初R3は別ページ／Assetsに43場面・25共通部品・171リンク・名前付き10 Flowを保存。今回もR1／R2は維持し、R3既存メニュー4枚の子だけを置換してIDを保持、既存12枚へ静的な目20個＋共有masterを追加済み。残る元39枚の本文・元25共通Assetsは保持。ブラウザ59場面（43＋言語／テーマ代表16）と限定模擬検査は成功したが、追加16枚も保存済み。Native59枚・map51＋目master・207リンク・途中印なし・検証エラー0。ライトの追加画像はユーザー指定で省略。公開drawer参照`https://artifact-diagnoser.ktonk.com/`は実ChromiumのDesktop／Mobileで観察済み（`/tmp/agd-menu-reference/`）。

## Open decisions
R3はユーザーが本実装の基準として承認。全Native画像の最終批評を意味しない。Nativeは静的代表状態の遷移で、実フォーム・生成・clipboard・全role／ページ組合せ・設定永続化の動作保証ではない。必須パスワード変更で現在値を省くAPI方針、自動生成仮パスワードの生成主体・安全な受渡し、再発行後に既存連携が未完了の場合の扱いは実装前に確定する。通常画面の説明文を削除し、初期登録と再発行後の画面を分離する。再発行後のデザイン例は既存連携確認済み・Cookie入力なしとするが、未完了連携の既存利用条件を解除するAPI変更はしていない。管理者が確認するログの種類・権限・保存期間・取得契約は再開後に確認する。利用シーンの時間帯や利用者属性は推測しない。アクセシビリティ基準は製品の別仕様として追加せず、デザイン検査では本文4.5:1と44px以上の操作領域を目標にする。
