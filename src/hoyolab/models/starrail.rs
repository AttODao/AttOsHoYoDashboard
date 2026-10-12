use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use super::common::FieldLink;

#[derive(Debug, Serialize, Deserialize)]
pub struct Index {
    pub stats: Stats,
    pub avatar_list: Vec<Avatar>,
    pub cur_head_icon_url: String,
    pub phone_background_image_url: String,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Stats {
    pub active_days: i64,
    pub avatar_num: i64,
    pub achievement_num: i64,
    pub chest_num: i64,
    pub abyss_process: String,
    pub field_ext_map: HashMap<String, FieldLink>,
    pub dream_paster_num: i64,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Avatar {
    pub id: i64,
    pub level: i64,
    pub name: String,
    pub element: String,
    pub icon: String,
    pub rarity: i64,
    pub rank: i64,
    pub is_chosen: bool,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DailyNote {
    pub current_stamina: i64,
    pub max_stamina: i64,
    pub stamina_recover_time: i64,
    /// The misspelling is the API's actual field name.
    pub accepted_epedition_num: i64,
    pub total_expedition_num: i64,
    pub expeditions: Vec<Expedition>,
    pub current_train_score: i64,
    pub max_train_score: i64,
    pub current_rogue_score: i64,
    pub max_rogue_score: i64,
    pub weekly_cocoon_cnt: i64,
    pub weekly_cocoon_limit: i64,
    pub current_reserve_stamina: i64,
    pub is_reserve_stamina_full: bool,
    /// Divergent Universe extensions are absent from the old response schema.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rogue_tourn_weekly_unlocked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rogue_tourn_weekly_max: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rogue_tourn_weekly_cur: Option<i64>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Expedition {
    pub avatars: Vec<String>,
    pub status: String,
    pub remaining_time: i64,
    pub name: String,
    pub item_url: String,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DailyInfo {
    pub total_sign_day: i64,
    pub is_sign: bool,
    pub sign_cnt_missed: i64,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}
