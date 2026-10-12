use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use super::common::FieldLink;

#[derive(Debug, Serialize, Deserialize)]
pub struct Index {
    pub role: Role,
    pub avatars: Vec<Avatar>,
    pub stats: Stats,
    /// The old model did not establish a city exploration schema.
    pub city_explorations: Vec<Value>,
    pub world_explorations: Vec<WorldExploration>,
    pub homes: Vec<Home>,
    pub query_tool_link: String,
    pub query_tool_image: String,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Role {
    #[serde(rename = "AvatarUrl")]
    pub avatar_url: String,
    pub nickname: String,
    pub region: String,
    pub level: i64,
    pub game_head_icon: String,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Avatar {
    pub id: i64,
    pub image: String,
    pub element: String,
    pub fetter: i64,
    pub level: i64,
    pub rarity: i64,
    pub actived_constellation_num: i64,
    pub card_image: String,
    pub is_chosen: bool,
    /// Added to newer character summaries, absent from the old schema.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Stats {
    pub active_day_number: i64,
    pub achievement_number: i64,
    pub anemoculus_number: i64,
    pub geoculus_number: i64,
    pub avatar_number: i64,
    pub way_point_number: i64,
    pub domain_number: i64,
    pub spiral_abyss: String,
    pub precious_chest_number: i64,
    pub luxurious_chest_number: i64,
    pub exquisite_chest_number: i64,
    pub common_chest_number: i64,
    pub electroculus_number: i64,
    pub magic_chest_number: i64,
    pub dendroculus_number: i64,
    pub hydroculus_number: i64,
    pub field_ext_map: HashMap<String, FieldLink>,
    /// Challenge summaries are newer than the old schema. Keep their nested
    /// fields intact rather than assuming the abbreviated documentation is full.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role_combat: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hard_challenge: Option<Value>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorldExploration {
    pub level: i64,
    pub exploration_percentage: i64,
    pub icon: String,
    pub name: String,
    pub r#type: String,
    pub offerings: Vec<Offering>,
    pub id: i64,
    pub parent_id: i64,
    pub map_url: String,
    pub strategy_url: String,
    pub background_image: String,
    pub inner_icon: String,
    pub cover: String,
    pub area_exploration_list: Vec<AreaExploration>,
    pub boss_list: Vec<Boss>,
    pub is_hot: bool,
    pub index_active: bool,
    pub detail_active: bool,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Offering {
    pub name: String,
    pub level: i64,
    pub icon: String,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AreaExploration {
    pub name: String,
    pub exploration_percentage: i64,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Boss {
    pub name: String,
    pub kill_num: i64,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Home {
    pub level: i64,
    pub visit_num: i64,
    pub comfort_num: i64,
    pub item_num: i64,
    pub name: String,
    pub icon: String,
    pub comfort_level_name: String,
    pub comfort_level_icon: String,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DailyNote {
    pub current_resin: i64,
    pub max_resin: i64,
    pub resin_recovery_time: String,
    pub finished_task_num: i64,
    pub total_task_num: i64,
    pub is_extra_task_reward_received: bool,
    pub remain_resin_discount_num: i64,
    pub resin_discount_num_limit: i64,
    pub current_expedition_num: i64,
    pub max_expedition_num: i64,
    pub expeditions: Vec<Expedition>,
    pub current_home_coin: i64,
    pub max_home_coin: i64,
    pub home_coin_recovery_time: String,
    /// Newer note extensions, absent from older responses. Nested wire fields
    /// remain unmodified until a complete schema is verified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transformer: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archon_quest_progress: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub daily_task: Option<Value>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Expedition {
    pub avatar_side_icon: String,
    pub status: String,
    pub remained_time: String,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DailyInfo {
    pub is_sign: bool,
    pub total_sign_day: i64,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}
