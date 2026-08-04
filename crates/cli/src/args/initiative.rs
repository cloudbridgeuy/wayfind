//! The initiative group: starting one, reading it, forking it, finishing it.

use std::path::PathBuf;

use clap::Subcommand;

/// What can be done to an initiative.
#[derive(Debug, Clone, Subcommand)]
pub enum InitiativeCommand {
    /// Chart a new initiative and its destination.
    Create {
        /// What to call it.
        #[arg(long, value_name = "NAME")]
        name: String,

        /// Where it is going.
        #[arg(long, value_name = "TEXT")]
        destination: String,

        /// Anything else worth recording about the destination.
        #[arg(long, value_name = "TEXT")]
        notes: Option<String>,
    },

    /// List the initiatives of this project.
    List,

    /// Show one initiative.
    Show {
        /// Which one.
        #[arg(value_name = "ID")]
        id: i64,
    },

    /// Finish an initiative and say why.
    Close {
        /// Which one.
        #[arg(value_name = "ID")]
        id: i64,

        /// Why it is finished.
        #[arg(long, value_name = "TEXT")]
        reason: String,
    },

    /// Fork an initiative at one of its snapshots.
    Clone {
        /// Which one to fork.
        #[arg(value_name = "ID")]
        id: i64,

        /// The snapshot to fork at.
        #[arg(long, value_name = "S")]
        snapshot: String,

        /// What to call the fork.
        #[arg(long, value_name = "NAME")]
        name: String,
    },

    /// Fork an initiative and bring another initiative's results into it.
    ///
    /// Import never writes into an existing initiative. The `--name` is the
    /// fork's name, and refusing the in-place form is what keeps the graph
    /// append-only.
    Import {
        /// The initiative and snapshot to fork, as `ID@S`.
        #[arg(long, value_name = "ID@S")]
        target: String,

        /// The initiative and snapshot to take results from, as `ID@S`.
        #[arg(long, value_name = "ID@S")]
        source: String,

        /// The manifest naming what to bring over.
        #[arg(long, value_name = "FILE")]
        manifest: PathBuf,

        /// What to call the fork.
        #[arg(long, value_name = "NAME")]
        name: String,
    },
}
