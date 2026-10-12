//! HoYoLAB response payloads, preserving the API's wire representations.

pub mod account;
pub mod common;
pub mod daily;
pub mod genshin;
pub mod starrail;

use serde::{Deserialize, Serialize};

/// The response envelope used by both game-record and check-in APIs.
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub retcode: i64,
    #[serde(default)]
    pub message: String,
    pub data: Option<T>,
}
