//! The Wayfind run: one stretch of unknown ground, charted before it is walked.
//!
//! This is v1's machinery, keyed by run instead of by initiative. The questions,
//! the dependencies, the decisions, the fog notes, the exclusions, and the
//! attachments all keep their v1 spelling below `run`, because the group they
//! moved under is the only thing about them that changed.

use std::path::PathBuf;

use clap::{Subcommand, ValueEnum};

/// What can be done with a run.
#[derive(Debug, Clone, Subcommand)]
pub enum RunCommand {
    /// Start a run from a result, toward somewhere it does not reach yet.
    Create {
        /// The result the run starts from.
        #[arg(long, value_name = "R")]
        from: String,

        /// Where the run is going.
        #[arg(long, value_name = "TEXT")]
        destination: String,
    },

    /// List the runs of this initiative.
    List,

    /// Show one run.
    Show {
        /// Which one.
        #[arg(value_name = "W")]
        run: String,
    },

    /// Show the run: frontier, decisions, fog, and exclusions.
    Map,

    /// Draw the run's dependency graph.
    Tree,

    /// Name the one question to answer now.
    Next,

    /// Write the run's handoff document.
    Handoff,

    /// Finish the run and turn what it settled into a result.
    Clear,

    /// Ask, answer, and order the run's questions.
    Question {
        /// Which one.
        #[command(subcommand)]
        command: QuestionCommand,
    },

    /// Record what the run does not know yet.
    Fog {
        /// Which one.
        #[command(subcommand)]
        command: FogCommand,
    },

    /// Record what the run deliberately leaves out.
    Scope {
        /// Which one.
        #[command(subcommand)]
        command: ScopeCommand,
    },

    /// File and read the documents a question rests on.
    Attach {
        /// Which one.
        #[command(subcommand)]
        command: AttachCommand,
    },
}

/// The kind of question being asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "lowercase")]
pub enum QuestionType {
    /// A question to grill an assumption with.
    Grilling,
    /// An investigation that gathers facts without settling design.
    Research,
    /// A build-to-learn spike.
    Prototype,
    /// Ordinary work with a definite finish.
    Task,
}

/// What can be done to a question.
#[derive(Debug, Clone, Subcommand)]
pub enum QuestionCommand {
    /// Ask a question of the run.
    Create {
        /// What kind of question it is.
        #[arg(long = "type", value_name = "TYPE")]
        question_type: QuestionType,

        /// The question itself.
        #[arg(long, value_name = "TEXT")]
        question: String,
    },

    /// Show one question and what it rests on.
    Show {
        /// Which one.
        #[arg(value_name = "N")]
        id: i64,
    },

    /// Take a question for this session.
    Claim {
        /// Which one.
        #[arg(value_name = "N")]
        id: i64,
    },

    /// Give a claimed question back.
    Release {
        /// Which one.
        #[arg(value_name = "N")]
        id: i64,
    },

    /// Settle a question, and record what was decided.
    Resolve {
        /// Which one.
        #[arg(value_name = "N")]
        id: i64,

        /// What was decided.
        #[arg(long, value_name = "TEXT", conflicts_with = "resolution_file")]
        resolution: Option<String>,

        /// A file holding what was decided, or `-` for standard input.
        #[arg(long, value_name = "FILE")]
        resolution_file: Option<PathBuf>,
    },

    /// Change what a settled question decided.
    Amend {
        /// Which one.
        #[arg(value_name = "N")]
        id: i64,

        /// What it decides instead.
        #[arg(long, value_name = "TEXT", conflicts_with = "resolution_file")]
        resolution: Option<String>,

        /// A file holding what it decides instead, or `-` for standard input.
        #[arg(long, value_name = "FILE")]
        resolution_file: Option<PathBuf>,
    },

    /// Record that one question cannot be answered before another.
    Block {
        /// The question that has to wait.
        #[arg(value_name = "N")]
        id: i64,

        /// The question it waits for.
        #[arg(long = "by", value_name = "M")]
        blocker: i64,
    },
}

/// What can be done with fog.
#[derive(Debug, Clone, Subcommand)]
pub enum FogCommand {
    /// Record something the run does not know yet.
    Add {
        /// What is unclear.
        #[arg(long, value_name = "TEXT")]
        note: String,
    },
}

/// What can be done with scope.
#[derive(Debug, Clone, Subcommand)]
pub enum ScopeCommand {
    /// Record something the run deliberately leaves out.
    Exclude {
        /// What is left out.
        #[arg(long, value_name = "TEXT")]
        note: String,
    },
}

/// What can be done with an attachment.
#[derive(Debug, Clone, Subcommand)]
pub enum AttachCommand {
    /// File a document against a question.
    Add {
        /// The question it belongs to.
        #[arg(value_name = "N")]
        question: i64,

        /// The file to read, or `-` for standard input.
        #[arg(long, value_name = "PATH")]
        file: PathBuf,

        /// What the document is.
        #[arg(long, value_name = "TEXT")]
        description: String,
    },

    /// List the documents of a question, or of the whole run.
    List {
        /// The question to list, instead of every question.
        #[arg(value_name = "N")]
        question: Option<i64>,
    },

    /// Show one document.
    Show {
        /// Which one.
        #[arg(value_name = "A")]
        attachment: i64,

        /// Write the stored bytes alone, with no document around them.
        #[arg(long)]
        raw: bool,
    },

    /// Point a second question at a document that already exists.
    Ref {
        /// The question that also rests on it.
        #[arg(value_name = "N")]
        question: i64,

        /// The document.
        #[arg(long, value_name = "A")]
        attachment: i64,
    },
}
