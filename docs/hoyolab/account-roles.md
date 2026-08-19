# ゲームアカウント一覧

[HoYoLAB API 調査の入口](../hoyolab-api.md)へ戻る。

ゲームレコードを呼ぶには、ゲームUID（`role_id`）とサーバー（`server`）が必要になる。これらはCookieの一部ではない。次のAPIでHoYoLABに連携済みのゲームアカウントから取得する。

```text
GET https://api-os-takumi.mihoyo.com/binding/api/getUserGameRolesByCookie
```

リクエストヘッダーは [認証と共通リクエスト](authentication.md) のCookie・User-Agent・DS・`x-rpc-*`セットを使用する。クエリパラメーターおよびリクエストボディは不要である。

## レスポンス

共通エンベロープの`data.list`に連携済みアカウントが入る。

```json
{
  "retcode": 0,
  "message": "OK",
  "data": {
    "list": [
      {
        "game_biz": "hk4e_global",
        "game_uid": "123456789",
        "region": "os_asia",
        "nickname": "Traveler",
        "level": 60
      }
    ]
  }
}
```

| JSONフィールド | 用途 |
| --- | --- |
| `game_biz` | 対象ゲームの判定 |
| `game_uid` | ゲームレコードの`role_id` |
| `region` | ゲームレコードの`server` |
| `nickname` | 表示用 |
| `level` | 複数アカウントから代表アカウントを選ぶ際に利用可能 |

このプロジェクトで対象にする`game_biz`は以下である。

| ゲーム | `game_biz` |
| --- | --- |
| 原神 | `hk4e_global` |
| 崩壊：スターレイル | `hkrpg_global` |

取得した`game_uid`と`region`を、ゲームレコードの`role_id`と`server`にそのまま使う。個別のAPIとレスポンス形式は[ゲームレコード API](../hoyolab-game-record/README.md)を参照。
