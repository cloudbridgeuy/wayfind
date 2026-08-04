//! Every command but `init` opens a store rather than creating one.
//!
//! Run against an `XDG_CONFIG_HOME` nothing has initialized, any of them
//! refuses the same way: exit 3, nothing on stdout, and an error document on
//! stderr naming the fix.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::Command;

#[test]
fn a_command_against_a_missing_store_reports_not_found_and_exits_three() {
    let home = tempfile::tempdir().unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wayfind2"))
        .args(["initiative", "list"])
        .env("XDG_CONFIG_HOME", home.path())
        .env_remove("WAYFIND_SESSION_ID")
        .output()
        .expect("run wayfind2");

    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());

    let document = String::from_utf8(output.stderr).unwrap();
    assert!(document.starts_with("+++\n"));
    assert!(document.contains("kind = \"error\""));
    assert!(document.contains("error = \"not-found\""));
    assert!(document.contains("wayfind2 init"));
}
