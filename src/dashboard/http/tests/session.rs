use super::*;
use crate::dashboard::{
    auth::{password, sessions::IssuedSession},
    db::entities::user,
    error::Error,
    http::session::{cookie_hash, set_cookie, signed_in},
};
use axum::http::{HeaderMap, HeaderValue};

#[test]
fn cookies_are_host_scoped_http_only_strict_and_never_set_domain() {
    for (secure, name, suffix) in [
        (true, "__Host-attos_session", "; Secure"),
        (false, "attos_session", ""),
    ] {
        let config = config(secure);
        assert_eq!(cookie_name(&config), name);
        assert_eq!(
            set_cookie(&config, "synthetic-token", 17).unwrap(),
            format!(
                "{name}=synthetic-token; Path=/; HttpOnly; SameSite=Strict; Max-Age=17{suffix}"
            )
        );
        assert_eq!(
            set_cookie(&config, "", 0).unwrap(),
            format!("{name}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0{suffix}")
        );
    }
    assert!(matches!(
        set_cookie(&config(true), "bad\r\nheader", 1),
        Err(Error::Internal)
    ));
}

#[test]
fn cookie_parser_hashes_only_one_valid_named_token_across_all_headers() {
    let name = "__Host-attos_session";
    let token = password::random_secret().unwrap();
    let named = format!("{name}={token}");
    let mut headers = HeaderMap::new();
    assert!(matches!(
        cookie_hash(&headers, name),
        Err(Error::Unauthorized)
    ));
    headers.append(
        header::COOKIE,
        format!("other=x; {named}; ignored=y").parse().unwrap(),
    );
    headers.append(header::COOKIE, "another=z".parse().unwrap());
    assert_eq!(
        cookie_hash(&headers, name).unwrap(),
        password::token_hash(&token).unwrap()
    );
    for duplicate in [format!("{named}; {named}"), format!("{named}; {name}=")] {
        let mut headers = HeaderMap::new();
        headers.append(header::COOKIE, duplicate.parse().unwrap());
        assert!(matches!(
            cookie_hash(&headers, name),
            Err(Error::Unauthorized)
        ));
    }
    headers.append(header::COOKIE, named.parse().unwrap());
    assert!(matches!(
        cookie_hash(&headers, name),
        Err(Error::Unauthorized)
    ));
    for value in [
        "",
        "short",
        &"g".repeat(64),
        &"a".repeat(63),
        &"a".repeat(65),
    ] {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, format!("{name}={value}").parse().unwrap());
        assert!(matches!(
            cookie_hash(&headers, name),
            Err(Error::Unauthorized)
        ));
    }
    let mut headers = HeaderMap::new();
    headers.insert(header::COOKIE, HeaderValue::from_bytes(&[0xff]).unwrap());
    assert!(matches!(
        cookie_hash(&headers, name),
        Err(Error::Unauthorized)
    ));
    headers.insert(
        header::COOKIE,
        format!("attos_session={token}").parse().unwrap(),
    );
    assert!(matches!(
        cookie_hash(&headers, name),
        Err(Error::Unauthorized)
    ));
}

#[tokio::test]
async fn signed_in_uses_remaining_lifetime_and_serializes_only_public_user_metadata() {
    let app = disconnected_app().await;
    let user = user::Model {
        id: Uuid::new_v4(),
        username: "synthetic-user".to_owned(),
        password_hash: "synthetic-hash-must-never-serialize".to_owned(),
        is_admin: false,
        active: true,
        must_change_password: true,
        created_at: Utc::now(),
        last_login_at: None,
    };
    let expected = json!({"id": user.id, "username": user.username, "is_admin": false, "active": true, "must_change_password": true, "last_login_at": null});
    let before = Utc::now();
    let expires_at = before + Duration::seconds(90);
    let response = signed_in(
        &app,
        &HeaderMap::new(),
        user.clone(),
        IssuedSession {
            token: "synthetic-token".to_owned(),
            expires_at,
        },
    )
    .await
    .unwrap();
    let after = Utc::now();
    let set = response.headers()[header::SET_COOKIE].to_str().unwrap();
    let age: i64 = set
        .split(';')
        .map(str::trim)
        .find_map(|s| s.strip_prefix("Max-Age="))
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        (expires_at.signed_duration_since(after).num_seconds().max(0)..=90).contains(&age),
        "cookie must use remaining time, not configured seven-day TTL"
    );
    assert_eq!(body(response).await, expected);
    let response = signed_in(
        &app,
        &HeaderMap::new(),
        user,
        IssuedSession {
            token: "synthetic-token".to_owned(),
            expires_at: Utc::now() - Duration::seconds(1),
        },
    )
    .await
    .unwrap();
    assert!(
        response.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
}
