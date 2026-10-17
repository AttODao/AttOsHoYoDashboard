//! Typed upstream boundary; the same business workflows are tested with a local mock.
use super::credentials::Credentials;
use attoshoyodashboard::{
    game::{GameKind, Genshin, Starrail},
    hoyolab::{HoyolabClient, RecordAccount, Result},
    models::account::Account,
};
use serde_json::Value;
use std::future::Future;

pub(in crate::dashboard) struct DailyState {
    pub is_sign: bool,
    pub total_sign_day: i64,
}
pub(in crate::dashboard) struct SignState {
    pub requires_verification: Option<bool>,
}

pub(in crate::dashboard) trait Gateway: Send + Sync + 'static {
    fn identity(
        &self,
        cookie: &Credentials,
    ) -> impl Future<Output = Result<(String, String)>> + Send;
    fn roles(&self, cookie: &Credentials) -> impl Future<Output = Result<Vec<Account>>> + Send;
    fn note(
        &self,
        cookie: &Credentials,
        game: GameKind,
        account: &RecordAccount,
    ) -> impl Future<Output = Result<Value>> + Send;
    fn daily(
        &self,
        cookie: &Credentials,
        game: GameKind,
    ) -> impl Future<Output = Result<DailyState>> + Send;
    fn sign(
        &self,
        cookie: &Credentials,
        game: GameKind,
    ) -> impl Future<Output = Result<SignState>> + Send;
}

pub(in crate::dashboard) struct LiveGateway;
impl Gateway for LiveGateway {
    async fn identity(&self, cookie: &Credentials) -> Result<(String, String)> {
        let client = HoyolabClient::new(cookie.as_str())?;
        Ok((
            client.account_identity().await?,
            client.community_identity().await?,
        ))
    }
    async fn roles(&self, cookie: &Credentials) -> Result<Vec<Account>> {
        HoyolabClient::new(cookie.as_str())?.accounts().await
    }
    async fn note(
        &self,
        cookie: &Credentials,
        game: GameKind,
        account: &RecordAccount,
    ) -> Result<Value> {
        let client = HoyolabClient::new(cookie.as_str())?;
        match game {
            GameKind::Genshin(_) => {
                serde_json::to_value(client.daily_note(Genshin, account).await?)
            }
            GameKind::Starrail(_) => {
                serde_json::to_value(client.daily_note(Starrail, account).await?)
            }
        }
        .map_err(|_| "typed note serialization failed".into())
    }
    async fn daily(&self, cookie: &Credentials, game: GameKind) -> Result<DailyState> {
        let client = HoyolabClient::new(cookie.as_str())?;
        Ok(match game {
            GameKind::Genshin(_) => {
                let state = client.daily_info(Genshin).await?;
                DailyState {
                    is_sign: state.is_sign,
                    total_sign_day: state.total_sign_day,
                }
            }
            GameKind::Starrail(_) => {
                let state = client.daily_info(Starrail).await?;
                DailyState {
                    is_sign: state.is_sign,
                    total_sign_day: state.total_sign_day,
                }
            }
        })
    }
    async fn sign(&self, cookie: &Credentials, game: GameKind) -> Result<SignState> {
        let client = HoyolabClient::new(cookie.as_str())?;
        let state = match game {
            GameKind::Genshin(_) => client.daily_sign(Genshin).await?,
            GameKind::Starrail(_) => client.daily_sign(Starrail).await?,
        };
        Ok(SignState {
            requires_verification: state.requires_verification(),
        })
    }
}

pub(in crate::dashboard) fn game(raw: &str) -> crate::dashboard::error::Result<GameKind> {
    match raw {
        "genshin" => Ok(GameKind::Genshin(Genshin)),
        "starrail" => Ok(GameKind::Starrail(Starrail)),
        _ => Err(crate::dashboard::error::Error::Invalid("unknown game")),
    }
}

pub(in crate::dashboard) fn error_code(error: &attoshoyodashboard::hoyolab::Error) -> String {
    use attoshoyodashboard::hoyolab::Error::*;
    match error {
        Upstream { code } => format!("upstream:{code}"),
        Http(status) => format!("http:{status}"),
        Network | ResponseRead => "network".into(),
        _ => "upstream_data".into(),
    }
}
pub(in crate::dashboard) fn cookie_invalid(error: &attoshoyodashboard::hoyolab::Error) -> bool {
    matches!(
        error,
        attoshoyodashboard::hoyolab::Error::Upstream {
            code: -100 | 10001 | -1071
        }
    )
}
pub(in crate::dashboard) fn transient(error: &attoshoyodashboard::hoyolab::Error) -> bool {
    matches!(
        error,
        attoshoyodashboard::hoyolab::Error::Network
            | attoshoyodashboard::hoyolab::Error::ResponseRead
            | attoshoyodashboard::hoyolab::Error::Http(500..=599)
    )
}
