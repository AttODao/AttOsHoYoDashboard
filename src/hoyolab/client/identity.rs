use serde::Deserialize;
use serde_json::{Value, value::RawValue};

use super::{Error, HoyolabClient, Result, send};

pub(super) const ACCOUNT_IDENTITY_URL: &str =
    "https://api-account-os.hoyolab.com/auth/api/getUserAccountInfoByLToken";
pub(super) const COMMUNITY_IDENTITY_URL: &str =
    "https://bbs-api-os.hoyolab.com/community/painter/wapi/user/full";

const INVALID_ACCOUNT_ID: &str = "レスポンスdataのaccount_idが不正です";
const INVALID_COMMUNITY_UID: &str = "レスポンスdataのuser_info.uidが不正です";

impl HoyolabClient {
    /// Candidate authenticated account identity from HoYoLAB's account API.
    ///
    /// This is a response candidate only; it is not proof that a cookie belongs to a
    /// dashboard user or to any game UID.
    pub async fn account_identity(&self) -> Result<String> {
        let data = send(
            self.http
                .get(ACCOUNT_IDENTITY_URL)
                .header(reqwest::header::COOKIE, self.cookie.clone()),
        )
        .await?;
        parse_account_identity(&data)
    }

    /// Candidate authenticated community identity from HoYoLAB's BBS API.
    ///
    /// This is a response candidate only; it is not proof that a cookie belongs to a
    /// dashboard user or to any game UID.
    pub async fn community_identity(&self) -> Result<String> {
        let data = self.get(COMMUNITY_IDENTITY_URL, &[], None).await?;
        parse_community_identity(&data)
    }
}

fn parse_account_identity(data: &RawValue) -> Result<String> {
    #[derive(Deserialize)]
    struct AccountIdentity {
        account_id: Option<String>,
    }

    let identity: AccountIdentity =
        serde_json::from_str(data.get()).map_err(|_| invalid_data(INVALID_ACCOUNT_ID))?;
    let account_id = identity.account_id.ok_or(Error::MissingData)?;
    canonical_positive_u64(&account_id, INVALID_ACCOUNT_ID)
}

fn parse_community_identity(data: &RawValue) -> Result<String> {
    let value: Value =
        serde_json::from_str(data.get()).map_err(|_| invalid_data(INVALID_COMMUNITY_UID))?;
    let uid = value
        .get("user_info")
        .and_then(|user_info| user_info.get("uid"))
        .ok_or(Error::MissingData)?;
    match uid {
        Value::String(value) => canonical_positive_u64(value, INVALID_COMMUNITY_UID),
        Value::Number(value) => value
            .as_u64()
            .filter(|uid| *uid > 0)
            .map(|uid| uid.to_string())
            .ok_or_else(|| invalid_data(INVALID_COMMUNITY_UID)),
        _ => Err(invalid_data(INVALID_COMMUNITY_UID)),
    }
}

fn canonical_positive_u64(value: &str, detail: &'static str) -> Result<String> {
    value
        .parse::<u64>()
        .ok()
        .filter(|id| *id > 0)
        .map(|id| id.to_string())
        .ok_or_else(|| invalid_data(detail))
}

fn invalid_data(detail: &'static str) -> Error {
    Error::InvalidData(detail.to_owned())
}
