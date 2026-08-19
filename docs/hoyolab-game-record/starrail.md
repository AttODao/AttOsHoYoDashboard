# 崩壊：スターレイル ゲームレコードAPI

ベースURL:

```text
https://bbs-api-os.hoyolab.com/game_record/hkrpg/api
```

共通のリクエスト・レスポンス形式は [common.md](common.md) を参照してください。以下のJSONはすべて、生レスポンスの`data`部分を省略して示しています。

## エンドポイント一覧

| パス | HTTPメソッド | 固有引数 | 主な用途 |
| --- | --- | --- | --- |
| `/note` | GET | なし | 開拓力、依頼、予備開拓力などのリアルタイム情報 |
| `/index` | GET | なし | 基本戦績とキャラクター概要 |
| `/role/basicInfo` | GET | なし | プレイヤー基本情報 |
| `/avatar/info` | GET | `need_wiki=true` | キャラクター、光円錐、遺物の詳細 |
| `/challenge` | GET | `schedule_type`、`need_all=true` | 忘却の庭／混沌の記憶系の記録 |
| `/rogue` | GET | `schedule_type`、`need_detail=true` | 模擬宇宙系の記録 |
| `/challenge_story` | GET | `schedule_type`、`need_all=true` | 虚構叙事の記録 |
| `/challenge_boss` | GET | `schedule_type`、`need_all=true` | 末日の幻影の記録 |
| `/get_act_calender` | GET | なし | イベント・跳躍カレンダー |
| `/challenge_peak` | GET | `schedule_type` | Anomaly Arbitrationの記録 |

`get_act_calender`の`calender`はAPI上の綴りです。`calendar`へ置き換えないでください。

## リアルタイム情報とプロフィール

### `GET /note`

```json
{
  "current_stamina": 0,
  "max_stamina": 300,
  "stamina_recover_time": 0,
  "accepted_epedition_num": 0,
  "total_expedition_num": 0,
  "expeditions": [
    {
      "avatars": ["..."],
      "status": "Ongoing",
      "remaining_time": 0,
      "name": "...",
      "item_url": "..."
    }
  ],
  "current_train_score": 0,
  "max_train_score": 0,
  "current_rogue_score": 0,
  "max_rogue_score": 0,
  "rogue_tourn_weekly_unlocked": false,
  "rogue_tourn_weekly_max": 0,
  "rogue_tourn_weekly_cur": 0,
  "weekly_cocoon_cnt": 0,
  "weekly_cocoon_limit": 0,
  "current_reserve_stamina": 0,
  "is_reserve_stamina_full": false
}
```

`stamina_recover_time`と`remaining_time`は、参照実装では残り時間として扱われます。本人のリアルタイムノート設定が非公開だと取得できないことがあります。

### `GET /index`

```json
{
  "stats": {
    "active_days": 0,
    "avatar_num": 0,
    "achievement_num": 0,
    "chest_num": 0,
    "abyss_process": "...",
    "dream_paster_num": 0
  },
  "avatar_list": [{ "id": 0, "name": "...", "level": 0 }],
  "cur_head_icon_url": "...",
  "phone_background_image_url": "..."
}
```

`/index`にはプレイヤー名・サーバー・開拓レベルが含まれないため、参照実装は次の`/role/basicInfo`を併用します。

### `GET /role/basicInfo`

```json
{
  "nickname": "...",
  "region": "...",
  "level": 0,
  "avatar": "..."
}
```

## キャラクター詳細

### `GET /avatar/info`

通常は`need_wiki=true`をクエリに付けます。

```json
{
  "avatar_list": [
    {
      "id": 0,
      "name": "...",
      "level": 0,
      "equip": {},
      "relics": [],
      "ornaments": [],
      "skills": [],
      "properties": [],
      "servant_detail": {}
    }
  ],
  "equip_wiki": {},
  "relic_wiki": {},
  "property_info": {},
  "recommend_property": {},
  "relic_properties": []
}
```

`avatar_list`は、キャラクター、光円錐、遺物／次元界オーナメント、軌跡・ステータスを含みます。詳細なキャラクター情報を取得する用途では、このエンドポイントが中心になります。

## 挑戦コンテンツ

`/challenge`、`/challenge_story`、`/challenge_boss`は、いずれも現在期／前期を次の共通形式で指定します。

| 値 | 対象 |
| ---: | --- |
| `schedule_type=1` | 現在期 |
| `schedule_type=2` | 前期 |

いずれも`need_all=true`を付けます。

### `GET /challenge`

```json
{
  "name": "...",
  "schedule_id": 0,
  "begin_time": {},
  "end_time": {},
  "star_num": 0,
  "extra_star_num": 0,
  "max_floor": "...",
  "max_floor_id": 0,
  "battle_num": 0,
  "has_data": false,
  "all_floor_detail": [
    { "maze_id": 0, "name": "...", "round_num": 0, "node_1": {}, "node_2": {} }
  ],
  "groups": []
}
```

各ノードには挑戦日時と編成キャラクターが入ります。

### `GET /challenge_story`

虚構叙事の記録です。基本形は`/challenge`と同じですが、各フロアノードに`score`と`buff`が追加されます。

```json
{
  "star_num": 0,
  "extra_star_num": 0,
  "all_floor_detail": [
    { "maze_id": 0, "round_num": 0, "node_1": { "score": 0, "buff": {} } }
  ],
  "groups": [],
  "has_data": false
}
```

### `GET /challenge_boss`

末日の幻影の記録です。`/challenge_story`と同様のフロア構造に加えて、ノードごとに`boss_defeated`、シーズンごとに上・下・追加ボス情報が入ります。

```json
{
  "star_num": 0,
  "extra_star_num": 0,
  "all_floor_detail": [
    { "maze_id": 0, "node_1": { "boss_defeated": false, "score": 0 } }
  ],
  "groups": [
    { "upper_boss": {}, "lower_boss": {}, "tierce_boss": {} }
  ],
  "has_data": false
}
```

### `GET /challenge_peak`

参照実装のモデル名は`AnomalyArbitration`です。`schedule_type=3`は直近3件、`schedule_type=1`は現在の記録を返す扱いです。

```json
{
  "challenge_peak_records": [
    {
      "group": { "group_id": 0, "name_mi18n": "..." },
      "boss_info": {},
      "mob_infos": [],
      "has_challenge_record": false,
      "battle_num": 0,
      "boss_record": {},
      "mob_records": [],
      "boss_stars": 0,
      "mob_stars": 0
    }
  ],
  "challenge_peak_best_record_brief": {},
  "role": { "role_id": "...", "nickname": "...", "server": "...", "level": 0 }
}
```

## 模擬宇宙系

### `GET /rogue`

参照実装の既定値は`schedule_type=3&need_detail=true`です。

```json
{
  "role": { "nickname": "...", "server": "...", "level": 0 },
  "basic_info": {
    "unlocked_buff_num": 0,
    "unlocked_miracle_num": 0,
    "unlocked_skill_points": 0
  },
  "current_record": { "basic": {}, "records": [], "has_data": false },
  "last_record": { "basic": {}, "records": [], "has_data": false }
}
```

各`records[]`には、最終編成、祝福、奇物、難易度、進行度、スコア、終了日時が入ります。

## イベント・跳躍カレンダー

### `GET /get_act_calender`

```json
{
  "avatar_card_pool_list": [],
  "equip_card_pool_list": [],
  "act_list": [],
  "challenge_list": [],
  "cur_game_version": "...",
  "now": 0
}
```

キャラクター跳躍・光円錐跳躍には対象アイテム、期間、バージョンが入ります。イベント・挑戦には名称、説明、進行度、報酬、期間、状態が入ります。
