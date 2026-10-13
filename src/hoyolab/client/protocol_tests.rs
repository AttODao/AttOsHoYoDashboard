//! Synthetic upstream envelopes: protocol classification is not identity verification.
use super::*;

#[test]
fn envelope_requires_explicit_retcode_and_data() {
    for body in [
        b"not JSON".as_slice(),
        br#"{"data":{}}"#,
        br#"{"retcode":"0","data":{}}"#,
    ] {
        assert!(matches!(decode_response(body), Err(Error::InvalidEnvelope)));
    }
    for body in [
        br#"{"retcode":0}"#.as_slice(),
        br#"{"retcode":0,"data":null}"#,
    ] {
        assert!(matches!(decode_response(body), Err(Error::MissingData)));
    }
    assert_eq!(
        decode_response(br#"{"retcode":0,"data":{"valid":true}}"#)
            .unwrap()
            .get(),
        r#"{"valid":true}"#
    );
}

#[test]
fn upstream_error_precedes_payload_shape_and_discards_message() {
    let error = decode_response(
        br#"{"retcode":-100,"message":"synthetic-token-echo","data":{"unexpected":"secret"}}"#,
    )
    .unwrap_err();
    assert_eq!(error, Error::Upstream { code: -100 });
    for diagnostic in [error.to_string(), format!("{error:?}")] {
        assert!(!diagnostic.contains("synthetic-token-echo"));
        assert!(!diagnostic.contains("unexpected"));
    }
}

#[test]
fn typed_payload_errors_never_echo_unexpected_private_values() {
    #[derive(Deserialize)]
    struct Payload {
        _count: u32,
    }
    let raw = serde_json::from_str::<Box<RawValue>>(r#"{"_count":"synthetic-secret"}"#).unwrap();
    let error = parse::<Payload>(&raw).err().unwrap();
    assert!(matches!(error, Error::InvalidData(_)));
    assert!(!error.to_string().contains("synthetic-secret"));
    assert!(!format!("{error:?}").contains("synthetic-secret"));
}

#[test]
fn request_headers_mark_cookie_sensitive_and_select_protocol_family() {
    let http = build_client("ltuid_v2=123; ltoken_v2=synthetic-token", UA).unwrap();
    // Inspect requests only: these addresses never receive network traffic.
    let record = prepare_request(
        &http,
        "http://127.0.0.1/record",
        &[("role_id", "123")],
        None,
        reqwest::Method::GET,
    )
    .unwrap()
    .build()
    .unwrap();
    assert!(record.headers().contains_key("ds"));
    assert!(!record.headers().contains_key("x-rpc-signgame"));
    let sign = prepare_request(
        &http,
        "http://127.0.0.1/sign",
        &[],
        Some("hk4e"),
        reqwest::Method::POST,
    )
    .unwrap()
    .build()
    .unwrap();
    assert_eq!(sign.method(), reqwest::Method::POST);
    assert_eq!(sign.headers()["x-rpc-signgame"], "hk4e");
    assert!(!sign.headers().contains_key("ds"));
    // Default headers are applied at execution; client diagnostics must still redact them.
    assert!(!format!("{http:?}").contains("synthetic-token"));
}
