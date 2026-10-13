# 疎通テスト

[HoYoLAB API 調査の入口](../hoyolab-api.md)へ戻る。

[`src/bin/hoyolab_check.rs`](../../src/bin/hoyolab_check.rs)（機能実装: [`src/hoyolab/client.rs`](../../src/hoyolab/client.rs)） は、Cookieの有効性、連携済みゲームアカウント、ログインボーナス状態、ゲームレコードを読み取り専用で確認するプログラムである。

## 準備

```console
cp .env.example .env
```

`.env`にログイン済みブラウザのCookieヘッダーを設定する。

```dotenv
HOYOLAB_COOKIE="ltuid_v2=<HoYoLAB UID>; ltoken_v2=<認証トークン>"
```

必要なCookieと扱い方は[認証と共通リクエスト](authentication.md)を参照。`.env`はGit管理しない。

## 実行

```console
cargo run --bin hoyolab_check
```

型付きデータのJSONも表示する場合は次を使う（個人情報を含む非公開用出力）。

```console
cargo run --bin hoyolab_check -- --raw
```

設定ファイルを指定する場合は `cargo run --bin hoyolab_check -- --env <path>` を使う。既存の環境変数は設定ファイルより優先され、`HOYOLAB_LTUID_V2` / `HOYOLAB_LTOKEN_V2`による指定も利用できる。通常出力にはCookie値・UID・ニックネームを表示しない。`--raw`は型付きデータを出力し、UIDや戦績などの個人情報を含むため公開しないこと。

リダイレクトは追跡せず、各リクエスト20秒・全体90秒・レスポンス4MiBに制限する。

## 呼び出すAPI

`client.accounts()`で連携済みゲームアカウント一覧を取得し、両ゲームの`daily_info`・`daily_home`・`index`・`daily_note`を型付きモデルで検証する。

| 対象 | 原神 | 崩壊：スターレイル |
| --- | --- | --- |
| ログインボーナス状態 | `GET /event/sol/info` | `GET /event/luna/hkrpg/os/info` |
| 月間報酬一覧 | `GET /event/sol/home` | `GET /event/luna/hkrpg/os/home` |
| 基本ゲームレコード | `GET /event/game_record/genshin/api/index` | `GET /game_record/hkrpg/api/index` |
| リアルタイム状態 | `GET /event/game_record/genshin/api/dailyNote` | `GET /game_record/hkrpg/api/note` |

ログインボーナスを実際に受け取る`POST /sign`は呼ばず、公開設定の変更も行わない。URLと実装上の注意点は[ログインボーナス](daily-checkin.md)に記載している。

リアルタイム状態が`retcode=10102`になる場合は、HoYoLABの戦績画面で該当するリアルタイムノートの公開設定を確認すること。
