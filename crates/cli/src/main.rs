//! Wayfind's imperative shell.
//!
//! This binary owns every effect: argument parsing, layered configuration,
//! filesystem and standard-input access, the clock, the SQLite adapters, and
//! writing output. It holds no business rule; it asks `wayfind_core` for every
//! decision and applies the result.
//!
//! The order matters. The command line is parsed first, so `--help` and
//! `--version` answer without a configuration file or a database. Only once
//! there is a command to carry out is anything opened.
#![deny(clippy::unwrap_used, clippy::expect_used)]

use std::io::{self, BufWriter};
use std::process::ExitCode;

use clap::Parser;
use wayfind_cli::app;
use wayfind_cli::args::Cli;
use wayfind_cli::context::SystemEnvironment;

fn main() -> ExitCode {
    // Clap prints help, version, and usage errors itself and exits, so nothing
    // below runs for those.
    let cli = Cli::parse();

    let environment = SystemEnvironment;
    // A document is written a piece at a time, so buffering keeps one command to
    // one write. `app::run` flushes before it returns.
    let mut output = BufWriter::new(io::stdout().lock());

    match app::run(cli, &environment, &mut output) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // The same one-line shape the script used, so an agent or a wrapper
            // that reads standard error keeps working.
            eprintln!("wayfind: {error}");
            ExitCode::FAILURE
        }
    }
}
