//! Small shared fixtures for redemption-code tests: synthetic users, links and UIDs, and a
//! local stand-in for the code source.
use crate::dashboard::{
    auth::{
        password::token_hash,
        sessions::{Identity, Sessions},
    },
    db::entities::{game_account, hoyolab_account, user},
    hoyolab::credentials::{CookieKey, Credentials},
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

/// A link in `status` (`verified`, `cookie_invalid`, …) with a sealed synthetic Cookie.
pub(in crate::dashboard) async fn link(
    db: &DatabaseConnection,
    owner: Uuid,
    uid: &str,
    status: &str,
) -> Uuid {
    let key = CookieKey::from_hex(&"11".repeat(32)).unwrap();
    let cookie =
        Credentials::parse(&format!("ltoken_v2=synthetictoken{uid}; ltuid_v2={uid}")).unwrap();
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
