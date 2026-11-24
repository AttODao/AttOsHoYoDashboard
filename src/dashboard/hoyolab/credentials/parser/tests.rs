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
        "ltoken_v2=fake; ltoken_v2=fake2; ltuid_v2=1",
        "ltoken_v2=fake; ltuid_v2=1; ltuid_v2=2",
        "ltoken=fake; ltuid=1; account_id=2",
        "ltoken=fake; ltoken_v2=fake2; ltuid=1",
        "ltoken_v2=fake; account_id_v2=1; ltuid=2",
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
fn submissions_require_the_full_v2_bundle_including_mid() {
    let expected = format!("ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID}");
    for raw in [
        format!("Cookie: _ga=GA1; ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID};"),
        format!(
            "ltoken=legacy; empty=; valueless; account_id_v2=9; account_mid_v2=other; ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID}"
        ),
    ] {
        let credentials = Credentials::parse_submitted(&raw).expect("full v2 bundle submits");
        assert_eq!(credentials.as_str(), expected, "{raw:?}");
        assert_eq!(credentials.claimed_uid(), "42");
    }

    for raw in [
        format!("ltoken_v2={V2_TOKEN}; ltuid_v2=42"),
        format!("ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2="),
        format!("ltoken_v2={V2_TOKEN}; ltuid_v2=42; account_mid_v2={MID}"),
        format!("ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2=bad\\mid"),
        format!("ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID}; ltmid_v2={MID}"),
        format!("ltoken_v2={V2_TOKEN}; account_id_v2=42; ltmid_v2={MID}"),
        format!("ltuid_v2=42; ltmid_v2={MID}"),
        format!("ltoken={LEGACY_TOKEN}; ltuid=42; ltmid_v2={MID}"),
        String::new(),
    ] {
        assert!(
            matches!(Credentials::parse_submitted(&raw), Err(Error::Invalid(_))),
            "{raw:?} should be rejected"
        );
    }
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
fn parse_accepts_v2_pair_despite_empty_malformed_or_conflicting_extras() {
    for (raw, expected) in [
        (
            format!("ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2=; account_mid_v2=; _ga="),
            format!("ltoken_v2={V2_TOKEN}; ltuid_v2=42"),
        ),
        (
            format!("valueless; bad name=x; =x; ltoken_v2={V2_TOKEN}; ltuid_v2=42"),
            format!("ltoken_v2={V2_TOKEN}; ltuid_v2=42"),
        ),
        (
            format!(
                "ltoken=legacy; ltuid=7; account_id=8; account_id_v2=9; account_id_v2=10; ltoken_v2={V2_TOKEN}; ltuid_v2=42"
            ),
            format!("ltoken_v2={V2_TOKEN}; ltuid_v2=42"),
        ),
        (
            format!("ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2=bad token; account_mid_v2={MID}"),
            format!("ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID}"),
        ),
        (
            format!("ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2=a; ltmid_v2=b"),
            format!("ltoken_v2={V2_TOKEN}; ltuid_v2=42"),
        ),
        (
            format!("ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID}; account_mid_v2=other"),
            format!("ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID}"),
        ),
    ] {
        let credentials = Credentials::parse(&raw).expect("complete v2 pair parses");
        assert_eq!(credentials.as_str(), expected, "{raw:?}");
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
    ] {
        assert!(
            matches!(Credentials::parse(raw), Err(Error::Invalid(_))),
            "{raw:?} should be rejected"
        );
    }
}

const COOKIE_TOKEN: &str = "fake_cookie.token-XYZ==";

#[test]
fn submissions_keep_redemption_credentials_bound_to_the_same_account() {
    let raw = format!(
        "ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID}; cookie_token_v2={COOKIE_TOKEN}; account_id_v2=000042; account_mid_v2={MID}; _ga=x"
    );
    let credentials = Credentials::parse_submitted(&raw).expect("v2 bundle with cookie_token");
    // Reads and the check-in keep sending only the ltoken pair.
    assert_eq!(
        credentials.as_str(),
        format!("ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID}")
    );
    assert_eq!(
        credentials.redemption(),
        Some(
            format!("cookie_token_v2={COOKIE_TOKEN}; account_id_v2=42; account_mid_v2={MID}")
                .as_str()
        )
    );
    // The sealed text parses back to the same credentials (what `open` relies on).
    let reopened = Credentials::parse(&credentials.sealed_text()).unwrap();
    assert_eq!(reopened.as_str(), credentials.as_str());
    assert_eq!(reopened.redemption(), credentials.redemption());
    assert_eq!(reopened.claimed_uid(), "42");

    // Without cookie_token_v2 nothing is kept for redemption.
    let plain = Credentials::parse_submitted(&format!(
        "ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID}; cookie_token=legacy-v1; account_id_v2=42"
    ))
    .unwrap();
    assert_eq!(plain.redemption(), None);
    assert_eq!(plain.sealed_text(), plain.as_str());
}

#[test]
fn redemption_credentials_of_another_account_or_without_an_account_are_refused() {
    for raw in [
        // The cookie_token names another HoYoLAB account than the verified ltoken pair.
        format!(
            "ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID}; cookie_token_v2={COOKIE_TOKEN}; account_id_v2=43"
        ),
        // No account_id_v2 to bind it to.
        format!(
            "ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID}; cookie_token_v2={COOKIE_TOKEN}"
        ),
        format!(
            "ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID}; cookie_token_v2={COOKIE_TOKEN}; account_id_v2=42; account_id_v2=43"
        ),
        format!(
            "ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID}; cookie_token_v2=bad\\token; account_id_v2=42"
        ),
        format!(
            "ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID}; cookie_token_v2=YOUR_TOKEN; account_id_v2=42"
        ),
        format!(
            "ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID}; cookie_token_v2={COOKIE_TOKEN}; cookie_token_v2=other; account_id_v2=42"
        ),
    ] {
        assert!(
            matches!(Credentials::parse_submitted(&raw), Err(Error::Invalid(_))),
            "{raw:?} should be rejected"
        );
        assert!(
            matches!(Credentials::parse(&raw), Err(Error::Invalid(_))),
            "{raw:?} should be rejected when stored too"
        );
    }
    // Without a MID the redemption header carries no account_mid_v2.
    let stored = Credentials::parse(&format!(
        "ltoken_v2={V2_TOKEN}; ltuid_v2=42; cookie_token_v2={COOKIE_TOKEN}; account_id_v2=42"
    ))
    .unwrap();
    assert_eq!(
        stored.redemption(),
        Some(format!("cookie_token_v2={COOKIE_TOKEN}; account_id_v2=42").as_str())
    );
}

#[test]
fn the_submitted_account_mid_v2_is_sent_for_redemption_and_survives_sealing() {
    // genshin.py sends the whole Cookie and hoyolab-auto keeps the Cookie's own
    // `account_mid_v2`: when it differs from `ltmid_v2`, the submitted value is the one sent.
    let raw = format!(
        "ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID}; cookie_token_v2={COOKIE_TOKEN}; account_id_v2=42; account_mid_v2=other_mid_9"
    );
    let credentials = Credentials::parse_submitted(&raw).unwrap();
    assert_eq!(
        credentials.as_str(),
        format!("ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID}")
    );
    let expected =
        format!("cookie_token_v2={COOKIE_TOKEN}; account_id_v2=42; account_mid_v2=other_mid_9");
    assert_eq!(credentials.redemption(), Some(expected.as_str()));
    let reopened = Credentials::parse(&credentials.sealed_text()).unwrap();
    assert_eq!(reopened.as_str(), credentials.as_str());
    assert_eq!(reopened.redemption(), Some(expected.as_str()));
    // An unusable account_mid_v2 falls back to ltmid_v2, as the ltoken pair's MID does.
    let fallback = Credentials::parse_submitted(&format!(
        "ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID}; cookie_token_v2={COOKIE_TOKEN}; account_id_v2=42; account_mid_v2=bad\\mid"
    ))
    .unwrap();
    assert_eq!(
        fallback.redemption(),
        Some(
            format!("cookie_token_v2={COOKIE_TOKEN}; account_id_v2=42; account_mid_v2={MID}")
                .as_str()
        )
    );
}

#[test]
fn token_shaped_values_reach_the_redemption_header_byte_for_byte() {
    // Shapes HoYoLAB tokens take (synthetic): a `v2_` prefix, base64 padding, `-`, `_`, `.`,
    // `/`, `+` and percent-escapes. Nothing is trimmed, decoded or re-encoded.
    for token in [
        "v2_CAQSDGM5b3FhcTNzM2d1OBokYjk4YTU3ZDAtZmFrZS0wMDAwLTAwMDAtc3ludGhldGljIAEo==",
        "v2_abc.DEF-ghi_jkl/mno+pqr=",
        "v2_percent%3D%2Fescaped%2B",
    ] {
        for raw in [
            format!(
                "ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID}; cookie_token_v2={token}; account_id_v2=42; account_mid_v2={MID}"
            ),
            // A DevTools copy: the header name, extra spaces and unrelated cookies around it.
            format!(
                "Cookie:  mi18nLang=ja-jp;  cookie_token_v2={token};account_id_v2=42 ; ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID}; account_mid_v2={MID}; _MHYUUID=x  "
            ),
        ] {
            let credentials = Credentials::parse_submitted(&raw).expect("token-shaped value");
            let expected =
                format!("cookie_token_v2={token}; account_id_v2=42; account_mid_v2={MID}");
            assert_eq!(credentials.redemption(), Some(expected.as_str()), "{raw}");
            let reopened = Credentials::parse(&credentials.sealed_text()).unwrap();
            assert_eq!(reopened.redemption(), Some(expected.as_str()), "{raw}");
        }
    }
    // A quoted or comma-carrying value is refused, never stored altered.
    for token in ["\"v2_quoted\"", "v2_a,b"] {
        let raw = format!(
            "ltoken_v2={V2_TOKEN}; ltuid_v2=42; ltmid_v2={MID}; cookie_token_v2={token}; account_id_v2=42"
        );
        assert!(
            matches!(Credentials::parse_submitted(&raw), Err(Error::Invalid(_))),
            "{raw}"
        );
    }
}
