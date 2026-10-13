# ドキュメント

## AttOsGameDashboard

| 文書 | 読む目的 |
|---|---|
| [仕様書](spec/AttOsGameDashboard.md) | 何ができるか、誰が使えるか、どのように振る舞うかを知る |
| [実装設計案](design/AttOsGameDashboard.md) | 技術構成、データモデル、処理方式、API候補を検討する |
| [実装計画・検証課題](plans/AttOsGameDashboard.md) | 実装順序、実証が必要なこと、未確定事項、原資料を確認する |

## HoYoLABクライアント

- [HoYoLAB APIのリクエスト仕様・調査記録](hoyolab-api.md)
- [ゲームレコードAPI資料](hoyolab-game-record/README.md)
- [認証](hoyolab/authentication.md)
- [ゲームアカウント一覧](hoyolab/account-roles.md)
- [ログインボーナス](hoyolab/daily-checkin.md)
- [疎通確認クライアント](hoyolab/test-client.md)

## Dashboard文書の更新ルール

- 仕様書には利用者から見える機能・権限・制約・挙動を置く。実装方法、推奨値、作業手順、調査記録、文書の編集指示は分離する。
- 確定した要件と設計案・未確定事項を混同しない。図にないルールを勝手に補完せず、仕様本文や既存実装と照合する。
- 仕様を変更するときは、関連する図・表・文章も更新する。
- 文書はMarkdownとしてGitで管理する。文書内のMermaidソースを図の正本とし、PNG/SVGは派生成果物とする。
- 図は1図1目的とし、構文を検証する。可能ならレンダリング後の見切れ・矢印・可読性も確認する。
- Pi Mermaid Studioなどで別の`.mmd`を正本にする場合は、Markdownから参照する方式に切り替え、同じ図を二重に手動管理しない。
- Mermaidは責務・処理・関係を説明する。詳細なAPI契約やDB定義は、型・OpenAPI・マイグレーションで管理する。
