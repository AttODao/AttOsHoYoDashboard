//! Executable entry points run with an empty environment and synthetic dotenv only.
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};
use uuid::Uuid;

struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("attos-cli-test-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        // Stop dotenv's ancestor search: no real environment file is ever read.
        fs::write(root.join(".env"), "# synthetic test only\n").unwrap();
        Self(root)
    }
    fn run(&self, binary: &str, args: &[&str]) -> Output {
        Command::new(binary)
            .current_dir(&self.0)
            .env_clear()
            .args(args)
            .output()
            .unwrap()
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn dashboard_help_does_not_require_a_database_or_credentials() {
    let output = Sandbox::new().run(env!("CARGO_BIN_EXE_attosgameapi"), &["--help"]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("bootstrap-admin USERNAME"));
    assert!(text.contains("standard input"));
}

#[test]
fn invalid_commands_fail_before_database_access_without_echoing_arguments() {
    let output = Sandbox::new().run(
        env!("CARGO_BIN_EXE_attosgameapi"),
        &["synthetic-sensitive-option"],
    );
    assert!(!output.status.success());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("usage:"));
    assert!(!error.contains("synthetic-sensitive-option"));
    assert!(!error.contains("DATABASE_URL"));
}
