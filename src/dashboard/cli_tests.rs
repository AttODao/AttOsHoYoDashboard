use super::*;

#[test]
fn initial_admin_environment_preserves_credentials_without_process_env_changes() {
    let (username, password) = initial_admin_from_env(|name| match name {
        "INITIAL_ADMIN_USERNAME" => Ok("synthetic-admin".into()),
        "INITIAL_ADMIN_PASSWORD" => Ok("  synthetic-password-with-spaces  ".into()),
        _ => Err(std::env::VarError::NotPresent),
    })
    .unwrap();
    assert_eq!(username, "synthetic-admin");
    assert!(password == "  synthetic-password-with-spaces  ");
}

#[test]
fn initial_admin_environment_errors_do_not_echo_values() {
    let error = initial_admin_from_env(|_| Err(std::env::VarError::NotPresent)).unwrap_err();
    assert!(matches!(error, Error::Invalid(_)));
    let error = initial_admin_from_env(|name| match name {
        "INITIAL_ADMIN_USERNAME" => Ok("synthetic-admin".into()),
        _ => Err(std::env::VarError::NotUnicode(
            "synthetic-private-value".into(),
        )),
    })
    .unwrap_err();
    assert!(!format!("{error:?}").contains("synthetic-private-value"));
}

#[test]
fn stdin_password_only_strips_a_single_terminal_line_ending() {
    for suffix in ["", "\n", "\r\n"] {
        let input = format!("  secret with spaces{suffix}");
        assert_eq!(
            read_password_from(input.as_bytes()).unwrap(),
            "  secret with spaces"
        );
    }
    assert_eq!(
        read_password_from(b"inside\nnewline\n".as_slice()).unwrap(),
        "inside\nnewline"
    );
}

#[test]
fn stdin_password_is_bounded_and_invalid_utf8_is_redacted() {
    assert_eq!(
        read_password_from("a".repeat(256).as_bytes())
            .unwrap()
            .len(),
        256
    );
    assert_eq!(
        read_password_from(format!("{}\r\n", "a".repeat(256)).as_bytes())
            .unwrap()
            .len(),
        256
    );
    assert!(read_password_from("a".repeat(259).as_bytes()).is_err());
    let error = read_password_from(&[0xff, 0xfe][..]).unwrap_err();
    assert_eq!(
        error.to_string(),
        "could not read password from standard input"
    );
}

#[test]
fn input_failure_never_contains_the_io_error_payload() {
    struct FailingInput;
    impl Read for FailingInput {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("synthetic-sensitive-value"))
        }
    }
    let error = read_password_from(FailingInput).unwrap_err();
    assert!(!format!("{error:?}").contains("synthetic-sensitive-value"));
}
