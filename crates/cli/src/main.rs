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

mod args;
mod config;
mod error;

use std::process::ExitCode;

use clap::Parser;
use wayfind_core::StorageConfig;

use crate::args::Cli;
use crate::config::ConfigContext;
use crate::error::{ShellError, ShellResult};

fn main() -> ExitCode {
    // Clap prints help, version, and usage errors itself and exits, so nothing
    // below runs for those.
    let cli = Cli::parse();

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // The same one-line shape the script used, so an agent or a wrapper
            // that reads standard error keeps working.
            eprintln!("wayfind: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> ShellResult<()> {
    let resolved = config::load_config(ConfigContext {
        explicit_file: cli.globals.config.clone(),
        cli: cli.globals.config_source(),
    })?;

    // The storage and search adapters, and the dispatch that uses them, arrive
    // with the tasks that follow. Until then a command is parsed and resolved,
    // and then refused outright rather than half carried out.
    let StorageConfig::Sqlite(store) = &resolved.storage;
    Err(ShellError::refused(format!(
        "the sqlite backend at {} is not wired up yet",
        store.database.display()
    )))
}
