# HoYoLABゲームレコードAPI

指定された海外版のゲームレコードAPIを、原神と崩壊：スターレイルに分けて記録します。

| ゲーム | ベースURL | 詳細 |
| --- | --- | --- |
| 原神 | `https://sg-public-api.hoyolab.com/event/game_record/genshin/api` | [genshin.md](genshin.md) |
| 崩壊：スターレイル | `https://bbs-api-os.hoyolab.com/game_record/hkrpg/api` | [starrail.md](starrail.md) |

共通の認証、ヘッダー、リクエスト形式、レスポンス外包は [common.md](common.md) を参照してください。

## 調査範囲

HoYoLABにはこのAPIの公式かつ網羅的な公開仕様がありません。また、ベースURLに対してHTTP `GET`を送っても利用可能なパスの一覧は返りません。

そのため、この文書の「利用可能なエンドポイント」は、2026-10-12時点で `seriaati/genshin.py` の現行master（commit `ab289440508ef81a9672a198a53b9a31f9365c4a`）が、この2つのベースURLへ実際にリクエストするパスを抽出したものです。参照実装に未対応のエンドポイントが存在しないことは保証しません。

| ゲーム | GET | POST | 合計 |
| --- | ---: | ---: | ---: |
| 原神 | 10 | 4 | 14 |
| 崩壊：スターレイル | 10 | 0 | 10 |

旧commitとの差分・無効Cookie GET結果と検証限界は [調査の入口](../hoyolab-api.md) に記載しています。原神の `GET /gcg/matchList` が追加されました。旧件数表はGET/POSTの集計誤りも修正しています。POSTは呼び出していません。

レスポンス例は、API外包を除いた `data` 部分の構造です。`...` は省略したフィールドまたは配列要素を表します。フィールドはゲーム更新で追加・削除される可能性があります。

## 文書の読み方

- **HTTPメソッド**: `GET` はクエリ文字列、`POST` はJSONボディで引数を送ります。
- **共通引数**: ほぼすべてのパスで `role_id`（ゲームUID）と `server`（ゲームサーバー）が必要です。
- **レスポンス**: APIの生レスポンスは `retcode` / `message` / `data` で包まれます。各ゲーム別文書では `data` を説明します。
- **書き込み**: 原神の `POST /character/top` だけは状態を変更する可能性があります。疎通テストでは呼びません。

## 参照元

- [共通のゲームレコード要求処理](https://github.com/seriaati/genshin.py/blob/ab289440508ef81a9672a198a53b9a31f9365c4a/genshin/client/components/chronicle/base.py)
- [原神のChronicle実装](https://github.com/seriaati/genshin.py/blob/ab289440508ef81a9672a198a53b9a31f9365c4a/genshin/client/components/chronicle/genshin.py)
- [スターレイルのChronicle実装](https://github.com/seriaati/genshin.py/blob/ab289440508ef81a9672a198a53b9a31f9365c4a/genshin/client/components/chronicle/starrail.py)
- [ベースURL定義](https://github.com/seriaati/genshin.py/blob/ab289440508ef81a9672a198a53b9a31f9365c4a/genshin/client/routes.py)

