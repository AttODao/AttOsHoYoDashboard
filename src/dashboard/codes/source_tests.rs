use super::*;
use axum::{
    Router,
    http::{StatusCode as Http, header},
    response::IntoResponse,
    routing::get,
};
use serde_json::json;

const SAMPLE: &str = r#"{"codes":[{"id":830,"code":"PJ2KEBRAYSRM","status":"OK","game":"genshin","rewards":"Primogem*60;Adventurer's Experience*5"},{"id":842,"code":"ARCHONSANSWERS","status":"OK","game":"genshin","rewards":"Primogem*40;Hero's Wit*3;Mora*30,000"}],"game":"genshin"}"#;

#[test]
fn the_published_shape_parses_into_normalized_codes() {
    let listing = parse(SAMPLE.as_bytes(), "genshin").unwrap();
    assert_eq!(listing.rejected, 0);
    assert_eq!(
        listing.codes,
        [
            SourceCode {
                code: "PJ2KEBRAYSRM".into(),
                rewards: "Primogem*60;Adventurer's Experience*5".into(),
            },
            SourceCode {
                code: "ARCHONSANSWERS".into(),
                rewards: "Primogem*40;Hero's Wit*3;Mora*30,000".into(),
            },
        ]
    );
    // Star Rail is `hkrpg` at the source; other games are never requested.
    assert_eq!(source_game("starrail"), Some("hkrpg"));
    assert_eq!(source_game("genshin"), Some("genshin"));
    assert_eq!(source_game("nap"), None);
    assert_eq!(source_game("honkai3rd"), None);
    let rail = json!({"codes": [{"code": "STARRAILGIFT", "game": "hkrpg", "rewards": "50 stellar jade"}], "game": "hkrpg"});
    let listing = parse(rail.to_string().as_bytes(), "hkrpg").unwrap();
    assert_eq!(listing.codes[0].code, "STARRAILGIFT");
}

#[test]
fn invalid_rows_are_rejected_and_counted_while_valid_ones_stay() {
    let body = json!({"codes": [
        {"code": "lunai0910", "rewards": null},
        {"code": "LUNAI0910", "rewards": "duplicate"},
        {"code": "ABC"},
        {"code": "TOO-MANY-DASHES"},
        {"code": "A".repeat(33)},
        {"code": "ＦＵＬＬＷＩＤＴＨ"},
        {"code": 12345678},
        {"code": "OTHERGAME1", "game": "nap"},
        {"code": "BADREWARDS", "rewards": 60},
        {"code": "EXPIRED1", "status": "NOT_OK"},
        {"code": "WEIRDSTATUS", "status": false},
        "not an object",
        {"code": "GOODCODE2"}
    ]});
    let listing = parse(body.to_string().as_bytes(), "genshin").unwrap();
    assert_eq!(
        listing
            .codes
            .iter()
            .map(|code| code.code.as_str())
            .collect::<Vec<_>>(),
        ["LUNAI0910", "GOODCODE2"]
    );
    assert_eq!(listing.codes[0].rewards, "");
    // Nine bad rows; the inactive one and the duplicate are neither kept nor rejected.
    assert_eq!(listing.rejected, 9);
}

#[test]
fn rows_beyond_the_limit_are_rejected() {
    let rows: Vec<_> = (0..MAX_ROWS + 5)
        .map(|n| json!({"code": format!("CODE{n:04}")}))
        .collect();
    let listing = parse(json!({"codes": rows}).to_string().as_bytes(), "genshin").unwrap();
    assert_eq!(listing.codes.len(), MAX_ROWS);
    assert_eq!(listing.rejected, 5);
}

#[test]
fn bodies_that_are_not_a_code_list_are_invalid() {
    for body in [
        "",
        "not json",
        "[]",
        r#"{"detail": "rate limited"}"#,
        r#"{"codes": {}}"#,
        r#"{"codes": [], "game": "hkrpg"}"#,
        r#"{"codes": [{"code": "x"}, {"code": "?"}]}"#,
    ] {
        assert_eq!(
            parse(body.as_bytes(), "genshin"),
            Err(FetchError::InvalidPayload),
            "{body}"
        );
    }
    // An empty list is a valid answer: the source lists nothing right now.
    assert_eq!(
        parse(br#"{"codes": [], "game": "genshin"}"#, "genshin").unwrap(),
        Listing::default()
    );
    // Only inactive rows: also nothing listed, not an error.
    let inactive = parse(
        br#"{"codes": [{"code": "OLDCODE1", "status": "NOT_OK"}]}"#,
        "genshin",
    )
    .unwrap();
    assert!(inactive.codes.is_empty());
    assert_eq!(inactive.rejected, 0);
}

#[test]
fn reward_text_keeps_plain_words_and_drops_markup() {
    assert_eq!(
        rewards("  30 stellar jade,\n three traveler’s guides  "),
        "30 stellar jade, three traveler’s guides"
    );
    assert_eq!(
        rewards("[Primogem](https://evil.example) <b>x</b> `@everyone` _a_ ~b~ |c| #d \\e"),
        "Primogem(https//evil.example) bx/b everyone a b c d e"
    );
    assert_eq!(rewards("\u{7}\u{200b}"), "");
    let long = rewards(&"ab ".repeat(400));
    assert_eq!(long.chars().count(), MAX_REWARDS - 1);
    assert!(!long.ends_with(' '));
    assert_eq!(code(" lunai0910 "), Some("LUNAI0910".to_owned()));
    assert_eq!(code("GENSHIN GIFT"), None);
}

/// A local stand-in for the code source; `localhost` names the same server under another
/// host, so a redirect there counts as leaving the source.
async fn mock() -> u16 {
    let large = "x".repeat(MAX_BODY + 1);
    let router = Router::new()
        .route(
            "/ok/codes",
            get(|| async { ([(header::CONTENT_TYPE, "application/json")], SAMPLE) }),
        )
        .route(
            "/error/codes",
            get(|| async { (Http::INTERNAL_SERVER_ERROR, "synthetic-upstream-body") }),
        )
        .route("/large/codes", get(move || async move { large }))
        .route(
            "/slow/codes",
            get(|| async {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                SAMPLE
            }),
        )
        .route(
            "/moved/codes",
            get(|| async { (Http::FOUND, [(header::LOCATION, "/ok/codes?game=genshin")]) }),
        )
        .route(
            "/html/codes",
            get(|| async { "<html>maintenance</html>".into_response() }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let away = Router::new().route(
        "/away/codes",
        get(move || async move {
            (
                Http::FOUND,
                [(
                    header::LOCATION,
                    format!("http://localhost:{port}/ok/codes?game=genshin"),
                )],
            )
        }),
    );
    tokio::spawn(async move {
        axum::serve(listener, router.merge(away)).await.unwrap();
    });
    port
}

fn source(port: u16, path: &str, timeout: Duration) -> Source {
    Source::build(
        Url::parse(&format!("http://127.0.0.1:{port}/{path}/")).unwrap(),
        timeout,
    )
}

#[tokio::test]
async fn fetches_are_bounded_and_failures_carry_only_a_class() {
    let port = mock().await;
    let quick = Duration::from_millis(500);
    let listing = source(port, "ok", TIMEOUT).fetch("genshin").await.unwrap();
    assert_eq!(listing.codes.len(), 2);
    for (path, expected) in [
        ("error", FetchError::HttpStatus),
        ("large", FetchError::TooLarge),
        ("slow", FetchError::Timeout),
        ("away", FetchError::Redirect),
        ("html", FetchError::InvalidPayload),
    ] {
        assert_eq!(
            source(port, path, quick).fetch("genshin").await,
            Err(expected),
            "{path}"
        );
    }
    // A redirect within the same origin is followed.
    assert_eq!(
        source(port, "moved", TIMEOUT)
            .fetch("genshin")
            .await
            .unwrap()
            .codes
            .len(),
        2
    );
    // A port nobody listens on any more is a connection failure.
    let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let closed_port = closed.local_addr().unwrap().port();
    drop(closed);
    assert_eq!(
        source(closed_port, "ok", quick).fetch("genshin").await,
        Err(FetchError::Connect)
    );
    assert_eq!(
        source(port, "ok", quick).fetch("nap").await,
        Err(FetchError::InvalidPayload)
    );
    let classes: Vec<_> = FetchError::ALL.iter().map(|error| error.as_str()).collect();
    assert_eq!(
        classes,
        [
            "timeout",
            "connect",
            "http_status",
            "redirect",
            "too_large",
            "invalid_payload"
        ]
    );
}

#[tokio::test]
#[ignore = "live third-party GET to hoyo-codes.seria.moe; run only with separate approval"]
async fn live_source_lists_codes_for_both_games() {
    let source = Source::new(Url::parse(crate::dashboard::config::DEFAULT_CODES_SOURCE).unwrap());
    for game in ["genshin", "starrail"] {
        let listing = source.fetch(game).await.expect("live code list");
        assert_eq!(listing.rejected, 0, "{game}");
    }
}
