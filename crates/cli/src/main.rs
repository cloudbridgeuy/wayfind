//! Wayfind v2's imperative shell.
//!
//! This binary owns every effect: argument parsing, layered configuration,
//! filesystem and standard-input access, the clock, the SQLite store, and
//! writing output. It holds no business rule; it asks `wayfind_core` for every
//! decision and applies the result.
#![deny(clippy::unwrap_used, clippy::expect_used)]

use std::process::ExitCode;

use clap::Parser;
use wayfind_cli::{app, args::Cli, context::SystemEnvironment};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let environment = SystemEnvironment::new();
    let mut out = std::io::stdout();
    let mut err = std::io::stderr();

    match app::run(&cli, &environment, &mut out) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => app::report(&error, &mut err),
    }
}
