use super::Passkeys;
use crate::dashboard::{
    auth::{
        accounts::Accounts,
        password::token_hash,
        sessions::{Identity, Sessions},
    },
    test_support::{
        PASSWORD, TEMP_PASSWORD, TestDatabase,
        webauthn::{ORIGIN, RP_ID, TestAuthenticator},
    },
};
use uuid::Uuid;

pub(super) struct Fixture {
    pub db: TestDatabase,
    pub accounts: Accounts,
    pub actor: Identity,
    pub passkeys: Passkeys,
    pub authenticator: TestAuthenticator,
    pub key_id: Uuid,
}

impl Fixture {
    pub async fn new() -> Self {
        let db = TestDatabase::new().await;
        let (accounts, actor) = db.ready_admin().await;
        let passkeys = Passkeys::new(RP_ID, ORIGIN, None).unwrap();
        let mut authenticator = TestAuthenticator::new();
        let challenge = passkeys
            .registration_start(&accounts, &actor, "test key".into())
            .await
            .unwrap();
        let response = authenticator.register(&challenge, 1);
        let key_id = passkeys
            .registration_finish(&accounts, &actor, challenge.challenge_id, response)
            .await
            .unwrap()
            .id;
        Self {
            db,
            accounts,
            actor,
            passkeys,
            authenticator,
            key_id,
        }
    }
}

pub(super) async fn ready_user(accounts: &Accounts, admin: &Identity, username: &str) -> Identity {
    accounts
        .create_user(admin, username.into(), TEMP_PASSWORD.into())
        .await
        .unwrap();
    let (_, session) = accounts
        .login(username.into(), TEMP_PASSWORD.into())
        .await
        .unwrap();
    let actor = Sessions::authenticate(&accounts.db, &token_hash(&session.token).unwrap())
        .await
        .unwrap();
    let (_, session) = accounts
        .change_password(&actor, Some(TEMP_PASSWORD.into()), PASSWORD.into())
        .await
        .unwrap();
    Sessions::authenticate(&accounts.db, &token_hash(&session.token).unwrap())
        .await
        .unwrap()
}
