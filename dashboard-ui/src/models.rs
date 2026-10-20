use serde::{Deserialize, Serialize};
#[cfg(target_arch = "wasm32")]
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct UserView {
    pub id: Uuid,
    pub username: String,
    pub is_admin: bool,
    pub active: bool,
    pub must_change_password: bool,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct MeView {
    pub id: Uuid,
    pub username: String,
    pub is_admin: bool,
    pub active: bool,
    pub must_change_password: bool,
    pub onboarding: Onboarding,
    pub capabilities: Capabilities,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Onboarding {
    pub password_change_required: bool,
    pub cookie_setup_complete: bool,
    pub cookie_status: String,
    pub verification_available: bool,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Capabilities {
    pub reward_posting_available: bool,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct CookieStatus {
    pub id: Option<Uuid>,
    pub hoyolab_uid: Option<String>,
    pub status: String,
    pub setup_complete: bool,
    pub updated_at: Option<String>,
    #[serde(default)]
    pub verified_at: Option<String>,
    pub verification_available: bool,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct GameAccount {
    pub id: Uuid,
    pub hoyolab_account_id: Uuid,
    pub game_kind: String,
    pub game_uid: String,
    pub region: String,
    pub visible: bool,
    pub detected: bool,
    pub last_detected_at: String,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct GameStatus {
    pub game_kind: String,
    pub game_uid: String,
    pub region: String,
    pub visible: bool,
    pub detected: bool,
    pub data: Option<DailySummary>,
    pub fetched_at: Option<String>,
    pub error_code: Option<String>,
    pub error_at: Option<String>,
    pub stale: bool,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct DailySummary {
    pub current_resin: Option<i64>,
    pub max_resin: Option<i64>,
    pub current_home_coin: Option<i64>,
    pub max_home_coin: Option<i64>,
    pub finished_task_num: Option<i64>,
    pub total_task_num: Option<i64>,
    pub is_extra_task_reward_received: Option<bool>,
    pub resin_full_at: Option<String>,
    pub home_coin_full_at: Option<String>,
    pub current_stamina: Option<i64>,
    pub max_stamina: Option<i64>,
    pub current_reserve_stamina: Option<i64>,
    pub current_train_score: Option<i64>,
    pub max_train_score: Option<i64>,
    pub stamina_full_at: Option<String>,
    pub expeditions: Option<Vec<Expedition>>,
    pub remain_resin_discount_num: Option<i64>,
    pub resin_discount_num_limit: Option<i64>,
    pub current_expedition_num: Option<i64>,
    pub max_expedition_num: Option<i64>,
    pub transformer_obtained: Option<bool>,
    pub transformer_ready: Option<bool>,
    pub transformer_ready_at: Option<String>,
    pub current_rogue_score: Option<i64>,
    pub max_rogue_score: Option<i64>,
    pub weekly_cocoon_cnt: Option<i64>,
    pub weekly_cocoon_limit: Option<i64>,
    pub is_reserve_stamina_full: Option<bool>,
    pub rogue_tourn_weekly_unlocked: Option<bool>,
    pub rogue_tourn_weekly_cur: Option<i64>,
    pub rogue_tourn_weekly_max: Option<i64>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Expedition {
    pub status: Option<String>,
    pub name: Option<String>,
    pub avatar_side_icon: Option<String>,
    pub item_url: Option<String>,
    pub remained_time: Option<ExpeditionDuration>,
    pub remaining_time: Option<ExpeditionDuration>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExpeditionDuration {
    Text(String),
    Seconds(i64),
}

impl ExpeditionDuration {
    /// Whole seconds when upstream reports a number or a decimal string.
    pub fn seconds(&self) -> Option<i64> {
        match self {
            Self::Text(text) => text.trim().parse().ok(),
            Self::Seconds(seconds) => Some(*seconds),
        }
    }
}

impl std::fmt::Display for ExpeditionDuration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text(text) => f.write_str(text),
            Self::Seconds(seconds) => write!(f, "{seconds}s"),
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct RewardSetting {
    pub id: Uuid,
    pub game_kind: String,
    pub enabled: bool,
    pub local_time: String,
    pub auth_status: String,
    pub next_run_at: Option<String>,
    pub last_run: Option<LastRun>,
    pub is_sign: Option<bool>,
    #[serde(default)]
    pub total_sign_day: Option<i64>,
    #[serde(default)]
    pub today_award: Option<DailyAward>,
    pub last_checked_at: Option<String>,
    pub error_code: Option<String>,
    pub error_at: Option<String>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct DailyAward {
    pub name: String,
    pub count: i64,
    pub icon: String,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct LastRun {
    pub id: Uuid,
    pub state: String,
    pub ended_at: Option<String>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct RewardRun {
    pub id: Uuid,
    pub setting_id: Uuid,
    pub game_kind: String,
    pub trigger: String,
    pub state: String,
    pub created_at: String,
    pub ended_at: Option<String>,
    pub attempt_count: i32,
    pub attempts: Vec<RewardAttempt>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct RewardAttempt {
    pub number: i32,
    pub outcome: String,
    pub error_code: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettings {
    pub timezone_offset: String,
    pub session_ttl_days: i64,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct PasskeyView {
    pub id: Uuid,
    pub label: String,
    pub created_at: String,
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct PasskeyChallenge {
    pub challenge_id: Uuid,
    pub options: Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn me_view_is_flat_and_parses_onboarding_capabilities() {
        let value = json!({
            "id":"018f4f5a-0b01-7cc1-9e11-111111111111",
            "username":"admin",
            "is_admin":true,
            "active":true,
            "must_change_password":false,
            "onboarding":{
                "password_change_required":false,
                "cookie_setup_complete":true,
                "cookie_status":"verified",
                "verification_available":true
            },
            "capabilities":{"reward_posting_available":false}
        });
        let parsed: MeView = serde_json::from_value(value).unwrap();
        assert_eq!(parsed.username, "admin");
        assert!(!parsed.must_change_password);
        assert_eq!(parsed.onboarding.cookie_status, "verified");
        assert!(!parsed.capabilities.reward_posting_available);
    }

    #[test]
    fn daily_summary_uses_backend_field_names_only() {
        let value = json!({
            "current_resin": 12,
            "max_resin": 160,
            "current_home_coin": 500,
            "max_home_coin": 2400,
            "finished_task_num": 3,
            "total_task_num": 4,
            "is_extra_task_reward_received": false,
            "resin_full_at": "2026-01-01T00:00:00Z",
            "home_coin_full_at": "2026-01-02T00:00:00Z",
            "current_stamina": 20,
            "max_stamina": 240,
            "current_reserve_stamina": 1000,
            "current_train_score": 200,
            "max_train_score": 500,
            "stamina_full_at": "2026-01-03T00:00:00Z",
            "expeditions": [
                {"status":"Ongoing","name":"A","remained_time":"01:02:03","avatar_side_icon":"https://upload-os-bbs.hoyolab.com/synthetic-avatar.png"},
                {"status":"Finished","name":"B","remaining_time":0,"item_url":"https://act-upload.hoyolab.com/synthetic-item.png"}
            ]
        });
        let parsed: DailySummary = serde_json::from_value(value).unwrap();
        assert_eq!(parsed.current_reserve_stamina, Some(1000));
        let expeditions = parsed.expeditions.unwrap();
        assert_eq!(
            expeditions[0].avatar_side_icon.as_deref(),
            Some("https://upload-os-bbs.hoyolab.com/synthetic-avatar.png")
        );
        assert!(expeditions[0].item_url.is_none());
        assert_eq!(
            expeditions[1].item_url.as_deref(),
            Some("https://act-upload.hoyolab.com/synthetic-item.png")
        );
        assert!(expeditions[1].avatar_side_icon.is_none());
        assert_eq!(
            expeditions[0].remained_time.as_ref().unwrap().to_string(),
            "01:02:03"
        );
        assert_eq!(
            expeditions[1].remaining_time.as_ref().unwrap().to_string(),
            "0s"
        );
        for field in ["remained_time", "remaining_time"] {
            for (value, expected) in [(json!(120), "120s"), (json!("120"), "120")] {
                let expedition: Expedition = serde_json::from_value(json!({field: value})).unwrap();
                let duration = expedition
                    .remained_time
                    .or(expedition.remaining_time)
                    .unwrap();
                assert_eq!(duration.to_string(), expected);
                assert_eq!(duration.seconds(), Some(120));
            }
        }
        assert_eq!(ExpeditionDuration::Text("01:02:03".into()).seconds(), None);
    }

    #[test]
    fn reward_run_history_shape_parses_attempts() {
        let id = "018f4f5a-0b01-7cc1-9e11-222222222222";
        let value = json!({
            "id": id,
            "setting_id": "018f4f5a-0b01-7cc1-9e11-333333333333",
            "game_kind":"genshin",
            "trigger":"manual",
            "state":"done",
            "created_at":"2026-01-01T00:00:00Z",
            "ended_at":null,
            "attempt_count":1,
            "attempts":[{"number":1,"outcome":"ok","error_code":null,"started_at":"2026-01-01T00:00:00Z","finished_at":null}]
        });
        let parsed: RewardRun = serde_json::from_value(value).unwrap();
        assert_eq!(parsed.id.to_string(), id);
        assert_eq!(parsed.attempts[0].number, 1);
        assert_eq!(parsed.attempts[0].error_code, None);
    }
}
