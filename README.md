# AttOsGameAPI

原神・崩壊：スターレイルのHoYoLAB APIを呼ぶ型付きRustクライアントです。Cookie認証の疎通確認プログラムと、ゲームAPIが未実装のHTTPサーバー雛形も含みます。

## 型付きAPIクライアント

```rust,no_run
use attosgameapi::{
    game::{Genshin, Starrail},
    hoyolab::{HoyolabClient, RecordAccount},
};

# async fn example() -> Result<(), String> {
let cookie = std::env::var("HOYOLAB_COOKIE").map_err(|_| "Cookieが未設定です")?;
let client = HoyolabClient::new(&cookie)?;
let account = RecordAccount {
    uid: "123456789".into(),
    server: "os_asia".into(),
};
let index = client.index(Genshin, &account).await?;
let note = client.daily_note(Genshin, &account).await?;
let info = client.daily_info(Starrail).await?;
let home = client.daily_home(Starrail).await?;
# Ok(())
# }
```

`index`と`daily_note`には、対象ゲームのUIDとserverが必要です。serverはアカウント一覧から取得した値を渡してください。`daily_info`、`daily_home`、`daily_sign`はCookieのアカウントを対象とします。戻り値は検証済みエンベロープの内側の型付き`data`です。

`daily_sign(Genshin)`／`daily_sign(Starrail)`だけは実際に報酬を受け取るPOSTです。自動再試行はしません。成功retcodeでも追加認証が必要な場合があるため、戻り値の`requires_verification()`と認証情報を確認してください。`None`は受取成功を意味せず、受取の確定には`daily_info`の`is_sign`を再確認します。テストはローカルのモックAPIだけを使用し、本番の受け取り操作は行いません。

`GameKind`は`Genshin(Genshin)`／`Starrail(Starrail)`を持ち、JSONでは`"genshin"`／`"starrail"`として読み書きします。実行時にゲームを選ぶ場合は、このenumを`match`して型付きメソッドを呼びます。新しいゲームはstruct、`Game`実装、対応モデル、`GameKind`への追加で対応できます。HTTPサーバー側のゲームendpointは未実装のままです。

## 責務別の配置

- `src/hoyolab/mod.rs`: HoYoLABクライアントの公開窓口。
- `src/hoyolab/client.rs`: Cookie認証、DS署名、HTTP通信、エンベロープ検証。
- `src/hoyolab/game.rs`: ゲームのマーカー型、`Game` trait、エンドポイント定数。
- `src/hoyolab/models/`: 共通・ゲーム別の型付きレスポンスモデル。
- `src/lib.rs`: ライブラリの入口。既存の`game`・`models`はルートからも再exportします。
- `src/main.rs`・サーバー側モジュール: HTTPサーバー雛形（ゲームハンドラーは未実装）。
- `examples/hoyolab_check.rs`: 読み取り専用の実API疎通確認。
- `tests/fixtures/hoyolab/*.json`: モック検証用の合成fixture（認証済み実データではありません）。
- ルートの`.env`・`.env.example`: ローカル認証設定と秘密情報を含まない設定例。

## ビルド・テスト

Rustのstableツールチェーン（edition 2024対応）を使用します。

```console
cargo build --locked --bins --examples
cargo test --locked --all-targets
```

サーバー雛形は`cargo run`で起動します（ポート8080、`/docs`と`/openapi.json`）。旧Go実装と同じくゲーム操作用のハンドラーはありません。APIドキュメントUIは外部CDNを使用します。

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

2026-10-12（環境の日時）に`genshin.py` master (`ab289440508ef81a9672a198a53b9a31f9365c4a`) と実エンドポイントへの無効Cookie GETを使って再確認しました。以前の`cf675f3`と比べ、以下の移植対象のURL・Cookie利用・DS署名・ヘッダー値に変更はありません。原神には`GET /gcg/matchList`が追加されていますが、既存の疎通プログラムの範囲には含めません。有効Cookieでの成功や全レスポンスフィールドの不変性は未確認です。詳しい結果と根拠は[再確認記録](docs/hoyolab-api.md#以前の仕様との差分と現行値)を参照してください。

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
cp .env.example .env
```

`.env`を編集します。

```dotenv
HOYOLAB_COOKIE="ltuid_v2=123456789; ltoken_v2=v2_xxxxxxxxxxxxxxxxx"
```

`.env`はルートの`.gitignore`で除外しています。実値を`.env.example`やソースコードに書かないでください。

## 疎通確認

```console
cargo run --example hoyolab_check
```

型付きデータのJSONも確認する場合（個人情報を含む非公開用出力）:

```console
cargo run --example hoyolab_check -- --raw
```

設定ファイルを変える場合は`cargo run --example hoyolab_check -- --env <path>`を使用します。既存の環境変数を設定ファイルより優先し、`HOYOLAB_LTUID_V2`と`HOYOLAB_LTOKEN_V2`による指定も使用できます。リダイレクトは追跡せず、各リクエスト20秒・全体90秒・レスポンス4MiBに制限します。`--raw`の出力にはUIDや戦績などの個人情報が含まれるため、公開しないでください。

プログラムは`client.accounts()`で連携済みアカウント一覧を取得し、両ゲームの`daily_info`・`daily_home`・`index`・`daily_note`を型付きモデルで検証します。通常出力にはCookie値・UID・ニックネームを表示しません。対象のGETは次の通りです。

| 対象 | 原神 | 崩壊：スターレイル |
| --- | --- | --- |
| ログインボーナス状態 | `/event/sol/info` | `/event/luna/hkrpg/os/info` |
| 月間報酬一覧 | `/event/sol/home` | `/event/luna/hkrpg/os/home` |
| 基本ゲームレコード | `/event/game_record/genshin/api/index` | `/game_record/hkrpg/api/index` |
| リアルタイム状態 | `/event/game_record/genshin/api/dailyNote` | `/game_record/hkrpg/api/note` |

これは読み取り専用の確認です。ログインボーナスを実際に受け取る`POST .../sign`や公開設定の変更は行いません。リアルタイム状態が`retcode=10102`になる場合は、HoYoLABの戦績画面でリアルタイムノートの公開設定を確認してください。

調査元は[genshin.py](https://github.com/seriaati/genshin.py)の`routes.py`、`daily.py`、原神／スターレイルのBattle Chronicle実装です。
