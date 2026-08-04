//! The command line, exactly as an operator types it.
//!
//! This module only describes shapes. Parsing produces a [`Cli`] and nothing
//! else happens: no file is read, no database is opened, no clock is consulted.
//! That is what lets `--help` and `--version` answer on a machine that has no
//! Wayfind database at all.
//!
//! The commands mirror the Bash script at commit `0120d61`, option for option,
//! so an existing habit or script keeps working.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use wayfind_v1_core::{ConfigSource, TicketType};

/// Wayfind's command line.
#[derive(Debug, Clone, Parser)]
#[command(
    name = "wayfind",
    version,
    about = "Chart an initiative: tickets, decisions, dependencies, and handoffs",
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
/// `wayfind --initiative 4 map` and `wayfind map --initiative 4` are one
/// command.
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

    /// Which store to open.
    #[arg(long, global = true, value_name = "NAME")]
    pub backend: Option<String>,

    /// Which index to search.
    #[arg(long = "search-backend", global = true, value_name = "NAME")]
    pub search_backend: Option<String>,

    /// Settings addressed to one named backend.
    #[command(flatten)]
    pub backend_options: BackendOptions,
}

/// Backend settings, named after the section they override.
///
/// The dotted names match the configuration file, so an operator who has read
/// `[sqlite] database` knows to pass `--sqlite.database` without being told.
#[derive(Debug, Clone, Default, Args)]
pub struct BackendOptions {
    /// The SQLite database file to open.
    #[arg(long = "sqlite.database", global = true, value_name = "PATH")]
    pub sqlite_database: Option<PathBuf>,

    /// The database file holding the FTS5 index.
    #[arg(long = "sqlite-fts5.database", global = true, value_name = "PATH")]
    pub search_database: Option<PathBuf>,

    /// The FTS5 table to query.
    #[arg(long = "sqlite-fts5.table", global = true, value_name = "NAME")]
    pub search_table: Option<String>,
}

impl Globals {
    /// The configuration this command line asks for, as one layer.
    ///
    /// Only backend settings appear here. `--project`, `--session`, and
    /// `--initiative` say what to act on, not what to open, so they stay on
    /// [`Globals`] and are read by the shell directly.
    pub fn config_source(&self) -> ConfigSource {
        ConfigSource {
            backend: self.backend.clone(),
            search_backend: self.search_backend.clone(),
            sqlite_database: self.backend_options.sqlite_database.clone(),
            search_database: self.backend_options.search_database.clone(),
            search_table: self.backend_options.search_table.clone(),
        }
    }
}

/// Every command Wayfind answers.
#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Create the database and register this project.
    Init,

    /// Start or finish an initiative.
    Initiative {
        /// Which one.
        #[command(subcommand)]
        command: InitiativeCommand,
    },

    /// Show the initiative: frontier, decisions, fog, and exclusions.
    Map,

    /// Draw the dependency graph.
    Tree,

    /// Name the one ticket to work on now.
    Next,

    /// Write the digest that hands this initiative to someone else.
    Handoff,

    /// Show a ticket, or act on one.
    #[command(args_conflicts_with_subcommands = true)]
    Ticket {
        /// The ticket to show.
        #[arg(value_name = "ID")]
        id: Option<i64>,

        /// What to do to a ticket instead of showing it.
        #[command(subcommand)]
        command: Option<TicketCommand>,
    },

    /// Work with the documents attached to tickets.
    #[command(alias = "attachment")]
    Attach {
        /// Which one.
        #[command(subcommand)]
        command: AttachCommand,
    },

    /// Work with this session.
    Session {
        /// Which one.
        #[command(subcommand)]
        command: SessionCommand,
    },

    /// List the sessions on this initiative.
    Sessions {
        /// Which one.
        #[command(subcommand)]
        command: SessionsCommand,
    },

    /// Record what is not yet specified.
    Fog {
        /// Which one.
        #[command(subcommand)]
        command: FogCommand,
    },

    /// Record what is out of scope.
    Scope {
        /// Which one.
        #[command(subcommand)]
        command: ScopeCommand,
    },

    /// Search this initiative's tickets.
    Search {
        /// What to look for, in FTS5 query syntax.
        #[arg(value_name = "TERMS")]
        query: String,

        /// How many results to return.
        #[arg(long, default_value_t = 10, value_name = "N")]
        limit: u32,

        /// How many results to skip first.
        #[arg(long, default_value_t = 0, value_name = "N")]
        offset: u32,
    },

    /// Write this initiative's tickets as records.
    Dump {
        /// Write comma-separated records. Required, and the only format.
        #[arg(long, required = true)]
        csv: bool,

        /// How many tickets to write.
        #[arg(long, default_value_t = 100, value_name = "N")]
        limit: u32,

        /// How many tickets to skip first.
        #[arg(long, default_value_t = 0, value_name = "N")]
        offset: u32,
    },
}

/// What can be done to an initiative.
#[derive(Debug, Clone, Subcommand)]
pub enum InitiativeCommand {
    /// Start an initiative and make it the project's active one.
    Create {
        /// What it is called.
        #[arg(long, value_name = "NAME")]
        name: String,

        /// What finishing it looks like.
        #[arg(long, value_name = "TEXT")]
        destination: String,

        /// Anything else worth carrying.
        #[arg(long, value_name = "TEXT")]
        notes: Option<String>,
    },

    /// Declare the map clear. Refused while a ticket is still open or claimed.
    Clear,
}

/// What can be done to a ticket.
#[derive(Debug, Clone, Subcommand)]
pub enum TicketCommand {
    /// Add a ticket to the initiative.
    Create {
        /// One line naming the ticket.
        #[arg(long, value_name = "TEXT")]
        title: String,

        /// What kind of work it is.
        #[arg(long, value_name = "TYPE", value_enum)]
        r#type: TicketTypeArg,

        /// The question it must answer.
        #[arg(long, value_name = "TEXT")]
        question: String,
    },

    /// Take a ticket for this session.
    Claim {
        /// Which ticket.
        #[arg(value_name = "ID")]
        id: i64,
    },

    /// Record the decision that closes a claimed ticket.
    Resolve {
        /// Which ticket.
        #[arg(value_name = "ID")]
        id: i64,

        /// Where the decision text comes from.
        #[command(flatten)]
        resolution: ResolutionSource,
    },

    /// Repair the text of a decision that is already recorded.
    Amend {
        /// Which ticket.
        #[arg(value_name = "ID")]
        id: i64,

        /// Where the replacement text comes from.
        #[command(flatten)]
        resolution: ResolutionSource,
    },

    /// Record that a ticket cannot start until another one is resolved.
    Block {
        /// The ticket that must wait.
        #[arg(value_name = "ID")]
        id: i64,

        /// The ticket it waits for.
        #[arg(long = "by", value_name = "BLOCKER")]
        blocker: i64,
    },
}

/// What can be done to an attachment.
#[derive(Debug, Clone, Subcommand)]
pub enum AttachCommand {
    /// Attach a document to a ticket.
    Add {
        /// The ticket that owns it.
        #[arg(value_name = "TICKET")]
        ticket: i64,

        /// The file to read, or `-` for standard input.
        #[arg(long, value_name = "PATH")]
        file: PathBuf,

        /// What the document is for.
        #[arg(long, value_name = "TEXT")]
        description: String,

        /// What to call it, instead of the file's own name.
        #[arg(long, value_name = "NAME")]
        name: Option<String>,

        /// Delete the source file once the stored copy matches it.
        #[arg(long = "move")]
        move_source: bool,
    },

    /// Point another ticket at an attachment it did not create.
    Ref {
        /// The ticket that gets the reference.
        #[arg(value_name = "TICKET")]
        ticket: i64,

        /// Which attachment.
        #[arg(long, value_name = "ID")]
        attachment: i64,
    },

    /// Drop a reference. The attachment itself stays.
    Unref {
        /// The ticket that loses the reference.
        #[arg(value_name = "TICKET")]
        ticket: i64,

        /// Which attachment.
        #[arg(long, value_name = "ID")]
        attachment: i64,
    },

    /// List this initiative's attachments.
    List {
        /// Only the ones on this ticket.
        #[arg(value_name = "TICKET")]
        ticket: Option<i64>,
    },

    /// Print an attachment.
    Show {
        /// Which attachment.
        #[arg(value_name = "ID")]
        id: i64,

        /// Print the stored bytes alone, with no heading.
        #[arg(long)]
        raw: bool,
    },

    /// Delete an attachment and everything pointing at it.
    Rm {
        /// Which attachment.
        #[arg(value_name = "ID")]
        id: i64,
    },
}

/// What can be done with this session.
#[derive(Debug, Clone, Subcommand)]
pub enum SessionCommand {
    /// Report where this session left off.
    Resume,

    /// List the sessions on this initiative.
    List,
}

/// What can be done with the session list.
#[derive(Debug, Clone, Subcommand)]
pub enum SessionsCommand {
    /// List the sessions on this initiative.
    List,
}

/// What can be added to the fog.
#[derive(Debug, Clone, Subcommand)]
pub enum FogCommand {
    /// Record something the initiative has not specified yet.
    Add {
        /// What is unspecified.
        #[arg(long, value_name = "TEXT")]
        note: String,
    },
}

/// What can be excluded from scope.
#[derive(Debug, Clone, Subcommand)]
pub enum ScopeCommand {
    /// Record something this initiative deliberately will not do.
    Exclude {
        /// What is out of scope.
        #[arg(long, value_name = "TEXT")]
        note: String,
    },
}

/// Where a decision's text comes from. Exactly one of the two is given.
#[derive(Debug, Clone, Args)]
#[group(required = true, multiple = false)]
pub struct ResolutionSource {
    /// The decision, typed on the command line.
    #[arg(long, value_name = "TEXT")]
    pub resolution: Option<String>,

    /// A file holding the decision, or `-` for standard input.
    #[arg(long = "resolution-file", value_name = "PATH")]
    pub resolution_file: Option<PathBuf>,
}

/// Where text is read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextSource {
    /// It was typed on the command line.
    Inline(String),
    /// It is in this file.
    File(PathBuf),
    /// It is arriving on standard input.
    Stdin,
}

/// The name that means standard input wherever a path is accepted.
pub const STDIN_PATH: &str = "-";

/// Read a path as a source, treating `-` as standard input.
pub fn text_source(path: PathBuf) -> TextSource {
    if path.as_os_str() == STDIN_PATH {
        return TextSource::Stdin;
    }
    TextSource::File(path)
}

impl ResolutionSource {
    /// Where the decision text will come from.
    ///
    /// Clap has already refused both and neither, so one of the two is present.
    /// The remaining branch cannot be reached; it reads standard input, which
    /// is the harmless choice if a future edit to the group ever lets it
    /// through.
    pub fn source(&self) -> TextSource {
        if let Some(text) = &self.resolution {
            return TextSource::Inline(text.clone());
        }
        match &self.resolution_file {
            Some(path) => text_source(path.clone()),
            None => TextSource::Stdin,
        }
    }
}

/// A ticket type as the command line spells it.
///
/// The core's [`TicketType`] is deliberately unaware of Clap, so this mirrors
/// it. Keeping the mirror here is what makes `--type` list its choices in
/// `--help` and refuse an unknown one before anything is opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum TicketTypeArg {
    /// A question to grill an assumption with.
    Grilling,
    /// An investigation that gathers facts without settling design.
    Research,
    /// A build-to-learn spike.
    Prototype,
    /// Ordinary work with a definite finish.
    Task,
}

impl From<TicketTypeArg> for TicketType {
    fn from(value: TicketTypeArg) -> Self {
        match value {
            TicketTypeArg::Grilling => TicketType::Grilling,
            TicketTypeArg::Research => TicketType::Research,
            TicketTypeArg::Prototype => TicketType::Prototype,
            TicketTypeArg::Task => TicketType::Task,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::path::PathBuf;

    use clap::{CommandFactory, Parser};

    use super::{Cli, Command, TextSource, TicketCommand, TicketTypeArg};

    fn parse(argv: &[&str]) -> Cli {
        Cli::parse_from(argv)
    }

    #[test]
    fn the_command_line_description_is_self_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn a_global_setting_reads_the_same_on_either_side_of_the_command() {
        let before = parse(&["wayfind", "--initiative", "4", "map"]);
        let after = parse(&["wayfind", "map", "--initiative", "4"]);
        assert_eq!(before.globals.initiative, Some(4));
        assert_eq!(after.globals.initiative, Some(4));
    }

    #[test]
    fn a_dotted_backend_setting_becomes_a_configuration_layer() {
        let cli = parse(&[
            "wayfind",
            "--sqlite.database",
            "/tmp/wayfind.sqlite",
            "--sqlite-fts5.table",
            "other_search",
            "map",
        ]);
        let source = cli.globals.config_source();
        assert_eq!(
            source.sqlite_database,
            Some(PathBuf::from("/tmp/wayfind.sqlite"))
        );
        assert_eq!(source.search_table.as_deref(), Some("other_search"));
        assert_eq!(source.backend, None);
    }

    #[test]
    fn a_bare_ticket_id_shows_the_ticket_and_a_word_names_a_command() {
        let shown = parse(&["wayfind", "ticket", "12"]);
        assert!(matches!(
            shown.command,
            Command::Ticket {
                id: Some(12),
                command: None
            }
        ));

        let created = parse(&[
            "wayfind",
            "ticket",
            "create",
            "--title",
            "Pick a store",
            "--type",
            "grilling",
            "--question",
            "Which one?",
        ]);
        let Command::Ticket {
            command: Some(TicketCommand::Create { r#type, title, .. }),
            ..
        } = created.command
        else {
            panic!("ticket create");
        };
        assert_eq!(r#type, TicketTypeArg::Grilling);
        assert_eq!(title, "Pick a store");
    }

    #[test]
    fn an_unknown_ticket_type_is_refused_before_anything_is_opened() {
        let refusal = Cli::try_parse_from([
            "wayfind",
            "ticket",
            "create",
            "--title",
            "t",
            "--type",
            "guesswork",
            "--question",
            "q",
        ])
        .expect_err("an unknown type");
        assert!(refusal.to_string().contains("guesswork"));
    }

    #[test]
    fn a_decision_comes_from_the_command_line_a_file_or_standard_input() {
        let inline = parse(&["wayfind", "ticket", "resolve", "3", "--resolution", "done"]);
        let Command::Ticket {
            command: Some(TicketCommand::Resolve { resolution, .. }),
            ..
        } = inline.command
        else {
            panic!("ticket resolve");
        };
        assert_eq!(resolution.source(), TextSource::Inline("done".to_string()));

        let from_file = parse(&[
            "wayfind",
            "ticket",
            "resolve",
            "3",
            "--resolution-file",
            "/tmp/decision.md",
        ]);
        let Command::Ticket {
            command: Some(TicketCommand::Resolve { resolution, .. }),
            ..
        } = from_file.command
        else {
            panic!("ticket resolve");
        };
        assert_eq!(
            resolution.source(),
            TextSource::File(PathBuf::from("/tmp/decision.md"))
        );

        let from_stdin = parse(&[
            "wayfind",
            "ticket",
            "resolve",
            "3",
            "--resolution-file",
            "-",
        ]);
        let Command::Ticket {
            command: Some(TicketCommand::Resolve { resolution, .. }),
            ..
        } = from_stdin.command
        else {
            panic!("ticket resolve");
        };
        assert_eq!(resolution.source(), TextSource::Stdin);
    }

    #[test]
    fn a_decision_cannot_come_from_two_places_or_none() {
        assert!(Cli::try_parse_from([
            "wayfind",
            "ticket",
            "resolve",
            "3",
            "--resolution",
            "done",
            "--resolution-file",
            "/tmp/x",
        ])
        .is_err());
        assert!(Cli::try_parse_from(["wayfind", "ticket", "resolve", "3"]).is_err());
    }

    #[test]
    fn dump_writes_records_only_when_asked_for_them() {
        let cli = parse(&["wayfind", "dump", "--csv"]);
        assert!(matches!(
            cli.command,
            Command::Dump {
                csv: true,
                limit: 100,
                offset: 0
            }
        ));
        assert!(Cli::try_parse_from(["wayfind", "dump"]).is_err());
    }

    #[test]
    fn search_counts_from_the_same_place_the_script_did() {
        let cli = parse(&["wayfind", "search", "storage backend"]);
        assert!(matches!(
            cli.command,
            Command::Search {
                limit: 10,
                offset: 0,
                ..
            }
        ));
    }

    #[test]
    fn the_script_s_spelling_of_attach_still_works() {
        let cli = parse(&["wayfind", "attachment", "list"]);
        assert!(matches!(cli.command, Command::Attach { .. }));
    }

    #[test]
    fn help_and_version_are_answered_without_a_command() {
        for flag in ["--help", "--version"] {
            let outcome = Cli::try_parse_from(["wayfind", flag]).expect_err("an early exit");
            assert!(matches!(
                outcome.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ));
        }
    }
}
