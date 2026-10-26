use super::{ApiTokens, Scope, hash, permits};
use crate::dashboard::{
    auth::{
        accounts::Accounts,
        password::token_hash,
        sessions::{Credential, Identity, Sessions, reauthenticate},
    },
    db::entities::{api_token, audit},
    error::Error,
    test_support::{PASSWORD, TEMP_PASSWORD, TestDatabase},
};
use axum::http::Method;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};

async fn ready_user(accounts: &Accounts, admin: &Identity, username: &str) -> Identity {
    accounts
        .create_user(admin, username.to_owned(), TEMP_PASSWORD.to_owned())
        .await
        .unwrap();
    let (_, session) = accounts
        .login(username.to_owned(), TEMP_PASSWORD.to_owned())
        .await
        .unwrap();
    let actor = Sessions::authenticate(&accounts.db, &token_hash(&session.token).unwrap())
        .await
        .unwrap();
    let (_, session) = accounts
        .change_password(&actor, Some(TEMP_PASSWORD.to_owned()), PASSWORD.to_owned())
        .await
        .unwrap();
    Sessions::authenticate(&accounts.db, &token_hash(&session.token).unwrap())
        .await
        .unwrap()
}

async fn token_identity(accounts: &Accounts, token: &str) -> Result<Identity, Error> {
    ApiTokens::authenticate(&accounts.db, &hash(token)?).await
}

async fn audits(accounts: &Accounts, action: &str) -> u64 {
    audit::Entity::find()
        .filter(audit::Column::Action.eq(action))
        .count(&accounts.db)
        .await
        .unwrap()
}

#[tokio::test]
async fn issuance_returns_the_plaintext_once_and_stores_only_its_hash() {
    let fixture = TestDatabase::new().await;
    let (accounts, admin) = fixture.ready_admin().await;
    let issued = ApiTokens::issue(&accounts.db, &admin, "  deck  ", Scope::Run)
        .await
        .unwrap();
    let secret = issued.token.strip_prefix("ahd_").unwrap();
    assert_eq!(secret.len(), 64);
    assert!(secret.bytes().all(|b| b.is_ascii_hexdigit()));
    let stored = api_token::Entity::find_by_id(issued.row.id)
        .one(&accounts.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.name, "deck");
    assert_eq!(stored.scope, "run");
    assert_eq!(stored.token_hash, hash(&issued.token).unwrap());
    assert!(!stored.token_hash.contains(secret));
    assert_eq!(stored.last_used_at, None);
    assert_eq!(audits(&accounts, "api_token_created").await, 1);

    let identity = token_identity(&accounts, &issued.token).await.unwrap();
    assert_eq!(identity.user.id, admin.user.id);
    assert!(matches!(
        identity.credential,
        Credential::ApiToken { id, scope: Scope::Run } if id == issued.row.id
    ));
    // The fresh re-authentication used by token-permitted operations accepts it ...
    assert!(reauthenticate(&accounts.db, &identity).await.is_ok());
    // ... while session-only operations never do.
    assert!(matches!(
        Sessions::authenticate(&accounts.db, &identity.token_hash).await,
        Err(Error::Unauthorized)
    ));
    assert!(matches!(
        identity.session_expires_at(),
        Err(Error::Forbidden)
    ));
}

#[tokio::test]
async fn names_scopes_and_the_per_user_limit_are_validated() {
    let fixture = TestDatabase::new().await;
    let (accounts, admin) = fixture.ready_admin().await;
    for name in ["", "   ", "line\nbreak", &"x".repeat(101)] {
        assert!(matches!(
            ApiTokens::issue(&accounts.db, &admin, name, Scope::Read).await,
            Err(Error::Invalid(_))
        ));
    }
    assert!(
        ApiTokens::issue(&accounts.db, &admin, &"あ".repeat(100), Scope::Read)
            .await
            .is_ok()
    );
    assert!(matches!(Scope::parse("write"), Err(Error::Invalid(_))));
    assert_eq!(Scope::parse("read").unwrap(), Scope::Read);
    for n in 1..10 {
        ApiTokens::issue(&accounts.db, &admin, &format!("tool {n}"), Scope::Read)
            .await
            .unwrap();
    }
    assert!(matches!(
        ApiTokens::issue(&accounts.db, &admin, "eleventh", Scope::Read).await,
        Err(Error::Invalid("too many API tokens"))
    ));
    let listed = ApiTokens::list(&accounts.db, admin.user.id).await.unwrap();
    assert_eq!(listed.len(), 10);
    assert!(
        listed
            .windows(2)
            .all(|pair| pair[0].created_at >= pair[1].created_at)
    );
}

#[tokio::test]
async fn tokens_cannot_manage_tokens_and_revocation_is_owner_scoped() {
    let fixture = TestDatabase::new().await;
    let (accounts, admin) = fixture.ready_admin().await;
    let other = ready_user(&accounts, &admin, "token-owner").await;
    let issued = ApiTokens::issue(&accounts.db, &other, "deck", Scope::Run)
        .await
        .unwrap();
    let token = token_identity(&accounts, &issued.token).await.unwrap();
    assert!(matches!(
        ApiTokens::issue(&accounts.db, &token, "nested", Scope::Read).await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        ApiTokens::revoke(&accounts.db, &token, issued.row.id).await,
        Err(Error::Forbidden)
    ));
    // Another user's token id is indistinguishable from a missing one.
    assert!(matches!(
        ApiTokens::revoke(&accounts.db, &admin, issued.row.id).await,
        Err(Error::NotFound)
    ));
    ApiTokens::revoke(&accounts.db, &other, issued.row.id)
        .await
        .unwrap();
    assert_eq!(audits(&accounts, "api_token_revoked").await, 1);
    assert!(matches!(
        token_identity(&accounts, &issued.token).await,
        Err(Error::Unauthorized)
    ));
    assert!(matches!(
        reauthenticate(&accounts.db, &token).await,
        Err(Error::Unauthorized)
    ));
}

#[tokio::test]
async fn malformed_tokens_are_unauthorized() {
    let fixture = TestDatabase::new().await;
    let (accounts, _) = fixture.ready_admin().await;
    for token in [
        "",
        "ahd_",
        "ahd_xyz",
        &"a".repeat(64),
        &format!("AHD_{}", "a".repeat(64)),
        &format!("ahd_{}", "a".repeat(64)),
    ] {
        assert!(matches!(
            token_identity(&accounts, token).await,
            Err(Error::Unauthorized)
        ));
    }
}

#[tokio::test]
async fn administrator_lifecycle_actions_revoke_tokens_but_own_password_change_keeps_them() {
    let fixture = TestDatabase::new().await;
    let (accounts, admin) = fixture.ready_admin().await;

    let user = ready_user(&accounts, &admin, "rotating-user").await;
    let kept = ApiTokens::issue(&accounts.db, &user, "kept", Scope::Read)
        .await
        .unwrap();
    accounts
        .change_password(
            &user,
            Some(PASSWORD.to_owned()),
            "another-permanent-password".to_owned(),
        )
        .await
        .unwrap();
    assert!(token_identity(&accounts, &kept.token).await.is_ok());

    accounts
        .reset_password(&admin, user.user.id, TEMP_PASSWORD.to_owned())
        .await
        .unwrap();
    assert!(matches!(
        token_identity(&accounts, &kept.token).await,
        Err(Error::Unauthorized)
    ));

    let disabled = ready_user(&accounts, &admin, "disabled-user").await;
    let issued = ApiTokens::issue(&accounts.db, &disabled, "deck", Scope::Read)
        .await
        .unwrap();
    accounts
        .set_active(&admin, disabled.user.id, false)
        .await
        .unwrap();
    accounts
        .set_active(&admin, disabled.user.id, true)
        .await
        .unwrap();
    assert!(matches!(
        token_identity(&accounts, &issued.token).await,
        Err(Error::Unauthorized)
    ));

    let deleted = ready_user(&accounts, &admin, "deleted-user").await;
    let issued = ApiTokens::issue(&accounts.db, &deleted, "deck", Scope::Read)
        .await
        .unwrap();
    accounts
        .request_deletion(
            &admin,
            deleted.user.id,
            PASSWORD.to_owned(),
            "deleted-user".to_owned(),
        )
        .await
        .unwrap();
    assert!(matches!(
        token_identity(&accounts, &issued.token).await,
        Err(Error::Unauthorized)
    ));
    assert_eq!(
        api_token::Entity::find().count(&accounts.db).await.unwrap(),
        0
    );
}

#[tokio::test]
async fn inactive_users_cannot_authenticate_with_a_token() {
    let fixture = TestDatabase::new().await;
    let (accounts, admin) = fixture.ready_admin().await;
    let user = ready_user(&accounts, &admin, "inactive-user").await;
    let issued = ApiTokens::issue(&accounts.db, &user, "deck", Scope::Read)
        .await
        .unwrap();
    // Deactivate directly, bypassing the lifecycle revocation, to test the user check itself.
    crate::dashboard::db::entities::user::Entity::update_many()
        .col_expr(
            crate::dashboard::db::entities::user::Column::Active,
            sea_orm::sea_query::Expr::value(false),
        )
        .filter(crate::dashboard::db::entities::user::Column::Id.eq(user.user.id))
        .exec(&accounts.db)
        .await
        .unwrap();
    assert!(matches!(
        token_identity(&accounts, &issued.token).await,
        Err(Error::Unauthorized)
    ));
}

#[test]
fn scope_policy_allows_only_dashboard_reads_and_the_manual_run() {
    let reads = [
        "/api/v1/me",
        "/api/v1/server-settings",
        "/api/v1/me/hoyolab",
        "/api/v1/game-accounts",
        "/api/v1/game-accounts/{id}/status",
        "/api/v1/reward-settings",
        "/api/v1/reward-runs",
    ];
    for scope in [Scope::Read, Scope::Run] {
        for route in reads {
            assert!(permits(scope, &Method::GET, route), "{route}");
            assert!(permits(scope, &Method::HEAD, route), "{route}");
            assert!(!permits(scope, &Method::POST, route), "{route}");
        }
        for (method, route) in [
            (Method::GET, "/api/v1/me/api-tokens"),
            (Method::POST, "/api/v1/me/api-tokens"),
            (Method::DELETE, "/api/v1/me/api-tokens/{id}"),
            (Method::GET, "/api/v1/me/sessions"),
            (Method::GET, "/api/v1/me/passkeys"),
            (Method::GET, "/api/v1/me/notifications"),
            (Method::GET, "/api/v1/admin/users"),
            (Method::GET, "/api/v1/admin/audit"),
            (Method::PATCH, "/api/v1/reward-settings/{id}"),
            (Method::PATCH, "/api/v1/game-accounts/{id}"),
            (Method::PUT, "/api/v1/me/password"),
            (Method::POST, "/api/v1/auth/logout"),
            (Method::POST, "/api/v1/me/hoyolab/sync"),
            (Method::DELETE, "/api/v1/me"),
        ] {
            assert!(!permits(scope, &method, route), "{method} {route}");
        }
    }
    let run = "/api/v1/reward-settings/{id}/run";
    assert!(!permits(Scope::Read, &Method::POST, run));
    assert!(permits(Scope::Run, &Method::POST, run));
    assert!(!permits(Scope::Run, &Method::GET, run));
}
