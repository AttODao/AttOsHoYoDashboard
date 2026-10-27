use super::*;
use crate::dashboard::{
    auth::api_tokens::{self, ApiTokens, Scope},
    db::entities::audit,
    test_support::TestDatabase,
};
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, sea_query::Expr};

const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

#[tokio::test]
async fn rfc_vector_issues_new_authenticated_session_records_audit_and_is_single_use() {
    let test = TestDatabase::new().await;
    let (accounts, actor) = test.ready_admin().await;
    let grants = AppLogins::new();
    let before = audit::Entity::find()
        .filter(audit::Column::Action.eq("signed_in"))
        .count(&test.db)
        .await
        .unwrap();
    let code = grants
        .approve(&accounts, &actor, CHALLENGE.to_owned())
        .await
        .unwrap();
    assert_eq!(code.len(), 64);
    assert!(
        code.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    let (user, session) = grants.exchange(&accounts, &code, VERIFIER).await.unwrap();
    assert_eq!(user.id, actor.user.id);
    let hash = password::token_hash(&session.token).unwrap();
    assert_ne!(hash, actor.token_hash);
    assert_eq!(
        Sessions::authenticate(&test.db, &hash)
            .await
            .unwrap()
            .user
            .id,
        user.id
    );
    assert_eq!(
        audit::Entity::find()
            .filter(audit::Column::Action.eq("signed_in"))
            .count(&test.db)
            .await
            .unwrap(),
        before + 1
    );
    assert!(matches!(
        grants.exchange(&accounts, &code, VERIFIER).await,
        Err(Error::Unauthorized)
    ));
}

#[tokio::test]
async fn wrong_and_malformed_verifiers_consume_the_code() {
    let test = TestDatabase::new().await;
    let (accounts, actor) = test.ready_admin().await;
    let grants = AppLogins::new();
    for verifier in [
        "a".repeat(43),
        "a".repeat(42),
        "a".repeat(129),
        format!("{}+", "a".repeat(42)),
        format!("{}=", "a".repeat(42)),
    ] {
        let code = grants
            .approve(&accounts, &actor, CHALLENGE.to_owned())
            .await
            .unwrap();
        assert!(matches!(
            grants.exchange(&accounts, &code, &verifier).await,
            Err(Error::Unauthorized)
        ));
        assert!(matches!(
            grants.exchange(&accounts, &code, VERIFIER).await,
            Err(Error::Unauthorized)
        ));
    }
}

#[tokio::test]
async fn expired_codes_and_malformed_challenges_and_codes_are_rejected() {
    let test = TestDatabase::new().await;
    let (accounts, actor) = test.ready_admin().await;
    let grants = AppLogins::with_ttl(Duration::ZERO);
    let code = grants
        .approve(&accounts, &actor, CHALLENGE.to_owned())
        .await
        .unwrap();
    assert!(matches!(
        grants.exchange(&accounts, &code, VERIFIER).await,
        Err(Error::Unauthorized)
    ));
    for challenge in [
        "a".repeat(42),
        format!("{}+", "a".repeat(42)),
        format!("{}=", "a".repeat(42)),
    ] {
        assert!(matches!(
            grants.approve(&accounts, &actor, challenge).await,
            Err(Error::Invalid(_))
        ));
    }
    for code in ["", "bad", &"g".repeat(64)] {
        assert!(matches!(
            grants.exchange(&accounts, code, VERIFIER).await,
            Err(Error::Unauthorized)
        ));
    }
}

#[tokio::test]
async fn api_token_actor_is_forbidden() {
    let test = TestDatabase::new().await;
    let (accounts, actor) = test.ready_admin().await;
    let token = ApiTokens::issue(&test.db, &actor, "synthetic app", Scope::Run)
        .await
        .unwrap();
    let actor = ApiTokens::authenticate(&test.db, &api_tokens::hash(&token.token).unwrap())
        .await
        .unwrap();
    assert!(matches!(
        AppLogins::new()
            .approve(&accounts, &actor, CHALLENGE.to_owned())
            .await,
        Err(Error::Forbidden)
    ));
}

#[tokio::test]
async fn revoked_browser_and_disabled_user_invalidate_grants_and_approval() {
    let test = TestDatabase::new().await;
    let (accounts, actor) = test.ready_admin().await;
    let grants = AppLogins::new();
    let code = grants
        .approve(&accounts, &actor, CHALLENGE.to_owned())
        .await
        .unwrap();
    Sessions::revoke_all(&test.db, actor.user.id).await.unwrap();
    assert!(matches!(
        grants.exchange(&accounts, &code, VERIFIER).await,
        Err(Error::Unauthorized)
    ));
    assert!(matches!(
        grants
            .approve(&accounts, &actor, CHALLENGE.to_owned())
            .await,
        Err(Error::Unauthorized)
    ));
    let session = accounts
        .sessions
        .issue(&test.db, actor.user.id)
        .await
        .unwrap();
    let actor = Sessions::authenticate(&test.db, &password::token_hash(&session.token).unwrap())
        .await
        .unwrap();
    let code = grants
        .approve(&accounts, &actor, CHALLENGE.to_owned())
        .await
        .unwrap();
    user::Entity::update_many()
        .col_expr(user::Column::Active, Expr::value(false))
        .filter(user::Column::Id.eq(actor.user.id))
        .exec(&test.db)
        .await
        .unwrap();
    assert!(matches!(
        grants.exchange(&accounts, &code, VERIFIER).await,
        Err(Error::Unauthorized)
    ));
    assert!(matches!(
        grants
            .approve(&accounts, &actor, CHALLENGE.to_owned())
            .await,
        Err(Error::Unauthorized)
    ));
}

#[tokio::test]
async fn pending_limit_is_busy_and_expired_entries_are_purged() {
    let test = TestDatabase::new().await;
    let (accounts, actor) = test.ready_admin().await;
    let grants = AppLogins::new();
    for _ in 0..MAX_PENDING {
        grants
            .approve(&accounts, &actor, CHALLENGE.to_owned())
            .await
            .unwrap();
    }
    assert!(matches!(
        grants
            .approve(&accounts, &actor, CHALLENGE.to_owned())
            .await,
        Err(Error::Busy)
    ));
    for grant in grants.pending.lock().unwrap().values_mut() {
        grant.expires_at = Instant::now();
    }
    assert!(
        grants
            .approve(&accounts, &actor, CHALLENGE.to_owned())
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn mandatory_password_change_does_not_block_app_sign_in() {
    let test = TestDatabase::new().await;
    let (accounts, actor) = test.ready_admin().await;
    user::Entity::update_many()
        .col_expr(user::Column::MustChangePassword, Expr::value(true))
        .filter(user::Column::Id.eq(actor.user.id))
        .exec(&test.db)
        .await
        .unwrap();
    let grants = AppLogins::new();
    let code = grants
        .approve(&accounts, &actor, CHALLENGE.to_owned())
        .await
        .unwrap();
    let (user, _) = grants.exchange(&accounts, &code, VERIFIER).await.unwrap();
    assert!(user.must_change_password);
}
