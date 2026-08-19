# 原神ゲームレコードAPI

ベースURL:

```text
https://sg-public-api.hoyolab.com/event/game_record/genshin/api
```

共通のリクエスト・レスポンス形式は [common.md](common.md) を参照してください。以下のJSONはすべて、生レスポンスの`data`部分を省略して示しています。

## エンドポイント一覧

| パス | HTTPメソッド | 固有引数 | 主な用途 |
| --- | --- | --- | --- |
| `/index` | GET | `avatar_list_type`（任意） | プロフィール、実績、探索度、キャラクター概要 |
| `/character/list` | POST | なし | 所持キャラクターと装備の概要 |
| `/character/detail` | POST | `character_ids` | キャラクター詳細、聖遺物、天賦 |
| `/spiralAbyss` | GET | `schedule_type` | 深境螺旋の履歴 |
| `/role_combat` | GET | `need_detail` | 幻想シアター |
| `/dailyNote` | GET | なし | 樹脂、派遣、デイリーなどのリアルタイム情報 |
| `/activities` | GET | なし | イベント記録 |
| `/gcg/basicInfo` | GET | なし | 七聖召喚の概要 |
| `/gcg/cardList` | GET | `need_avatar`、`need_action`、`offset`、`limit`、`need_stats` | 七聖召喚カード一覧 |
| `/act_calendar` | POST | なし | イベント・祈願カレンダー |
| `/char_master` | GET | なし | 空想の軌跡対象キャラクター |
| `/hard_challenge` | GET | `need_detail` | Stygian Onslaught記録 |
| `/character/top` | POST | `avatar_ids`、`uid_key`、`server_key` | 表示用上位キャラクターの更新（書き込み） |

## プロフィールとキャラクター

### `GET /index`

共通クエリに加え、装備なしのキャラクター概要を明示する場合は`avatar_list_type=0`を指定できます。

```json
{
  "role": { "nickname": "...", "level": 0, "region": "..." },
  "stats": {
    "achievement_number": 0,
    "active_day_number": 0,
    "avatar_number": 0,
    "spiral_abyss": "...",
    "common_chest_number": 0,
    "way_point_number": 0,
    "role_combat": { "is_unlock": true, "max_round_id": 0 },
    "hard_challenge": { "is_unlock": true, "has_data": false }
  },
  "avatars": [{ "id": 0, "name": "...", "level": 0, "fetter": 0 }],
  "world_explorations": [{ "name": "...", "exploration_percentage": 0, "offerings": [] }],
  "homes": []
}
```

`stats`には元素神の瞳数、宝箱数、ワープポイント数、秘境数なども含まれます。`world_explorations`には地域ごとの探索度・供物・討伐情報が入ります。

### `POST /character/list`

リクエストボディは`role_id`と`server`のみです。レスポンスは所持キャラクターごとの概要と武器です。

```json
{
  "list": [
    {
      "id": 0,
      "name": "...",
      "element": "...",
      "level": 0,
      "fetter": 0,
      "actived_constellation_num": 0,
      "weapon": {
        "id": 0,
        "name": "...",
        "level": 0,
        "rarity": 0,
        "affix_level": 0
      }
    }
  ]
}
```

### `POST /character/detail`

リクエストボディに、詳細を取得するキャラクターID一覧を加えます。

```json
{
  "role_id": "<UID>",
  "server": "<server>",
  "character_ids": [10000002, 10000003]
}
```

レスポンスにはキャラクターごとの武器、聖遺物、命ノ星座、天賦、各種ステータスと、Wiki用の対応表が入ります。

```json
{
  "list": [
    {
      "id": 0,
      "name": "...",
      "level": 0,
      "weapon": { "main_property": {}, "sub_property": {} },
      "relics": [{ "main_property": {}, "sub_property_list": [] }],
      "constellations": [],
      "skills": [],
      "selected_properties": [],
      "base_properties": [],
      "extra_properties": [],
      "element_properties": []
    }
  ],
  "property_map": {},
  "relic_property_options": {},
  "relic_wiki": {},
  "weapon_wiki": {},
  "avatar_wiki": {}
}
```

### `POST /character/top`

これは唯一の書き込みエンドポイントです。表示する上位キャラクターを更新します。

```json
{
  "avatar_ids": [10000002],
  "uid_key": "<UID>",
  "server_key": "<server>"
}
```

参照実装は成功レスポンスをモデル化せず、呼び出し元へ値を返しません。`retcode == 0`以外はエラーとして扱うべきです。自動化で呼ぶ前に、設定変更であることを利用者へ明示してください。

## 戦闘・挑戦記録

### `GET /spiralAbyss`

固有クエリは`schedule_type`です。

| 値 | 対象 |
| ---: | --- |
| `1` | 現在期 |
| `2` | 前期 |

```json
{
  "is_unlock": true,
  "schedule_id": 0,
  "start_time": 0,
  "end_time": 0,
  "total_battle_times": 0,
  "total_win_times": 0,
  "max_floor": "...",
  "total_star": 0,
  "reveal_rank": [],
  "defeat_rank": [],
  "damage_rank": [],
  "floors": [
    {
      "index": 0,
      "star": 0,
      "levels": [
        { "index": 0, "star": 0, "battles": [] }
      ]
    }
  ]
}
```

ランキング配列は、出場回数・撃破数・最大ダメージなどのキャラクターランキングです。`floors[].levels[].battles[]`には前半／後半ごとのキャラクターとタイムスタンプが入ります。

### `GET /role_combat`

幻想シアターの記録です。`need_detail=true`または`false`をクエリで指定します。

```json
{
  "is_unlock": true,
  "data": [
    {
      "rounds_data": [],
      "backup_avatars": [],
      "stat": { "difficulty_id": 0, "max_round_id": 0 },
      "schedule": { "schedule_id": 0, "start_time": 0, "end_time": 0 },
      "has_data": false,
      "has_detail_data": false,
      "fight_statisic": {}
    }
  ]
}
```

`rounds_data`には各幕の使用キャラクター、選択カード、バフ、終了時刻が入ります。

### `GET /hard_challenge`

Stygian Onslaughtの記録です。`need_detail=true`または`false`を指定します。通常の`data`の中に、さらに`data`配列があります。

```json
{
  "data": [
    {
      "schedule": { "schedule_id": 0, "name": "...", "is_valid": true },
      "single": { "best": {}, "challenge": [], "has_data": false },
      "mp": { "best": {}, "challenge": [], "has_data": false }
    }
  ]
}
```

`single`はソロ、`mp`はマルチプレイの記録です。各`challenge`には編成、敵、所要秒数、最高記録キャラクターが入ります。

## リアルタイム情報とイベント

### `GET /dailyNote`

```json
{
  "current_resin": 0,
  "max_resin": 200,
  "resin_recovery_time": "0",
  "current_home_coin": 0,
  "max_home_coin": 0,
  "home_coin_recovery_time": "0",
  "finished_task_num": 0,
  "total_task_num": 4,
  "is_extra_task_reward_received": false,
  "remain_resin_discount_num": 0,
  "resin_discount_num_limit": 3,
  "transformer": {},
  "expeditions": [
    { "avatar_side_icon": "...", "status": "Ongoing", "remained_time": "0" }
  ],
  "max_expedition_num": 0,
  "archon_quest_progress": {},
  "daily_task": {}
}
```

復帰時間はAPIでは秒数を文字列で返す項目があります。`transformer`、魔神任務進行、デイリー詳細はネストされたオブジェクトです。本人のリアルタイムノート設定が非公開だと取得できないことがあります。

### `GET /activities`

イベントごとにキーと構造が変わるため、固定スキーマとして扱うのは危険です。参照実装は`hyakunin_ikki`、`labyrinth_warriors`、`energy_amplifier`、`study_in_potions`、`summertime_odyssey`などの既知キーをモデル化しています。

```json
{
  "<イベント識別子>": {
    "exists_data": true,
    "records": []
  }
}
```

未開催・未対応イベントではキーが欠ける、または追加キーが現れる可能性があります。

### `GET /char_master`

```json
{
  "list": [
    {
      "avatar_id": 0,
      "name": "...",
      "icon": "...",
      "status": 0,
      "has_red_dot": false,
      "level_id": 0
    }
  ]
}
```

## 七聖召喚

### `GET /gcg/basicInfo`

```json
{
  "avatar_card_num_gained": 0,
  "action_card_num_gained": 0,
  "avatar_card_num_total": 0,
  "action_card_num_total": 0,
  "level": 0,
  "nickname": "...",
  "covers": [{ "id": 0, "image": "..." }]
}
```

### `GET /gcg/cardList`

固有クエリ:

```text
need_avatar=true|false
need_action=true|false
offset=<0始まり>
limit=<件数>
need_stats=false
```

```json
{
  "card_list": [
    {
      "id": 0,
      "card_type": "...",
      "name": "...",
      "use_count": 0,
      "tags": [],
      "card_wiki": "..."
    }
  ]
}
```

キャラクターカードには`hp`と`card_skills`、アクションカードには`action_cost`と`desc`が追加されます。

## カレンダー

### `POST /act_calendar`

リクエストボディは共通の`role_id`と`server`のみです。

```json
{
  "avatar_card_pool_list": [],
  "weapon_card_pool_list": [],
  "mixed_card_pool_list": [],
  "act_list": [],
  "fixed_act_list": []
}
```

祈願プールには`pool_id`、`version_name`、開始・終了時刻、対象キャラクター／武器が入り、イベントには`id`、`name`、`desc`、期間、報酬一覧、進行状態が入ります。

