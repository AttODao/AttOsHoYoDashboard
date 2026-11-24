//! Small shared fixtures for redemption-code tests: synthetic users, links and UIDs, a
//! local stand-in for the code source and a scripted stand-in for HoYoLAB redemption.
use crate::dashboard::{
    auth::{
        password::token_hash,
        sessions::{Identity, Sessions},
    },
    db::entities::{game_account, hoyolab_account, user},
    hoyolab::{
        credentials::{CookieKey, Credentials},
        gateway::RedeemGateway,
    },
};
use attoshoyodashboard::{
    game::GameKind,
    hoyolab::{Error as UpstreamError, RedemptionAnswer, RedemptionRequest},
};
use axum::{
    Router,
    extract::{Query, State},
    http::StatusCode,
    routing::get,
};
use chrono::{Duration, Utc};
use reqwest::Url;
use sea_orm::{ActiveModelTrait, DatabaseConnection, Set};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

pub(in crate::dashboard) async fn ready_user(db: &DatabaseConnection, name: &str) -> Identity {
    let user = user::ActiveModel {
        id: Set(Uuid::new_v4()),
        username: Set(name.to_owned()),
        password_hash: Set("synthetic-private-hash".to_owned()),
        is_admin: Set(false),
        active: Set(true),
        must_change_password: Set(false),
        created_at: Set(Utc::now()),
        last_login_at: Set(None),
    }
    .insert(db)
    .await
    .unwrap();
    let issued = Sessions::new(Duration::days(7))
        .issue(db, user.id)
        .await
        .unwrap();
    Sessions::authenticate(db, &token_hash(&issued.token).unwrap())
        .await
        .unwrap()
}

/// A link in `status` (`verified`, `cookie_invalid`, …) with a sealed synthetic Cookie
/// without redemption credentials.
pub(in crate::dashboard) async fn link(
    db: &DatabaseConnection,
    owner: Uuid,
    uid: &str,
    status: &str,
) -> Uuid {
    sealed_link(db, owner, uid, status, false).await
}

/// A verified link whose synthetic Cookie carries `cookie_token_v2`.
pub(in crate::dashboard) async fn redeem_link(
    db: &DatabaseConnection,
    owner: Uuid,
    uid: &str,
) -> Uuid {
    sealed_link(db, owner, uid, "verified", true).await
}

/// The key the fixtures seal Cookies with.
pub(in crate::dashboard) fn test_key() -> CookieKey {
    CookieKey::from_hex(&"11".repeat(32)).unwrap()
}

async fn sealed_link(
    db: &DatabaseConnection,
    owner: Uuid,
    uid: &str,
    status: &str,
    redemption: bool,
) -> Uuid {
    let key = test_key();
    let extra = if redemption {
        format!("; cookie_token_v2=syntheticcookietoken{uid}; account_id_v2={uid}")
    } else {
        String::new()
    };
    let cookie = Credentials::parse(&format!(
        "ltoken_v2=synthetictoken{uid}; ltuid_v2={uid}{extra}"
    ))
    .unwrap();
    hoyolab_account::ActiveModel {
        id: Set(Uuid::new_v4()),
        owner_user_id: Set(owner),
        hoyolab_uid: Set(Some(uid.to_owned())),
        encrypted_credentials: Set(Some(key.seal(owner, Some(uid), &cookie).unwrap())),
        credential_version: Set(1),
        auth_status: Set(status.to_owned()),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
        verified_at: Set(Some(Utc::now())),
        redemption_credentials: Set(redemption),
        redemption_action: Set(None),
        redemption_action_at: Set(None),
    }
    .insert(db)
    .await
    .unwrap()
    .id
}

pub(in crate::dashboard) async fn game(
    db: &DatabaseConnection,
    link: Uuid,
    kind: &str,
    uid: &str,
) -> Uuid {
    game_account::ActiveModel {
        id: Set(Uuid::new_v4()),
        hoyolab_account_id: Set(link),
        game_kind: Set(kind.to_owned()),
        game_uid: Set(uid.to_owned()),
        region: Set("os_asia".to_owned()),
        visible: Set(true),
        detected: Set(true),
        last_detected_at: Set(Utc::now()),
    }
    .insert(db)
    .await
    .unwrap()
    .id
}

/// The source's answer per game name (`genshin`, `hkrpg`): status and body.
type Answers = Arc<Mutex<HashMap<String, (u16, String)>>>;

/// A local code source on 127.0.0.1; answers can change between runs.
pub(in crate::dashboard) struct MockSource {
    pub(in crate::dashboard) url: Url,
    answers: Answers,
    requests: Arc<Mutex<Vec<String>>>,
}

impl MockSource {
    pub(in crate::dashboard) async fn start() -> Self {
        let answers: Answers = Arc::default();
        let requests: Arc<Mutex<Vec<String>>> = Arc::default();
        let state = (answers.clone(), requests.clone());
        let router = Router::new()
            .route(
                "/codes",
                get(
                    |State((answers, requests)): State<(Answers, Arc<Mutex<Vec<String>>>)>,
                     Query(query): Query<HashMap<String, String>>| async move {
                        let game = query.get("game").cloned().unwrap_or_default();
                        requests.lock().unwrap().push(game.clone());
                        let (status, body) = answers
                            .lock()
                            .unwrap()
                            .get(&game)
                            .cloned()
                            .unwrap_or((404, String::new()));
                        (StatusCode::from_u16(status).unwrap(), body)
                    },
                ),
            )
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self {
            url: Url::parse(&format!("http://127.0.0.1:{port}/")).unwrap(),
            answers,
            requests,
        }
    }

    /// Lists `codes` (code, rewards) for `game` (the source's name).
    pub(in crate::dashboard) fn list(&self, game: &str, codes: &[(&str, &str)]) {
        let rows: Vec<Value> = codes
            .iter()
            .map(|(code, rewards)| json!({"code": code, "status": "OK", "game": game, "rewards": rewards}))
            .collect();
        self.answer(game, 200, &json!({"codes": rows, "game": game}).to_string());
    }

    pub(in crate::dashboard) fn answer(&self, game: &str, status: u16, body: &str) {
        self.answers
            .lock()
            .unwrap()
            .insert(game.to_owned(), (status, body.to_owned()));
    }

    /// Games requested so far, in order.
    pub(in crate::dashboard) fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

/// Pacing for tests: short waits, the production caps.
pub(in crate::dashboard) fn fast_tuning() -> crate::dashboard::codes::redeem::RedeemTuning {
    crate::dashboard::codes::redeem::RedeemTuning {
        spacing: std::time::Duration::from_millis(20),
        cooldown_wait: std::time::Duration::from_millis(40),
        ..Default::default()
    }
}

/// One redemption request the mock received.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::dashboard) struct Sent {
    pub(in crate::dashboard) game: String,
    pub(in crate::dashboard) uid: String,
    pub(in crate::dashboard) region: String,
    pub(in crate::dashboard) code: String,
    /// The Cookie header the request would carry (synthetic).
    pub(in crate::dashboard) cookie: String,
    pub(in crate::dashboard) at: std::time::Instant,
}

type Script = std::collections::VecDeque<Result<RedemptionAnswer, UpstreamError>>;

/// A scripted answer carrying `retcode`, classified like a real one.
pub(in crate::dashboard) fn answer(retcode: i64) -> Result<RedemptionAnswer, UpstreamError> {
    Ok(RedemptionAnswer::from_retcode(retcode))
}
type Hook = Box<dyn FnMut(&str) + Send>;

/// Scripted answers per code (in order; retcode 0 once a script runs out) and every
/// request received. Nothing leaves the process.
#[derive(Clone, Default)]
pub(in crate::dashboard) struct MockRedeem {
    answers: Arc<Mutex<HashMap<String, Script>>>,
    sent: Arc<Mutex<Vec<Sent>>>,
    /// Runs before each answer (e.g. to change the database mid-run).
    hook: Arc<Mutex<Option<Hook>>>,
}

impl MockRedeem {
    pub(in crate::dashboard) fn new() -> Self {
        Self::default()
    }

    /// Answers for `code`, one per request, in order.
    pub(in crate::dashboard) fn script(
        &self,
        code: &str,
        answers: impl IntoIterator<Item = Result<RedemptionAnswer, UpstreamError>>,
    ) -> &Self {
        self.answers
            .lock()
            .unwrap()
            .entry(code.to_owned())
            .or_default()
            .extend(answers);
        self
    }

    pub(in crate::dashboard) fn on_request(&self, hook: impl FnMut(&str) + Send + 'static) {
        *self.hook.lock().unwrap() = Some(Box::new(hook));
    }

    pub(in crate::dashboard) fn sent(&self) -> Vec<Sent> {
        self.sent.lock().unwrap().clone()
    }

    /// The codes requested, in order.
    pub(in crate::dashboard) fn codes(&self) -> Vec<String> {
        self.sent().into_iter().map(|sent| sent.code).collect()
    }
}

impl RedeemGateway for MockRedeem {
    async fn redeem(
        &self,
        cookie: &Credentials,
        request: RedemptionRequest<'_>,
    ) -> Result<RedemptionAnswer, UpstreamError> {
        if let Some(hook) = self.hook.lock().unwrap().as_mut() {
            hook(request.code);
        }
        self.sent.lock().unwrap().push(Sent {
            game: match request.game {
                GameKind::Genshin(_) => "genshin",
                GameKind::Starrail(_) => "starrail",
            }
            .to_owned(),
            uid: request.uid.to_owned(),
            region: request.region.to_owned(),
            code: request.code.to_owned(),
            cookie: cookie.redemption().unwrap_or_default().to_owned(),
            at: std::time::Instant::now(),
        });
        self.answers
            .lock()
            .unwrap()
            .get_mut(request.code)
            .and_then(std::collections::VecDeque::pop_front)
            .unwrap_or_else(|| answer(0))
    }
}
