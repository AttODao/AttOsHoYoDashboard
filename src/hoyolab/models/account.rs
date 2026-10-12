use serde::{Deserialize, Serialize};

/// A linked game account returned by HoYoLAB's cookie binding API.
#[derive(Debug, Serialize, Deserialize)]
pub struct Account {
    pub game_biz: String,
    pub region: String,
    pub game_uid: String,
    #[serde(default)]
    pub nickname: String,
    #[serde(default)]
    pub level: i64,
}
