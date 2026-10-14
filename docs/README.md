# ドキュメント

## AttOsGameDashboard

| 文書 | 読む目的 |
|---|---|
| [仕様書](spec/AttOsGameDashboard.md) | 利用者から見える機能、権限、制約、現在の本番制限を知る |
| [実装設計案](design/AttOsGameDashboard.md) | 現在の責務分割、Dioxus Web/RSX UI、データ・API・ジョブ設計、proof gateを確認する |
| [実装計画・検証課題](plans/AttOsGameDashboard.md) | 実装済み範囲、残る上流実証、検証ギャップを確認する |
| [Dashboard OpenAPI 3.1](dashboard-api.json) | WebセッションAPIの手書き契約を機械可読で確認する |
| [Dashboard運用手順](dashboard-operations.md) | migration、bootstrap、TLS/Origin、秘密鍵、バックアップ、shutdownを確認する |

## HoYoLABクライアント

- [HoYoLAB APIのリクエスト仕様・調査記録](hoyolab-api.md)
- [ゲームレコードAPI資料](hoyolab-game-record/README.md)
- [認証](hoyolab/authentication.md)
- [ゲームアカウント一覧](hoyolab/account-roles.md)
- [ログインボーナス](hoyolab/daily-checkin.md)
- [疎通確認クライアント](hoyolab/test-client.md)

## Dashboard文書の更新ルール

- 仕様書には利用者から見える機能・権限・制約・挙動を置く。実装方法、推奨値、作業手順、調査記録、文書の編集指示は分離する。
- 確定した要件と設計案・未確定事項を混同しない。特にHoYoLAB owner bindingとreward POST contractは、実装済みpolicyと本番rollout可否を分けて書く。
- API契約は `dashboard-api.json` を正本とし、実装済みのDashboard routeだけを載せる。Dioxusのhash付き `/assets/{...}` は内部build出力であり、安定API pathとして追加しない。架空の `/record`、`/daily`、`/redirect`、`/large` routeを追加しない。
- UI要件はDioxus + RSX + Dioxus Componentsで固定する。native HTML/JS fallbackや別frontend方式を新規方針として書かない。
- 仕様を変更するときは、関連する図・表・文章も更新する。
- MermaidはMarkdown内のソースを正本にし、変更時は構文を検証する。
- 秘密値、Cookie、パスワード、セッショントークン、実アカウントの未加工応答を文書例に書かない。
