use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Check-in result. Successful responses may be empty; additional verification
/// and game-specific fields must not be discarded.
#[derive(Debug, Serialize, Deserialize)]
pub struct DailySign {
    /// Additional verification can be requested even with envelope retcode 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gt_result: Option<RiskVerification>,
    /// Some responses place verification fields directly in `data`.
    #[serde(flatten)]
    pub verification: RiskVerification,
}

/// Known check-in risk fields. They are optional because the docs do not
/// guarantee this payload or every field; unknown verification data is retained.
#[derive(Debug, Serialize, Deserialize)]
pub struct RiskVerification {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_code: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub challenge: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success: Option<i64>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

impl DailySign {
    /// Whether known risk fields request verification. `None` means the
    /// response contains no recognized evidence, not that claiming succeeded.
    /// Callers must still inspect retcode and any unknown verification fields.
    pub fn requires_verification(&self) -> Option<bool> {
        let risk = self.gt_result.as_ref().unwrap_or(&self.verification);
        if risk
            .challenge
            .as_ref()
            .is_some_and(|value| !value.is_empty())
        {
            return Some(true);
        }
        risk.risk_code.map(|code| code != 0)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DailyHome {
    pub awards: Vec<Award>,
    /// Some check-in APIs include the month; it is not part of every response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub month: Option<i64>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Award {
    pub name: String,
    pub cnt: i64,
    pub icon: String,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}
