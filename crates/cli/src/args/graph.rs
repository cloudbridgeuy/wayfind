//! Reading the result graph, and recording what a result led to.
//!
//! Every identifier here is a string, not a number. Nodes, transitions,
//! connections, and artifacts are addressed by the hash of their content;
//! snapshots by a per-initiative ordinal such as `S3`. Nothing in this group
//! allocates an id, so nothing in it takes one.

use std::path::PathBuf;

use clap::Subcommand;

/// Reading the graph, and continuing a result differently.
#[derive(Debug, Clone, Subcommand)]
pub enum GraphCommand {
    /// Show the graph at a snapshot.
    Show {
        /// The snapshot to read, instead of the head.
        #[arg(long, value_name = "S")]
        snapshot: Option<String>,
    },

    /// Show the results nothing has continued yet.
    Frontier {
        /// The snapshot to read, instead of the head.
        #[arg(long, value_name = "S")]
        snapshot: Option<String>,
    },

    /// Show the initiative's accepted transitions in order.
    History,

    /// Show what a result led to.
    Impact {
        /// Which result.
        #[arg(value_name = "NODE")]
        node: String,

        /// The snapshot to read, instead of the head.
        #[arg(long, value_name = "S")]
        snapshot: Option<String>,
    },

    /// Split one result into several, before continuing them separately.
    Split {
        /// The result to split.
        #[arg(value_name = "R")]
        node: String,

        /// The snapshot this write is based on.
        #[arg(long, value_name = "S")]
        base: String,

        /// The manifest describing the pieces.
        #[arg(long, value_name = "FILE")]
        manifest: PathBuf,
    },

    /// Bring several results together into one.
    Merge {
        /// The results to bring together.
        #[arg(value_name = "R", required = true, num_args = 1..)]
        nodes: Vec<String>,

        /// The snapshot this write is based on.
        #[arg(long, value_name = "S")]
        base: String,

        /// The manifest describing the result.
        #[arg(long, value_name = "FILE")]
        manifest: PathBuf,
    },

    /// Record that a result cannot be continued, and why.
    Block {
        /// Which result.
        #[arg(value_name = "R")]
        node: String,

        /// The snapshot this write is based on.
        #[arg(long, value_name = "S")]
        base: String,

        /// What is in the way.
        #[arg(long, value_name = "TEXT")]
        reason: String,
    },

    /// Record that a blocked result can be continued again.
    Recover {
        /// Which result.
        #[arg(value_name = "R")]
        node: String,

        /// The snapshot this write is based on.
        #[arg(long, value_name = "S")]
        base: String,

        /// The manifest describing what changed.
        #[arg(long, value_name = "FILE")]
        manifest: PathBuf,
    },

    /// Record that a result is not worth continuing, and why.
    Abandon {
        /// Which result.
        #[arg(value_name = "R")]
        node: String,

        /// The snapshot this write is based on.
        #[arg(long, value_name = "S")]
        base: String,

        /// Why it was left.
        #[arg(long, value_name = "TEXT")]
        reason: String,
    },

    /// Replace a continuation that already exists with a different one.
    Supersede {
        /// The results being replaced.
        #[arg(value_name = "R", required = true, num_args = 1..)]
        nodes: Vec<String>,

        /// The snapshot this write is based on.
        #[arg(long, value_name = "S")]
        base: String,

        /// The manifest describing the replacement.
        #[arg(long, value_name = "FILE")]
        manifest: PathBuf,
    },
}

/// Reading the snapshots an initiative passed through.
#[derive(Debug, Clone, Subcommand)]
pub enum SnapshotCommand {
    /// List the snapshots of this initiative.
    List,

    /// Show one snapshot and what it holds.
    Show {
        /// Which one.
        #[arg(value_name = "S")]
        snapshot: String,
    },

    /// Show what changed between two snapshots.
    Diff {
        /// The snapshot to read from.
        #[arg(value_name = "LEFT")]
        left: String,

        /// The snapshot to read to.
        #[arg(value_name = "RIGHT")]
        right: String,
    },
}

/// Reading one result node.
#[derive(Debug, Clone, Subcommand)]
pub enum NodeCommand {
    /// Show one result node.
    Show {
        /// Which one.
        #[arg(value_name = "R")]
        node: String,
    },
}

/// Reading and accepting transitions.
#[derive(Debug, Clone, Subcommand)]
pub enum TransitionCommand {
    /// Show one transition and what it consumed and produced.
    Show {
        /// Which one.
        #[arg(value_name = "T")]
        transition: String,
    },

    /// Record a result against the graph.
    Accept {
        /// The snapshot this write is based on.
        #[arg(long, value_name = "S")]
        base: String,

        /// The manifest describing the transition.
        #[arg(long, value_name = "FILE")]
        manifest: PathBuf,
    },
}

/// Reading one artifact.
#[derive(Debug, Clone, Subcommand)]
pub enum ArtifactCommand {
    /// Show one artifact.
    Show {
        /// Which one.
        #[arg(value_name = "A")]
        artifact: String,

        /// Write the stored bytes alone, with no document around them.
        #[arg(long)]
        raw: bool,
    },
}
