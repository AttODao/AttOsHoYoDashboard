# HoYoLAB API 調査

原神・崩壊：スターレイルの海外版HoYoLAB APIを利用するための入口です。認証、ログインボーナス、ゲームアカウントの特定、ゲームレコードを目的別に分けています。

HoYoLABには、この用途の一般公開された安定版API仕様書がありません。以下は、`genshin.py` の現行実装と現行エンドポイントの [`実APIテスト`](../src/hoyolab/client/live_tests.rs)・[`モックテスト`](../src/hoyolab/client/endpoint_tests.rs)（機能実装: [`src/hoyolab/client.rs`](../src/hoyolab/client.rs)） を基にした調査メモです。Cookie名・エンドポイント・必要ヘッダーは変更される可能性があります。

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
- 確認日時（環境の `date --iso-8601=seconds`）: 2026-10-12T06:25:53+00:00（GET開始: 06:25:17+00:00）
- 現在の upstream master: [`ab289440508ef81a9672a198a53b9a31f9365c4a`](https://github.com/seriaati/genshin.py/tree/ab289440508ef81a9672a198a53b9a31f9365c4a)（`git ls-remote` と取得したHEADで一致確認）
- 比較した以前の基準: docsは2026-07-27、README.mdは2026-07-26、いずれも `cf675f341f7b4f9a0311f64a1438307b919ebeed`。Rust移植後の実行手順は[疎通テスト](hoyolab/test-client.md)を参照。

## 以前の仕様との差分と現行値

対象のURL・認証要求について旧commitとのソース差分と現行実装を照合した。

| 項目 | 現行仕様・旧基準との差分 |
| --- | --- |
| アカウント一覧 | 変更なし。`GET https://api-os-takumi.mihoyo.com/binding/api/getUserGameRolesByCookie`。`game_biz` / `game_uid` / `region`を使い、UID・regionはCookieではない。 |
| Cookie | 対象フローの `ltuid_v2` / `ltoken_v2` 利用に変更なし。`cookie_token_v2` / `account_id_v2` の追加必須化は参照実装に見られない。成功に十分なCookieの組合せは無効Cookie検証では証明できない。stoken更新は対象外。 |
| 海外版DS | 変更なし。`<Unix秒>,<ランダム英字6文字>,<MD5>`、MD5対象は `salt=6s25p5ox5y14umn1p61aqyyvbvvl3lrt&t=<Unix秒>&r=<英字6文字>`。アカウント一覧・レコードに `x-rpc-app_version: 1.5.0`、`x-rpc-client_type: 5`、`x-rpc-language` / `x-rpc-lang: ja-jp`。 |
| 原神ログインボーナス | 変更なし。`https://sg-hk4e-api.hoyolab.com/event/sol`、`act_id=e202102251931481`、`x-rpc-signgame: hk4e`。 |
| スターレイルログインボーナス | 変更なし。`https://sg-public-api.hoyolab.com/event/luna/hkrpg/os`、`act_id=e202303301540311`、`x-rpc-signgame: hkrpg`。 |
| ボーナス共通 | `GET /info`・`GET /home`、`lang=ja-jp`、`Referer: https://act.hoyolab.com/`、Cookie・User-Agent。海外版では参照実装はDSセットを付けない。 |
| 原神レコード | ベースURL `https://sg-public-api.hoyolab.com/event/game_record/genshin/api` と `/index`・`/dailyNote` は変更なし。**`GET /gcg/matchList` が追加**（七聖召喚の対戦履歴、固有引数なし）。 |
| スターレイルレコード | `https://bbs-api-os.hoyolab.com/game_record/hkrpg/api` と既存10 GETは変更なし（基本 `/index`、リアルタイム `/note`）。 |
| レコード共通 | `role_id=<game_uid>&server=<region>` とDSセットは変更なし。日時のtimezone付加処理がupstreamで変更されたが、これはPython側のモデル変換でありHTTPの生JSON変更の証拠ではない。 |

移植対象のアカウント一覧・`/info`・`/index`・リアルタイムノートには定数変更は不要。原神の件数表は旧文書の集計誤り（GET 10 / POST 3）を修正した。旧実装はGET 9 / POST 4、現行はGET 10 / POST 4（14パス）。POSTはソース確認のみで呼び出していない。

### 実エンドポイント: 無効Cookieの読み取りGET

固定ダミー `ltuid_v2=0; ltoken_v2=invalid` のみを送信。Cookie・User-Agentと、上表の各種ヘッダーを付けた。レコードのダミー引数は `role_id=800000000`、原神 `server=os_asia`、スターレイル `server=prod_official_asia`。全10件がHTTP 200でJSON外包 `retcode` / `message` / `data` を返した（HTTP 200だけで成功と判定してはいけない）。

| GET | retcode | 観測結果 |
| --- | ---: | --- |
| アカウント一覧 | -100 | 「ログイン情報失効。再ログインしてください」、`data=null` |
| 原神・スターレイル `/info`（各1件） | -100 | 「ログインしてください」、`data=null` |
| 原神 `/home` | 0 | `month` / `awards` / `resign` / `now`。`awards` は31件。 |
| スターレイル `/home` | 0 | `month` / `awards` / `biz` / `resign` / `short_extra_award`。`awards` は31件。 |
| 原神 `/index`・`/dailyNote`・`/gcg/matchList`（各1件） | 10001 | `Please login`、`data=null` |
| スターレイル `/index`・`/note`（各1件） | 10001 | `Please login`、`data=null` |

### 根拠と検証限界

- 現行commitの [routes.py](https://github.com/seriaati/genshin.py/blob/ab289440508ef81a9672a198a53b9a31f9365c4a/genshin/client/routes.py)、[daily.py](https://github.com/seriaati/genshin.py/blob/ab289440508ef81a9672a198a53b9a31f9365c4a/genshin/client/components/daily.py)、[base.py](https://github.com/seriaati/genshin.py/blob/ab289440508ef81a9672a198a53b9a31f9365c4a/genshin/client/components/base.py)、[DS生成](https://github.com/seriaati/genshin.py/blob/ab289440508ef81a9672a198a53b9a31f9365c4a/genshin/utility/ds.py)、[salt定義](https://github.com/seriaati/genshin.py/blob/ab289440508ef81a9672a198a53b9a31f9365c4a/genshin/constants.py)および[Chronicle実装](https://github.com/seriaati/genshin.py/tree/ab289440508ef81a9672a198a53b9a31f9365c4a/genshin/client/components/chronicle)を確認した。
- 認証エラーはそのURLでの応答を確認しただけで、パスの有効性、各ヘッダーの必須性、DS検証の有無、有効Cookieでの成功や実データ形式を保証しない。`/home`の成功もCookie不要であることの一般的保証ではない。成功した状態APIやゲームレコードのJSONは取得していない。
- 既存の詳細JSON例は参照実装由来の参考形であり、今回全フィールドを実レスポンスで再検証してはいない。`/home`の件数・フィールドは当月の観測値である。
- 秘密情報・`.env`は未読。POST、`/sign`、公開設定変更、Cookie再発行は一切実行していない。有効Cookieテストは今回の対象外。

Cookieはパスワードと同じ秘密情報です。実値をソースコード、Git、ログ、Issue、チャットに残さないでください。
