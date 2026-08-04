//! The command line, exactly as an operator types it.
//!
//! This module only describes shapes. Parsing produces a [`Cli`] and nothing
//! else happens: no file is read, no database is opened, no clock is consulted.
//! That is what lets `--help` and `--version` answer on a machine that has no
//! Wayfind store at all.
//!
//! The tree follows the v2 CLI specification's proposed command tree, group for
//! group. Most groups do nothing yet — this slice only creates a store — but the
//! argument surface is settled here so a later slice replaces a handler's body
//! and never its spelling.

pub mod graph;
pub mod initiative;
pub mod run;
pub mod ticket;
pub mod work;

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use wayfind_core::config::ConfigSource;

/// Wayfind's command line.
#[derive(Debug, Clone, Parser)]
#[command(
    name = "wayfind2",
    version,
    about = "Chart an initiative as an append-only result graph",
    disable_help_subcommand = true
)]
pub struct Cli {
    /// The settings that apply to every command.
    #[command(flatten)]
    pub globals: Globals,

    /// What to do.
    #[command(subcommand)]
    pub command: Command,
}

/// Settings that hold for whatever command follows.
///
/// Every one is `global`, so it reads the same before or after the subcommand:
/// `wayfind2 --initiative 4 graph show` and `wayfind2 graph show --initiative 4`
/// are one command.
#[derive(Debug, Clone, Default, Args)]
pub struct Globals {
    /// The directory whose project key this command belongs to.
    #[arg(long, global = true, value_name = "PATH")]
    pub project: Option<PathBuf>,

    /// The session doing the work, instead of the one derived from the terminal.
    #[arg(long, global = true, value_name = "ID")]
    pub session: Option<String>,

    /// The initiative to act on, instead of the project's active one.
    #[arg(long, global = true, value_name = "ID")]
    pub initiative: Option<i64>,

    /// A configuration file to read instead of the default one.
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Settings addressed to the storage backend.
    #[command(flatten)]
    pub backend: BackendOptions,
}

/// Backend settings, named after the section they override.
///
/// The dotted names match the configuration file, so an operator who has read
/// `[sqlite] database` knows to pass `--sqlite.database` without being told.
#[derive(Debug, Clone, Default, Args)]
pub struct BackendOptions {
    /// The SQLite database file to open.
    #[arg(long = "sqlite.database", global = true, value_name = "PATH")]
    pub database: Option<PathBuf>,

    /// The database file holding the FTS5 index.
    #[arg(long = "sqlite-fts5.database", global = true, value_name = "PATH")]
    pub fts_database: Option<PathBuf>,
}

impl Globals {
    /// The configuration this command line asks for, as one layer.
    ///
    /// Only backend settings appear here. `--project`, `--session`, and
    /// `--initiative` say what to act on, not what to open, so they stay on
    /// [`Globals`] and are read by the shell directly.
    pub fn config_source(&self) -> ConfigSource {
        ConfigSource {
            database: self.backend.database.clone(),
            fts_database: self.backend.fts_database.clone(),
        }
    }
}

/// Every command Wayfind answers.
#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Create the store and register this project.
    Init,

    /// Read a v1 database and write its initiatives into this store.
    Migrate {
        /// The v1 database to read, instead of the default one.
        #[arg(long, value_name = "PATH")]
        from: Option<PathBuf>,

        /// Report what would be written without writing it.
        #[arg(long)]
        dry_run: bool,
    },

    /// Start, inspect, fork, or finish an initiative.
    Initiative {
        /// Which one.
        #[command(subcommand)]
        command: initiative::InitiativeCommand,
    },

    /// Read the result graph, and record what a result led to.
    Graph {
        /// Which one.
        #[command(subcommand)]
        command: graph::GraphCommand,
    },

    /// List and compare the snapshots an initiative passed through.
    Snapshot {
        /// Which one.
        #[command(subcommand)]
        command: graph::SnapshotCommand,
    },

    /// Read one result node.
    Node {
        /// Which one.
        #[command(subcommand)]
        command: graph::NodeCommand,
    },

    /// Read or accept one transition.
    Transition {
        /// Which one.
        #[command(subcommand)]
        command: graph::TransitionCommand,
    },

    /// Read one artifact.
    Artifact {
        /// Which one.
        #[command(subcommand)]
        command: graph::ArtifactCommand,
    },

    /// Choose what to work on, and hand it off.
    Work {
        /// Which one.
        #[command(subcommand)]
        command: work::WorkCommand,
    },

    /// Show the sessions working on this project.
    Sessions(work::SessionsArgs),

    /// Chart one stretch of unknown ground.
    Run {
        /// Which one.
        #[command(subcommand)]
        command: run::RunCommand,
    },

    /// Track project-level work that is not a result yet.
    Ticket(ticket::TicketArgs),

    /// Search result nodes, run questions, and tickets together.
    Search {
        /// What to look for.
        #[arg(value_name = "TERMS")]
        terms: String,

        /// How many records to return.
        #[arg(long, value_name = "N")]
        limit: Option<i64>,

        /// How many records to skip first.
        #[arg(long, value_name = "N")]
        offset: Option<i64>,
    },

    /// Write the question records to standard output for piping.
    Dump {
        /// Write comma-separated records.
        #[arg(long)]
        csv: bool,

        /// How many records to write.
        #[arg(long, value_name = "N")]
        limit: Option<i64>,

        /// How many records to skip first.
        #[arg(long, value_name = "N")]
        offset: Option<i64>,
    },
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use clap::CommandFactory;

    use super::*;

    #[test]
    fn the_command_line_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn a_global_reads_the_same_before_and_after_the_subcommand() {
        let before = Cli::try_parse_from(["wayfind2", "--initiative", "4", "graph", "history"]);
        let after = Cli::try_parse_from(["wayfind2", "graph", "history", "--initiative", "4"]);
        assert_eq!(before.unwrap().globals.initiative, Some(4));
        assert_eq!(after.unwrap().globals.initiative, Some(4));
    }

    #[test]
    fn the_backend_flags_keep_their_dotted_spelling() {
        let cli = Cli::try_parse_from([
            "wayfind2",
            "--sqlite.database",
            "/tmp/a.sqlite",
            "--sqlite-fts5.database",
            "/tmp/b.sqlite",
            "init",
        ])
        .unwrap();
        let source = cli.globals.config_source();
        assert_eq!(source.database.unwrap().to_str(), Some("/tmp/a.sqlite"));
        assert_eq!(source.fts_database.unwrap().to_str(), Some("/tmp/b.sqlite"));
    }

    #[test]
    fn every_group_of_the_command_tree_is_reachable() {
        for group in [
            "init",
            "migrate",
            "initiative",
            "graph",
            "snapshot",
            "node",
            "transition",
            "artifact",
            "work",
            "sessions",
            "run",
            "ticket",
            "search",
            "dump",
        ] {
            assert!(
                Cli::command()
                    .get_subcommands()
                    .any(|command| command.get_name() == group),
                "missing group {group}"
            );
        }
    }
}
