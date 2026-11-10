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
ブランドは AHD。既存の黒い面、マゼンタ／シアン、原神のミント、スターレイルのラベンダー、数値のOrbitron、本文のNoto Sans JPを維持する。日本語／英語とdark／lightに対応。詳細は `DESIGN.md`。

ダッシュボードは「開いて数秒で全体を見渡す」ための画面で、文字のラベルよりアイコン・記号・数値で伝える。アイコンはすべて自作のSVG線画で、ゲームの絵や公式ロゴは同梱しない。公式画像はHoYoLABの応答が返すURLのもの（派遣キャラクター・当日のログインボーナスの報酬）だけを使う。アイコンには必ず読み上げ用の名前を付け、色だけで状態を伝えず、数値と日時は実テキストで出す。警告・エラー・不明の表示は減らさず、不明の値を本物の値のように見せない。

## Scope and evidence
このルート文書が本実装用のImpeccableコンテキスト。`docs/ui-design/PRODUCT.md` と `docs/ui-design/DESIGN.md` は承認済みR3ドラフトの履歴であり、本実装の検証記録は `docs/ui-design/production-r3.md`。ダッシュボードは2026-11-11にユーザーが承認したアイコン中心UI（A案：アイコンタイル）へ作り直した。この変更に限り「全面リデザインしない」の方針を外し、削った・畳んだ要素は `DESIGN.md` に列挙する。イベント・設定・管理画面は今後のカードで同じアイコンセットに揃える。それ以外の修正では、承認済みの見た目を保ったまま依頼範囲の操作性・可読性・アクセシビリティを改善する。
