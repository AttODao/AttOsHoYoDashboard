# AttOsGameAPI

GoからHoYoLABの非公式APIを調査するための最小構成です。現時点ではAPIサーバー本体ではなく、Cookie認証と原神・崩壊：スターレイルの読み取りAPIを確認する疎通プログラムだけを含みます。

詳細なリクエスト仕様は [docs/hoyolab-api.md](docs/hoyolab-api.md) を参照してください。

ゲームレコードAPIのエンドポイント一覧とレスポンス構造は [docs/hoyolab-game-record/README.md](docs/hoyolab-game-record/README.md) を参照してください。

## 認証に必要な情報

海外版HoYoLABでは、今回確認するログインボーナスとゲームレコードのGET APIに次のCookieを使用します。

- `ltuid_v2`: HoYoLABアカウントID
- `ltoken_v2`: HoYoLAB認証トークン

ゲームレコードにはゲームUID (`game_uid`) とサーバー識別子 (`region`) も必要ですが、これはCookieではありません。`getUserGameRolesByCookie`から自動取得します。

`cookie_token_v2` / `account_id_v2` はコード交換など別のAPIで必要になることがありますが、今回の読み取り確認には不要です。`stoken`は期限切れCookieの再発行を実装するときに役立ちますが、このテストには保存しません。

HoYoLAB APIは公式の公開APIではなく、エンドポイント、署名用salt、必要Cookieが予告なく変わる可能性があります。Cookieはパスワード同様の秘密情報として扱ってください。

## 調査したリクエスト仕様

2026-07-26時点の`genshin.py` master (`cf675f3`) と実エンドポイントへの無効Cookieリクエストを基にしています。

アカウント一覧:

- `GET https://api-os-takumi.mihoyo.com/binding/api/getUserGameRolesByCookie`
- Cookie認証と`DS`ヘッダーを使用する。
- 戻り値の`game_biz`、`game_uid`、`region`から対象ゲーム・UID・サーバーを特定する。

ログインボーナス:

- 原神: `https://sg-hk4e-api.hoyolab.com/event/sol`
  - `act_id=e202102251931481`
  - `x-rpc-signgame: hk4e`
- 崩壊：スターレイル: `https://sg-public-api.hoyolab.com/event/luna/hkrpg/os`
  - `act_id=e202303301540311`
  - `x-rpc-signgame: hkrpg`
- 共通して`GET /info`が当日の受け取り状態、`GET /home`が月間報酬一覧、`POST /sign`が受け取り操作。
- 海外版では`lang`、`Referer: https://act.hoyolab.com/`、Cookie、User-Agentを送る。

ゲームレコード:

- 原神: `https://sg-public-api.hoyolab.com/event/game_record/genshin/api`
  - `GET /index`: 基本戦績
  - `GET /dailyNote`: 天然樹脂、探索派遣などのリアルタイム状態
- 崩壊：スターレイル: `https://bbs-api-os.hoyolab.com/game_record/hkrpg/api`
  - `GET /index`: 基本戦績
  - `GET /note`: 開拓力、依頼などのリアルタイム状態
- 共通クエリは`role_id=<game_uid>&server=<region>`。
- Cookie、User-Agent、`x-rpc-app_version`、`x-rpc-client_type`、言語、`DS`署名を送る。

海外版の`DS`は、現在の参照実装と同じくUnix秒、ランダム英字6文字、固定saltをMD5した値です。これは暗号学的な用途ではなく、HoYoLABクライアント識別用のプロトコル値です。

## Cookieの用意

1. ブラウザで <https://www.hoyolab.com> にログインする。
2. 開発者ツールの Application（Firefoxでは Storage）から、`https://www.hoyolab.com` のCookiesを開く。
3. `ltuid_v2` と `ltoken_v2` の値をコピーする。コンソールにCookieを表示するスクリプトは実行しない。
4. 次のようにローカル設定を作る。

```console
cp test/.env.example test/.env
```

`test/.env`を編集します。

```dotenv
HOYOLAB_COOKIE="ltuid_v2=123456789; ltoken_v2=v2_xxxxxxxxxxxxxxxxx"
```

`test/.env`はルートの`.gitignore`で除外しています。実値を`test/.env.example`やソースコードに書かないでください。

## 疎通確認

```console
go run ./test/login.go
```

完全なゲームレコードJSONも確認する場合:

```console
go run ./test/login.go -raw
```

プログラムは次を順にGETします。

| 対象 | 原神 | 崩壊：スターレイル |
| --- | --- | --- |
| ログインボーナス状態 | `/event/sol/info` | `/event/luna/hkrpg/os/info` |
| 基本ゲームレコード | `/event/game_record/genshin/api/index` | `/game_record/hkrpg/api/index` |
| リアルタイム状態 | `/event/game_record/genshin/api/dailyNote` | `/game_record/hkrpg/api/note` |

これは読み取り専用テストです。ログインボーナスを実際に受け取る`POST .../sign`は呼びません。リアルタイム状態が`retcode=10102`になる場合は、HoYoLABの戦績画面でリアルタイムノートの公開設定を有効にしてから再実行してください。

調査元は[genshin.py](https://github.com/seriaati/genshin.py)の`routes.py`、`daily.py`、原神／スターレイルのBattle Chronicle実装です。
