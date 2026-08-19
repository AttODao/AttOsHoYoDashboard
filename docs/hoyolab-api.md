# HoYoLAB API 調査

原神・崩壊：スターレイルの海外版HoYoLAB APIを利用するための入口です。認証、ログインボーナス、ゲームアカウントの特定、ゲームレコードを目的別に分けています。

HoYoLABには、この用途の一般公開された安定版API仕様書がありません。以下は、`genshin.py` の現行実装と疎通確認用の [`test/login.go`](../test/login.go) を基にした調査メモです。Cookie名・エンドポイント・必要ヘッダーは変更される可能性があります。

## 目的別の案内

| 確認したいこと | ドキュメント |
| --- | --- |
| Cookie、共通ヘッダー、DS署名 | [認証と共通リクエスト](hoyolab/authentication.md) |
| HoYoLABに連携済みのゲームUID・サーバーを得る | [ゲームアカウント一覧](hoyolab/account-roles.md) |
| 当日の状態確認、実際のログインボーナス受け取り | [ログインボーナス](hoyolab/daily-checkin.md) |
| 原神・スターレイルのゲームレコード全エンドポイントと`data`形式 | [ゲームレコード API](hoyolab-game-record/README.md) |
| 既存の読み取り専用テストの動かし方 | [疎通テスト](hoyolab/test-client.md) |

## 範囲

- 対象リージョン: 海外版 (`Region.OVERSEAS`)
- 対象ゲーム: 原神 (`hk4e_global`) と崩壊：スターレイル (`hkrpg_global`)
- 調査基準日: 2026-07-27
- 参照した `genshin.py` commit: [`cf675f341f7b4f9a0311f64a1438307b919ebeed`](https://github.com/seriaati/genshin.py/tree/cf675f341f7b4f9a0311f64a1438307b919ebeed)

Cookieはパスワードと同じ秘密情報です。実値をソースコード、Git、ログ、Issue、チャットに残さないでください。
