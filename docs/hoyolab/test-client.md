# 現行エンドポイントのテスト

[HoYoLAB API 調査の入口](../hoyolab-api.md)へ戻る。

テストはDashboardと同じく所有するモジュールへ置く。機能実装は[`src/hoyolab/client.rs`](../../src/hoyolab/client.rs)、エンドポイント定義は[`src/hoyolab/game.rs`](../../src/hoyolab/game.rs)。独立した確認用CLIは設けない。未実装のAPIや架空の`/record`・`/daily`等はテスト対象にしない。

## モック検証（通常実行）

[`endpoint_tests.rs`](../../src/hoyolab/client/endpoint_tests.rs)は実際の`Genshin`・`Starrail`と`accounts()`を呼び、現行のHTTPSホスト名・パス・method・query・ヘッダー・型付きモデル変換を確認する。名前解決をループバックのTLSモックへ固定し、Cookieと応答は合成値だけを使う。受け取りPOSTもローカルモックにしか送らない。

```console
cargo test --locked --lib hoyolab::client
```

`protocol_tests.rs`はエンベロープのretcode／data要件・上流エラー秘匿・現行リクエストの組み立て、`validation_tests.rs`はCookie・DS入力を検証する。これらは実APIのtoken-to-UID拘束や現在の生レスポンス形式を証明しない。

## 実APIの読み取り確認（明示実行のみ）

[`live_tests.rs`](../../src/hoyolab/client/live_tests.rs)の1件は、外部通信・実Cookieを要するため`#[ignore]`。通常の`cargo test`では実行しない。**別途実通信を承認した場合だけ**、Cookieを`HOYOLAB_COOKIE`環境変数として渡して実行する。`.env`の自動読み込みや環境変数の変更はしない。Cookieをコマンド履歴や公開ログへ記録しないこと。

```console
cargo test --locked --lib hoyolab::client::live_tests::current_read_only_endpoints -- --ignored --exact
```

必要なら`HOYOLAB_USER_AGENT`も環境変数として指定できる。設定不足・対象ゲーム未連携・APIエラーは失敗とし、スキップで成功扱いしない。各リクエスト20秒・全体90秒・レスポンス4MiBの上限、リダイレクト非追跡を維持し、Cookie・UID・ニックネーム・生データは出力しない。

## 対象

`GET /binding/api/getUserGameRolesByCookie`で連携済みゲームアカウント一覧を取得する。両ゲームの読み取りは次の通り。

| 対象 | 原神 | 崩壊：スターレイル |
| --- | --- | --- |
| ログインボーナス状態 | `GET /event/sol/info` | `GET /event/luna/hkrpg/os/info` |
| 月間報酬一覧 | `GET /event/sol/home` | `GET /event/luna/hkrpg/os/home` |
| 基本ゲームレコード | `GET /event/game_record/genshin/api/index` | `GET /game_record/hkrpg/api/index` |
| リアルタイム状態 | `GET /event/game_record/genshin/api/dailyNote` | `GET /game_record/hkrpg/api/note` |

実APIテストは`POST /sign`も公開設定変更も行わない。モックの`POST /sign`は送信形式・追加認証応答の変換だけを検証する。URL・認証の注意点は[認証](authentication.md)・[ログインボーナス](daily-checkin.md)を参照。

実HoYoLABアカウントは現在1個のみ。読み取り成功もモックの複数UIDも本人UIDの拘束証明にはならない。候補の本人確認APIは既存クライアントに未実装なので、このテストに先回りして追加しない。
