# AttOsHoYoDashboard

<!-- impeccable:product-schema 1 -->

## Platform
web

## Users and purpose
原神・崩壊：スターレイルのゲーム状態とログインボーナスを確認・設定する利用者、およびユーザーを管理する管理者向けの認証付きダッシュボード。

## Capabilities and constraints
本実装は Rust API + Dioxus Web/WASM。`dashboard-ui/src/` と `dashboard-ui/assets/dashboard.css` が現在のUI。API仕様は `docs/dashboard-api.json` と `docs/spec/`。
初期設定のパスワード変更・HoYoLAB本人確認が完了するまで通常画面を開放しない。Cookie保存と本人確認を区別し、実アカウントや秘密情報をデザイン検証に使わない。検証には合成レスポンスを使用する。

## Brand commitments
ブランドは AHD。既存の黒い面、マゼンタ／シアン、原神のミント、スターレイルのラベンダーを維持する。日本語／英語とdark／lightに対応。詳細は `DESIGN.md`。

## Scope and evidence
このルート文書が本実装用のImpeccableコンテキスト。`docs/ui-design/PRODUCT.md` と `docs/ui-design/DESIGN.md` は承認済みR3ドラフトの履歴であり、本実装の検証記録は `docs/ui-design/production-r3.md`。既存UIの修正では全面リデザインせず、依頼範囲の操作性・可読性・アクセシビリティを改善する。
