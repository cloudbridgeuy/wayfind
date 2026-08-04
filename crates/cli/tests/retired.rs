//! Every retired v1 spelling, driven as an operator would run it.
//!
//! Each row of `wayfind_core::retired::REPLACEMENTS` gets its own process:
//! the exact v1 words, split on spaces, become the argument vector. The
//! answer is always the same shape — exit 2, nothing on stdout, an `error`
//! document on stderr naming the exact v2 replacement.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::Command;

use wayfind_core::retired::REPLACEMENTS;

fn wayfind2(home: &std::path::Path, arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_wayfind2"))
        .args(arguments)
        .env("XDG_CONFIG_HOME", home)
        .env_remove("WAYFIND_SESSION_ID")
        .output()
        .expect("run wayfind2")
}

fn front_matter(stderr: &[u8]) -> toml::Table {
    let document = String::from_utf8(stderr.to_vec()).unwrap();
    let fenced = document
        .strip_prefix("+++\n")
        .expect("document opens with a front-matter fence");
    let end = fenced.find("+++").expect("document closes the fence");
    fenced[..end].parse::<toml::Table>().unwrap()
}

#[test]
fn every_retired_spelling_errors_with_its_replacement() {
    let home = tempfile::tempdir().unwrap();
    assert!(wayfind2(home.path(), &["init"]).status.success());

    for (v1, v2) in REPLACEMENTS {
        let words: Vec<&str> = v1.split(' ').collect();
        let output = wayfind2(home.path(), &words);

        assert_eq!(output.status.code(), Some(2), "{v1}");
        assert!(output.stdout.is_empty(), "{v1} wrote to stdout");

        let fm = front_matter(&output.stderr);
        assert_eq!(fm["kind"].as_str(), Some("error"), "{v1}");
        assert_eq!(fm["error"].as_str(), Some("retired-command"), "{v1}");
        assert_eq!(fm["replacement"].as_str(), Some(*v2), "{v1}");
    }
}

#[test]
fn the_survivors_do_not_answer_with_the_retired_command_token() {
    let home = tempfile::tempdir().unwrap();

    let init = wayfind2(home.path(), &["init"]);
    assert!(init.status.success());
    assert!(init.stderr.is_empty());

    for arguments in [vec!["search", "anything"], vec!["dump"]] {
        let output = wayfind2(home.path(), &arguments);
        if !output.stderr.is_empty() {
            let fm = front_matter(&output.stderr);
            assert_ne!(
                fm["error"].as_str(),
                Some("retired-command"),
                "{arguments:?}"
            );
        }
    }
}
