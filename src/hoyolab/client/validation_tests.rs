//! Cookie and DS validation uses synthetic values; no HTTP endpoints are invented.
use super::*;

#[test]
fn protocol_and_validation() {
    assert_eq!(validate_cookie("ltuid_v2=123; ltoken_v2=v2_secret"), Ok(()));
    assert_eq!(validate_cookie("account_id=123; ltoken=secret"), Ok(()));
    for cookie in [
        "",
        "ltuid_v2=123",
        "ltuid_v2=YOUR_UID; ltoken_v2=secret",
        "ltuid_v2=123; ltoken_v2=secret\r\nInjected: true",
    ] {
        assert!(validate_cookie(cookie).is_err());
    }
    assert_eq!(
        sign_secret(1700000000, "AbCdEf"),
        "1700000000,AbCdEf,52762c606b53f6830e4692f53664105e"
    );
    let ds = dynamic_secret().unwrap();
    let parts: Vec<_> = ds.split(',').collect();
    assert_eq!(parts.len(), 3);
    assert_eq!(parts[1].len(), 6);
    assert!(parts[1].bytes().all(|b| b.is_ascii_alphabetic()));
    assert_eq!(ds, sign_secret(parts[0].parse().unwrap(), parts[1]));
    assert_eq!(
        decode_response(br#"{"retcode":0,"data":{"ok":true}}"#)
            .unwrap()
            .get(),
        r#"{"ok":true}"#
    );
    for body in [
        br#"{"retcode":0,"data":null}"#.as_slice(),
        br#"{"retcode":0}"#,
        br#"{"data":{}}"#,
        b"not json",
    ] {
        assert!(decode_response(body).is_err());
    }
    assert!(
        decode_response(br#"{"retcode":10102,"message":"private","data":null}"#)
            .unwrap_err()
            .to_string()
            .contains("公開")
    );
}
