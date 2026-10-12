# 疎通テスト

[HoYoLAB API 調査の入口](../hoyolab-api.md)へ戻る。

[`src/bin/login.rs`](../../src/bin/login.rs)（機能実装: [`src/hoyolab.rs`](../../src/hoyolab.rs)） は、Cookieの有効性、連携済みゲームアカウント、ログインボーナス状態、ゲームレコードを読み取り専用で確認するプログラムである。

## 準備

```console
cp test/.env.example test/.env
```

`test/.env`にログイン済みブラウザのCookieヘッダーを設定する。

```dotenv
HOYOLAB_COOKIE="ltuid_v2=<HoYoLAB UID>; ltoken_v2=<認証トークン>"
```

必要なCookieと扱い方は[認証と共通リクエスト](authentication.md)を参照。`test/.env`はGit管理しない。

## 実行

```console
cargo run --bin login
```

完全なゲームレコードJSONも表示する場合は次を使う。

```console
cargo run --bin login -- --raw
```

設定ファイルを指定する場合は `cargo run --bin login -- --env <path>` を使う。旧CLI互換の `-raw` / `-env <path>` も利用できる（例: `cargo run --bin login -- -raw`）。raw出力にはゲームアカウント情報が含まれるため、取り扱いには注意する。

## 呼び出すAPI

| 対象 | 原神 | 崩壊：スターレイル |
| --- | --- | --- |
| ログインボーナス状態 | `GET /event/sol/info` | `GET /event/luna/hkrpg/os/info` |
| 基本ゲームレコード | `GET /event/game_record/genshin/api/index` | `GET /game_record/hkrpg/api/index` |
| リアルタイム状態 | `GET /event/game_record/genshin/api/dailyNote` | `GET /game_record/hkrpg/api/note` |

ログインボーナスを実際に受け取る`POST /sign`は呼ばない。URLと実装上の注意点は[ログインボーナス](daily-checkin.md)に記載している。

リアルタイム状態が`retcode=10102`になる場合は、HoYoLABの戦績画面で該当するリアルタイムノートの公開設定を確認すること。
