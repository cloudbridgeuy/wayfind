//! `wayfind2 init`, driven as an operator would run it.
//!
//! This is the one integration test that runs the built binary rather than
//! linking against the crate: `init` is the one command whose entire contract
//! is what a real process prints and exits with, under a real
//! `XDG_CONFIG_HOME`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::Command;

fn wayfind2(home: &std::path::Path, arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_wayfind2"))
        .args(arguments)
        .env("XDG_CONFIG_HOME", home)
        .env_remove("WAYFIND_SESSION_ID")
        .output()
        .expect("run wayfind2")
}

#[test]
fn init_creates_the_store_and_a_second_init_says_it_is_already_there() {
    let home = tempfile::tempdir().unwrap();

    let first = wayfind2(home.path(), &["init"]);
    assert!(first.status.success());
    let first_line = String::from_utf8(first.stdout).unwrap();
    assert!(first_line.starts_with("created a store at "));
    assert!(first_line.trim_end().ends_with("wayfind2.sqlite"));
    assert!(first.stderr.is_empty());

    let database = home.path().join("wayfind/wayfind2.sqlite");
    assert!(database.exists());

    let second = wayfind2(home.path(), &["init"]);
    assert!(second.status.success());
    let second_line = String::from_utf8(second.stdout).unwrap();
    assert!(second_line.starts_with("a store already exists at "));
    assert!(second.stderr.is_empty());
}
