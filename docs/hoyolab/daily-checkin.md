# ログインボーナス

[HoYoLAB API 調査の入口](../hoyolab-api.md)へ戻る。

このページは海外版の原神・崩壊：スターレイルに限る。受け取りは状態を変更する操作なので、通常は先に`GET /info`で`is_sign`を確認し、未受け取りのときだけ`POST /sign`を実行する。

## API一覧

すべてのURLに`act_id`と`lang=ja-jp`を含めた。`lang`は表示言語であり、別の言語を指定する場合は置き換える。

| 操作 | 原神 | 崩壊：スターレイル |
| --- | --- | --- |
| 状態確認 | `GET https://sg-hk4e-api.hoyolab.com/event/sol/info?act_id=e202102251931481&lang=ja-jp` | `GET https://sg-public-api.hoyolab.com/event/luna/hkrpg/os/info?act_id=e202303301540311&lang=ja-jp` |
| **実際に受け取る** | **`POST https://sg-hk4e-api.hoyolab.com/event/sol/sign?act_id=e202102251931481&lang=ja-jp`** | **`POST https://sg-public-api.hoyolab.com/event/luna/hkrpg/os/sign?act_id=e202303301540311&lang=ja-jp`** |
| 月間報酬一覧 | `GET https://sg-hk4e-api.hoyolab.com/event/sol/home?act_id=e202102251931481&lang=ja-jp` | `GET https://sg-public-api.hoyolab.com/event/luna/hkrpg/os/home?act_id=e202303301540311&lang=ja-jp` |

`POST /sign`は受け取りを発生させる。自動化するサービスでは、同じアカウントについて同時に複数回実行しないこと。[`実APIテスト`](../../src/hoyolab/client/live_tests.rs)はログインボーナスの`GET /info`と`GET /home`を型付きモデルで検証し、`POST /sign`は呼ばない。[`モックテスト`](../../src/hoyolab/client/endpoint_tests.rs)の`POST /sign`はループバックにだけ送信する。

## リクエスト仕様

### クエリとボディ

| 項目 | 値 | 必須 |
| --- | --- | --- |
| `act_id` | 原神: `e202102251931481`、スターレイル: `e202303301540311` | はい |
| `lang` | 例: `ja-jp` | はい |
| JSONボディ | なし（参照実装は空ボディで`POST /sign`を呼ぶ） | なし |

### ヘッダー

| ヘッダー | 値 |
| --- | --- |
| `Cookie` | ログイン済みCookie（[認証と共通リクエスト](authentication.md)を参照） |
| `User-Agent` | ブラウザ相当のUser-Agent |
| `Referer` | `https://act.hoyolab.com/` |
| `x-rpc-signgame` | 原神: `hk4e`、スターレイル: `hkrpg` |

海外版のログインボーナスでは、`genshin.py`は`DS`や`x-rpc-app_version`を付けずに上記のヘッダーを設定する。追加認証が発生した場合は、`x-rpc-challenge`、`x-rpc-seccode`、`x-rpc-validate`を求められることがあるが、通常の受け取りリクエストに固定値を付与してはいけない。

## レスポンスと実行順

各APIは、一般に次のエンベロープを返す。

```json
{
  "retcode": 0,
  "message": "OK",
  "data": {}
}
```

`GET /info`では少なくとも`data.is_sign`（本日受け取り済みか。海外版の「本日」はUTC+8の0時で切り替わる）と`data.total_sign_day`（当月の累積受け取り日数）を使う。`GET /home`の`data.awards`には月間報酬が入り、要素は`name`、`cnt`、`icon`を持つ。

受け取りの安全な流れは次の通り。

1. `GET /info`を実行する。
2. `retcode`が`0`で、かつ`data.is_sign`が`false`の場合だけ`POST /sign`を実行する。
3. `POST /sign`の`retcode`を確認する。
4. 必要なら再度`GET /info`と`GET /home`を呼び、受け取った日数と報酬を確定する。

`retcode`が成功以外の場合は受け取り済み・Cookie失効・追加認証・サーバー側の制限などを区別して処理する。`POST /sign`を成功扱いにする前に必ず`retcode`を確認すること。

## 調査根拠

`genshin.py` は海外版の報酬ベースURLを [routes.py](https://github.com/seriaati/genshin.py/blob/ab289440508ef81a9672a198a53b9a31f9365c4a/genshin/client/routes.py) で定義し、[daily.py](https://github.com/seriaati/genshin.py/blob/ab289440508ef81a9672a198a53b9a31f9365c4a/genshin/client/components/daily.py) で`POST /sign`を実行している。HoYoLAB側の仕様変更や追加認証には追随が必要である。
