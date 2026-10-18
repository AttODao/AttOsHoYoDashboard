use super::*;

const LEGACY_TOKEN: &str = "fake.legacy-token_ABC-123==";
const V2_TOKEN: &str = "fake_v2.token-ABC123==";
const MID: &str = "fake_mid-ABC_123==";

#[test]
fn parse_enforces_cookie_size_ascii_controls_and_pair_syntax() {
    let max_token = "a".repeat(MAX_COOKIE_LEN - "ltoken=; ltuid=1".len());
    let max_raw = format!("ltoken={max_token}; ltuid=1");
    assert_eq!(max_raw.len(), MAX_COOKIE_LEN);
    assert_eq!(
        Credentials::parse(&max_raw)
            .expect("max-size Cookie parses")
            .as_str()
            .len(),
        MAX_COOKIE_LEN
    );

    let too_long = format!("{max_raw}a");
    for raw in [
        "",
        too_long.as_str(),
        "ltoken=abcé; ltuid=1",
        "ltoken=abc\r\nInjected: yes; ltuid=1",
        "ltoken=abc\u{7f}; ltuid=1",
        "ltoken; ltuid=1",
        "Cookie:",
        "Set-Cookie: ltoken=abc; ltuid=1",
        "ltoken=abc;\nltuid=1",
        "ltoken=abc; bad name=value; ltuid=1",
    ] {
        assert!(
            matches!(Credentials::parse(raw), Err(Error::Invalid(_))),
            "{raw:?} should be rejected"
        );
    }
}

#[test]
fn parse_rejects_duplicates_alias_conflicts_and_primary_family_mixing() {
    for raw in [
        "ltoken=fake; ltuid=1; ltuid=1",
        "ltoken=fake; ltoken=fake; ltuid=1",
        "ltoken_v2=fake; ltuid_v2=1; ltmid_v2=mid; ltmid_v2=mid",
        "ltoken=fake; ltuid=1; account_id=2",
        "ltoken_v2=fake; ltuid_v2=1; ltmid_v2=mid; account_mid_v2=other",
        "ltoken=fake; ltoken_v2=fake2; ltuid=1; ltuid_v2=1",
        "ltoken=fake; ltuid=1; account_id_v2=2",
        "ltoken=fake; ltuid_v2=1",
        "ltoken_v2=fake; ltuid=1",
    ] {
        assert!(
            matches!(Credentials::parse(raw), Err(Error::Invalid(_))),
            "{raw:?} should be rejected"
        );
    }
}

#[test]
fn parse_drops_unrelated_and_nonselected_token_cookies() {
    let raw = format!(
        "_ga=GA1; stoken=drop-me; cookie_token=drop-me-too; account_id=000123; ltoken={LEGACY_TOKEN}; ltuid=123; other=value"
    );
    let credentials = Credentials::parse(&raw).expect("parse legacy with droppable cookies");
    assert_eq!(
        credentials.as_str(),
        format!("ltoken={LEGACY_TOKEN}; ltuid=123")
    );
    assert_eq!(credentials.claimed_uid(), "123");
}

#[test]
fn parse_accepts_pasted_request_header_lines() {
    let expected = format!("ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID}");
    for raw in [
        format!("Cookie: ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID}"),
        format!("cookie:ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID};"),
        format!(
            "  COOKIE: _ga=GA1; mi18nLang=ja-jp; mi18nLang=en-us; ltoken_v2={V2_TOKEN};; ltuid_v2=42; ltmid_v2={MID}; \r\n"
        ),
    ] {
        let credentials = Credentials::parse(&raw).expect("parse pasted Cookie header");
        assert_eq!(credentials.as_str(), expected);
        assert_eq!(credentials.claimed_uid(), "42");
    }
}

#[test]
fn parse_normalizes_legacy_v2_and_mid_aliases() {
    let legacy = Credentials::parse(&format!("account_id=000042; ltoken={LEGACY_TOKEN}"))
        .expect("parse legacy alias");
    assert_eq!(legacy.as_str(), format!("ltoken={LEGACY_TOKEN}; ltuid=42"));
    assert_eq!(legacy.claimed_uid(), "42");

    let v2 = Credentials::parse(&format!(
        "account_mid_v2={MID}; ltoken_v2={V2_TOKEN}; account_id_v2=000042"
    ))
    .expect("parse v2 aliases");
    assert_eq!(
        v2.as_str(),
        format!("ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID}")
    );
    assert_eq!(v2.claimed_uid(), "42");

    let v2_matching_mid = Credentials::parse(&format!(
        "ltmid_v2={MID}; account_mid_v2={MID}; ltoken_v2={V2_TOKEN}; ltuid_v2=42"
    ))
    .expect("parse matching MID aliases");
    assert_eq!(
        v2_matching_mid.as_str(),
        format!("ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID}")
    );
}

#[test]
fn parse_rejects_placeholders_bad_uids_and_bad_credential_tokens() {
    for raw in [
        "ltoken=YOUR_LTOKEN; ltuid=1",
        "ltoken=...; ltuid=1",
        "ltoken=fake; ltuid=0",
        "ltoken=fake; ltuid=-1",
        "ltoken=fake; ltuid=abc",
        "ltoken=fake; ltuid=18446744073709551616",
        "ltoken=; ltuid=1",
        "ltoken=bad token; ltuid=1",
        "ltoken=bad\"token; ltuid=1",
        "ltoken=bad,token; ltuid=1",
        "ltoken=bad\\token; ltuid=1",
        "ltoken_v2=fake; ltuid_v2=1; ltmid_v2=",
        "ltoken_v2=fake; ltuid_v2=1; ltmid_v2=bad token",
    ] {
        assert!(
            matches!(Credentials::parse(raw), Err(Error::Invalid(_))),
            "{raw:?} should be rejected"
        );
    }
}
