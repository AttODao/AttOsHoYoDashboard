# Product context for design drafts

<!-- impeccable:product-schema 1 -->

## Platform
web

## Users and purpose
ユーザーの依頼で確定：原神・崩壊：スターレイルを中心にゲーム状態、UID表示、HoYoLAB連携、ログインボーナスを管理する利用者、およびDashboardユーザーを管理する管理者。

## Capabilities and constraints
仕様の正本は `docs/spec/`、設計補足は `docs/design/`、HTTP境界は `docs/dashboard-api.json`。本番は Rust + Dioxus Web/WASM + RSX。API・認証・DB・製品UIの実装を変更しない。最終決定は人間がPenpotで行う。AIは2案を比較し、推奨候補を他画面に展開するところまで委任されている。
Cookie保存と本人確認完了を区別する。HoYoLAB連携・手動／定期報酬は通常機能で、暗号化設定・パスワード変更・本人確認済み連携など既存の利用条件を満たす場合に利用可能。利用承認と実上流契約の実証は別である。実在するCookie・認証情報は使わず、全例は合成データにする。架空の機能・API・実績を追加しない。

## Brand commitments
AttOsGameDashboard。ダーク基調、モダンで少し大胆なゲーム系UI、ゲーム別の区別、明確な情報階層、自然なスマートフォン操作。過剰な装飾やアニメーション、公式UIの模倣を避ける。

## Evidence on hand
`dashboard-ui/src/pages/`、`dashboard-ui/src/app/`、`dashboard-ui/assets/`。既存assetsはCSSのみでキャラクター画像・ロゴはない。接続済みPenpotファイルは開始時点で空。

## Open decisions
推奨候補の採否、最終配色・日本語表記・ゲーム見出し書体は人間の確定待ち。利用シーンの時間帯や利用者属性は推測しない。アクセシビリティ基準は製品の別仕様として追加せず、デザイン検査では本文4.5:1と44px以上の操作領域を目標にする。
