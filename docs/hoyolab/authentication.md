# 認証と共通リクエスト

[HoYoLAB API 調査の入口](../hoyolab-api.md)へ戻る。

## Cookie

海外版の読み取りAPIおよびログインボーナスで、まず次のv2 Cookieを使う。

| Cookie名 | 役割 |
| --- | --- |
| `ltuid_v2` | HoYoLABアカウントID |
| `ltoken_v2` | HoYoLABログイン認証トークン |

ローカルの `.env` にはCookieヘッダー全体を保存する。

```dotenv
HOYOLAB_COOKIE="ltuid_v2=<HoYoLAB UID>; ltoken_v2=<認証トークン>"
```

`.env` はGit管理しない。Cookieの実値を出力・共有しないこと。

### 互換Cookieと任意Cookie

[`src/hoyolab/client.rs`](../../src/hoyolab/client.rs) は、旧形式の `ltuid` / `ltoken` も認識する。`account_id_v2`（または旧形式の `account_id`）は、対応する`ltuid`がない場合の検証用フォールバックであり、コードが生成・追加するものではない。

`HOYOLAB_COOKIE` に含まれる `cookie_token_v2`、`account_mid_v2`、`ltmid_v2`、`stoken` などは文字列全体の一部として送られるが、このプロジェクトの対象APIで必須とはしていない。Cookieの型は参照実装の [cookie.py](https://github.com/seriaati/genshin.py/blob/ab289440508ef81a9672a198a53b9a31f9365c4a/genshin/models/auth/cookie.py) も参照すること。

## すべての対象リクエストに送るヘッダー

| ヘッダー | 値 | 備考 |
| --- | --- | --- |
| `Cookie` | `HOYOLAB_COOKIE` の値全体 | 値を加工せず送る |
| `User-Agent` | `HOYOLAB_USER_AGENT`、未設定時はテストの既定値 | ブラウザ相当の値 |

Cookieヘッダーの設定箇所は [`src/hoyolab/client.rs`](../../src/hoyolab/client.rs) である。

## DSと`x-rpc-*`ヘッダー

ゲームアカウント一覧とゲームレコードでは、上記に加えて次を送る。

| ヘッダー | 値 |
| --- | --- |
| `DS` | リクエストごとに生成 |
| `x-rpc-app_version` | `1.5.0` |
| `x-rpc-client_type` | `5` |
| `x-rpc-language` | `ja-jp` |
| `x-rpc-lang` | `ja-jp` |

海外版で使う`DS`の形式は次の通り。

```text
<Unix秒>,<ランダム英字6文字>,<MD5値>
```

`MD5値`は、次の文字列をMD5にした値である。

```text
salt=6s25p5ox5y14umn1p61aqyyvbvvl3lrt&t=<Unix秒>&r=<ランダム英字6文字>
```

これはパスワードのハッシュではなく、APIが求めるリクエスト識別値である。実装の根拠は [genshin.py の `ds.py`](https://github.com/seriaati/genshin.py/blob/ab289440508ef81a9672a198a53b9a31f9365c4a/genshin/utility/ds.py)。HTTPヘッダー名は大文字・小文字を区別しないため、`DS`と`ds`は同じヘッダーとして扱われる。

> ログインボーナスの海外版エンドポイントはこのDSセットを使用しない。必要ヘッダーは[ログインボーナス](daily-checkin.md)を参照。

## このプロジェクトが送らない値

読み取りテストは、以下を送らない。

```text
x-rpc-device_id
x-rpc-device_fp
x-rpc-app_id
x-rpc-game_biz
x-rpc-risky
x-rpc-aigis
x-rpc-verify
x-rpc-challenge
x-rpc-seccode
x-rpc-validate
```

これらはパスワードログイン、ゲームログイン、Geetestなど別のフローで使われる値である。ログインボーナス受け取り時に追加認証（Geetest）が求められた場合は、APIレスポンスを処理して別途対応が必要になる。
