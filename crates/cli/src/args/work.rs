//! Choosing what to work on, handing it off, and seeing who else is here.

use clap::{Args, Subcommand};

/// Choosing work and handing it off.
#[derive(Debug, Clone, Subcommand)]
pub enum WorkCommand {
    /// Show the frontier entries that can be taken up, and by what.
    Eligible {
        /// The snapshot to read, instead of the head.
        #[arg(long, value_name = "S")]
        snapshot: Option<String>,
    },

    /// Name the entries worth taking up first.
    Recommend {
        /// How many to name.
        #[arg(long, value_name = "N")]
        limit: Option<i64>,
    },

    /// Take a frontier entry for this session.
    ///
    /// A claim keeps the entry visible and eligible. It is a statement about
    /// who is on it, not a lock over the graph.
    Claim {
        /// Which result.
        #[arg(value_name = "R")]
        node: String,

        /// The snapshot the claim is taken against.
        #[arg(long, value_name = "S")]
        base: String,
    },

    /// Give a claimed frontier entry back.
    Release {
        /// Which result.
        #[arg(value_name = "R")]
        node: String,
    },

    /// Record which kind of work a claimed entry was handed off to.
    Handoff {
        /// Which result.
        #[arg(value_name = "R")]
        node: String,

        /// The transition kind the work will produce.
        #[arg(long, value_name = "KIND")]
        workflow: String,
    },
}

/// The `sessions` group.
///
/// A bare `sessions` lists, the way v1's did, so an existing habit keeps
/// working. The subcommands say the same thing out loud.
#[derive(Debug, Clone, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct SessionsArgs {
    /// Which one, if not the listing.
    #[command(subcommand)]
    pub command: Option<SessionsCommand>,
}

/// What can be asked of the sessions.
#[derive(Debug, Clone, Subcommand)]
pub enum SessionsCommand {
    /// Show this session's place: its initiative, its claim, and its work.
    Resume,

    /// List the sessions working on this project.
    List,
}
