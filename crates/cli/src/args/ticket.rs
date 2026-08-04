//! Tickets: project-level work that is not a result yet.
//!
//! v1 called a run question a ticket. v2 takes the name back for the ordinary
//! meaning — a piece of work somebody wrote down — and the questions moved under
//! `run question`. The two vocabularies below are the whole of a ticket's
//! machine-readable state.

use clap::{Args, Subcommand, ValueEnum};

/// The `ticket` group.
///
/// A bare `wayfind2 ticket ID` shows that ticket, so the commonest reading is
/// the shortest thing to type.
#[derive(Debug, Clone, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct TicketArgs {
    /// The ticket to show.
    #[arg(value_name = "ID")]
    pub id: Option<i64>,

    /// Which one, if not a reading.
    #[command(subcommand)]
    pub command: Option<TicketCommand>,
}

/// How urgent a ticket is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "lowercase")]
pub enum Priority {
    /// Worth doing, whenever.
    Low,
    /// The ordinary case.
    Normal,
    /// Ahead of the ordinary case.
    High,
    /// Now.
    Urgent,
}

/// Where a ticket has got to.
///
/// The status is informational. Nothing in the graph reads it, and closing a
/// ticket accepts no transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum Status {
    /// Written down, not started.
    Open,
    /// Somebody is on it.
    InProgress,
    /// Finished.
    Done,
    /// Not going to happen.
    Dropped,
}

/// What can be done to a ticket.
#[derive(Debug, Clone, Subcommand)]
pub enum TicketCommand {
    /// Write down a piece of work.
    Create {
        /// What it is.
        #[arg(long, value_name = "TEXT")]
        title: String,

        /// Anything else worth recording.
        #[arg(long, value_name = "TEXT")]
        description: Option<String>,

        /// How urgent it is.
        #[arg(long, value_name = "P")]
        priority: Option<Priority>,

        /// The initiative it belongs to.
        #[arg(long, value_name = "ID")]
        initiative: Option<i64>,
    },

    /// List the tickets of this project.
    List {
        /// Keep only these statuses. Repeat the flag to name several.
        #[arg(long, value_name = "ST")]
        status: Vec<Status>,

        /// Keep only these priorities. Repeat the flag to name several.
        #[arg(long, value_name = "P")]
        priority: Vec<Priority>,

        /// Keep only the tickets linked to this initiative.
        #[arg(long, value_name = "ID")]
        initiative: Option<i64>,

        /// Include the finished and the dropped ones.
        #[arg(long)]
        all: bool,
    },

    /// Change a ticket, or what it points at.
    Update {
        /// Which one.
        #[arg(value_name = "ID")]
        id: i64,

        /// What it is instead.
        #[arg(long, value_name = "TEXT")]
        title: Option<String>,

        /// What else is worth recording instead.
        #[arg(long, value_name = "TEXT")]
        description: Option<String>,

        /// How urgent it is instead.
        #[arg(long, value_name = "P")]
        priority: Option<Priority>,

        /// The initiative to link it to.
        #[arg(long, value_name = "ID", conflicts_with = "unlink")]
        link: Option<i64>,

        /// The initiative to unlink it from.
        #[arg(long, value_name = "ID")]
        unlink: Option<i64>,

        /// The result the link points at, within the linked initiative.
        #[arg(long, value_name = "R", requires = "link")]
        node: Option<String>,
    },

    /// Say where a ticket has got to.
    Status {
        /// Which one.
        #[arg(value_name = "ID")]
        id: i64,

        /// Where it has got to.
        #[arg(value_name = "STATUS")]
        status: Status,
    },
}
