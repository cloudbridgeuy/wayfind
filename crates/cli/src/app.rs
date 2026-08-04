//! The composition root.
//!
//! One place decides which handler answers a parsed command line, and one place
//! turns whatever comes back into what the operator sees. Keeping both here is
//! what lets `main` be four lines and lets a test drive the whole program
//! without a process.

use std::process::ExitCode;

use wayfind_core::render;

use crate::{
    args::{Cli, Command},
    commands,
    error::{ShellError, ShellResult},
    output::Output,
};

/// The exit code a failure that is not a refusal ends with.
///
/// Class 1 is reserved for faults the operator cannot answer — a broken pipe, an
/// unreadable file. It is never contractual behavior.
const INTERNAL_ERROR: u8 = 1;

/// Carry out one command line.
pub fn run(cli: &Cli, _out: &mut dyn Output) -> ShellResult<()> {
    match &cli.command {
        Command::Init
        | Command::Migrate { .. }
        | Command::Initiative { .. }
        | Command::Graph { .. }
        | Command::Snapshot { .. }
        | Command::Node { .. }
        | Command::Transition { .. }
        | Command::Artifact { .. }
        | Command::Work { .. }
        | Command::Sessions(_)
        | Command::Run { .. }
        | Command::Ticket(_)
        | Command::Search { .. }
        | Command::Dump { .. } => Err(commands::not_implemented()),
    }
}

/// Write a failure where the operator will find it, and say how it ended.
///
/// A refusal is an error document on standard error with the token the caller
/// matches on. Anything else is one line and exit 1, because there is no token
/// for it and inventing one would be a promise the program cannot keep.
pub fn report(error: &ShellError, err: &mut dyn Output) -> ExitCode {
    match error.rejection() {
        Some(rejection) => {
            let _ = err.text(&render::error::document(rejection));
            let _ = err.flush();
            ExitCode::from(rejection.exit_code())
        }
        None => {
            let _ = err.text(&format!("{error}\n"));
            let _ = err.flush();
            ExitCode::from(INTERNAL_ERROR)
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use clap::Parser;

    use super::*;

    fn refusal_for(arguments: &[&str]) -> ShellError {
        let cli = Cli::try_parse_from(arguments).unwrap();
        let mut out: Vec<u8> = Vec::new();
        run(&cli, &mut out).unwrap_err()
    }

    #[test]
    fn a_command_this_slice_does_not_carry_out_refuses_with_the_usage_token() {
        let error = refusal_for(&["wayfind2", "initiative", "list"]);
        let rejection = error.rejection().unwrap();
        assert_eq!(rejection.exit_code(), 2);
        assert_eq!(rejection.body_text(), Some("not implemented in this slice"));
    }

    #[test]
    fn a_refusal_is_reported_as_an_error_document_with_its_own_exit_code() {
        let error = refusal_for(&["wayfind2", "graph", "history"]);
        let mut err: Vec<u8> = Vec::new();
        let code = report(&error, &mut err);

        let document = String::from_utf8(err).unwrap();
        assert!(document.starts_with("+++\n"));
        assert!(document.contains("kind = \"error\""));
        assert!(document.contains("error = \"usage\""));
        assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::from(2)));
    }
}
