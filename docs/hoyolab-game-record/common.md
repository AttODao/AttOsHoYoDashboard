# 共通仕様

この文書は、[ゲームレコードAPI](README.md)の原神・崩壊：スターレイルに共通する形式を説明します。

## ベースURL

```text
原神:             https://sg-public-api.hoyolab.com/event/game_record/genshin/api
崩壊：スターレイル: https://bbs-api-os.hoyolab.com/game_record/hkrpg/api
```

各エンドポイントは、ベースURLの末尾にパスを付加します。例えば原神のリアルタイムノートは `GET <原神ベースURL>/dailyNote` です。

## 認証・ヘッダー

読み取りAPIには、少なくとも次の情報が必要です。

| 項目 | 値 | 用途 |
| --- | --- | --- |
| `Cookie` | `ltuid_v2`、`ltoken_v2`を含むCookie文字列 | HoYoLAB認証 |
| `User-Agent` | ブラウザ相当の値 | クライアント識別 |
| `DS` | リクエストごとに生成 | HoYoLABが要求する署名風の識別値 |
| `x-rpc-app_version` | `1.5.0` | クライアント情報 |
| `x-rpc-client_type` | `5` | クライアント情報 |
| `x-rpc-language` / `x-rpc-lang` | 例: `ja-jp` | 表示言語 |

Cookieと`DS`の詳細は [../hoyolab-api.md](../hoyolab-api.md) を参照してください。HTTPヘッダー名は大文字・小文字を区別しないため、`DS`と`ds`は同じものとして扱われます。

## リクエスト形式

### GET

`GET`エンドポイントでは、共通引数をクエリ文字列に入れます。

```text
GET <base>/<path>?role_id=<ゲームUID>&server=<ゲームサーバー>&<endpoint固有引数>
```

### POST

`POST`エンドポイントでは、共通引数と固有引数をJSONボディに入れます。

```json
{
  "role_id": "<ゲームUID>",
  "server": "<ゲームサーバー>",
  "...": "endpoint固有の値"
}
```

`genshin.py`の実装では、HTTPメソッドに応じて同じ引数セットをクエリまたはJSONボディへ振り分けています。

## 生レスポンスの外包

成功時の生レスポンスは、通常次の外包です。

```json
{
  "retcode": 0,
  "message": "OK",
  "data": {
    "...": "エンドポイント固有のデータ"
  }
}
```

`genshin.py`は`retcode == 0`を確認してから`data`だけを呼び出し元へ返します。本ディレクトリの各レスポンス例も、この内側の`data`を示します。

エラー時は`retcode`が非0です。代表例:

| `retcode` | 意味・対応 |
| ---: | --- |
| `-100` / `10001` | Cookieの無効化・期限切れ。再ログインしてCookieを更新する。 |
| `10102` | 戦績またはリアルタイムノートが非公開。HoYoLAB側の公開設定を確認する。 |
| `10101` | 参照制限の可能性。再試行を控え、時間を置く。 |

これは参照実装のエラー対応を基にした目安であり、メッセージやコードの割り当ては変わる可能性があります。

## 可視性と副作用

ゲームレコードには、公開設定が必要なものがあります。`dailyNote` と `note` はリアルタイム情報であり、`10102`になることがあります。

`genshin.py`には、本人のリアルタイムノートが非公開の場合に設定変更APIを呼ぶ補助機能がありますが、[`src/bin/login.rs`](../../src/bin/login.rs)はそのような設定変更をしません。APIサーバーへ組み込む際も、公開設定の変更は利用者の明示的な同意を得て行ってください。

