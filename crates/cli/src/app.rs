//! The composition root.
//!
//! One place decides which handler answers a parsed command line, and one place
//! turns whatever comes back into what the operator sees. Keeping both here is
//! what lets `main` be four lines and lets a test drive the whole program
//! without a process.

use std::process::ExitCode;

use wayfind_core::render;

use crate::{
    args::{
        graph::{GraphCommand, SnapshotCommand},
        initiative::InitiativeCommand,
        retired::{
            RetiredAttachCommand, RetiredFogCommand, RetiredScopeCommand, RetiredSessionCommand,
        },
        ticket::{TicketArgs, TicketCommand},
        Cli, Command,
    },
    commands::{self, retired, Shell},
    config::{self, ConfigContext},
    context::{self, Environment},
    error::{ShellError, ShellResult},
    output::Output,
    sqlite::{graph_write::SqliteGraph, SqliteStore},
};

/// The exit code a failure that is not a refusal ends with.
///
/// Class 1 is reserved for faults the operator cannot answer — a broken pipe, an
/// unreadable file. It is never contractual behavior.
const INTERNAL_ERROR: u8 = 1;

/// Carry out one command line.
///
/// Configuration is resolved and the project key derived before anything is
/// opened, so a command aimed at a database that cannot be named fails before
/// it touches a disk. Only `init` is allowed to bring the file into being;
/// every other command opens what is there, so a mistyped path is reported
/// instead of quietly leaving an empty store behind.
pub fn run(cli: &Cli, environment: &dyn Environment, out: &mut dyn Output) -> ShellResult<()> {
    let resolved = config::load_config(&ConfigContext {
        explicit_file: cli.globals.config.clone(),
        cli: cli.globals.config_source(),
    })?;
    let project = context::project_key(environment, cli.globals.project.as_deref())?;

    let already_existed = resolved.database.exists();
    let store = if matches!(cli.command, Command::Init) {
        SqliteStore::initialize(&resolved.database)?
    } else {
        SqliteStore::open(&resolved.database)?
    };

    let graph = SqliteGraph::new(store.connection());
    let shell = Shell {
        store: &store,
        graph: &graph,
        coordination: &graph,
        environment,
        database: resolved.database.clone(),
        project,
        chosen_session: cli.globals.session.clone(),
        chosen_initiative: cli.globals.initiative,
        now: environment.now(),
    };

    match &cli.command {
        Command::Init => commands::init::run(&shell, already_existed, out),

        Command::Initiative {
            command:
                InitiativeCommand::Create {
                    name,
                    destination,
                    notes,
                },
        } => commands::initiative::create(&shell, name, destination, notes.as_deref(), out),

        Command::Initiative {
            command: InitiativeCommand::List,
        } => commands::initiative::list(&shell, out),

        Command::Initiative {
            command: InitiativeCommand::Show { id },
        } => commands::initiative::show(&shell, *id, out),

        Command::Initiative {
            command: InitiativeCommand::Clear(args),
        } => Err(retired::refuse(&["initiative", "clear"], &args.rest)),

        Command::Graph {
            command: GraphCommand::Show { snapshot },
        } => commands::graph::show(&shell, snapshot.as_deref(), out),

        Command::Snapshot {
            command: SnapshotCommand::List,
        } => commands::snapshot::list(&shell, out),

        Command::Snapshot {
            command: SnapshotCommand::Show { snapshot },
        } => commands::snapshot::show(&shell, snapshot, out),

        Command::Ticket(TicketArgs {
            command: Some(TicketCommand::Claim(rest)),
            ..
        }) => Err(retired::refuse(&["ticket", "claim"], &rest.rest)),

        Command::Ticket(TicketArgs {
            command: Some(TicketCommand::Resolve(rest)),
            ..
        }) => Err(retired::refuse(&["ticket", "resolve"], &rest.rest)),

        Command::Ticket(TicketArgs {
            command: Some(TicketCommand::Amend(rest)),
            ..
        }) => Err(retired::refuse(&["ticket", "amend"], &rest.rest)),

        Command::Ticket(TicketArgs {
            command: Some(TicketCommand::Block(rest)),
            ..
        }) => Err(retired::refuse(&["ticket", "block"], &rest.rest)),

        Command::Map(args) => Err(retired::refuse(&["map"], &args.rest)),
        Command::Tree(args) => Err(retired::refuse(&["tree"], &args.rest)),
        Command::Next(args) => Err(retired::refuse(&["next"], &args.rest)),
        Command::Handoff(args) => Err(retired::refuse(&["handoff"], &args.rest)),

        Command::Session {
            command: RetiredSessionCommand::Resume(args),
        } => Err(retired::refuse(&["session", "resume"], &args.rest)),

        Command::Session {
            command: RetiredSessionCommand::List(args),
        } => Err(retired::refuse(&["session", "list"], &args.rest)),

        Command::Fog {
            command: RetiredFogCommand::Add(args),
        } => Err(retired::refuse(&["fog", "add"], &args.rest)),

        Command::Scope {
            command: RetiredScopeCommand::Exclude(args),
        } => Err(retired::refuse(&["scope", "exclude"], &args.rest)),

        Command::Attach {
            command: RetiredAttachCommand::Add(args),
        } => Err(retired::refuse(&["attach", "add"], &args.rest)),

        Command::Attach {
            command: RetiredAttachCommand::Ref(args),
        } => Err(retired::refuse(&["attach", "ref"], &args.rest)),

        Command::Attach {
            command: RetiredAttachCommand::Unref(args),
        } => Err(retired::refuse(&["attach", "unref"], &args.rest)),

        Command::Attach {
            command: RetiredAttachCommand::List(args),
        } => Err(retired::refuse(&["attach", "list"], &args.rest)),

        Command::Attach {
            command: RetiredAttachCommand::Show(args),
        } => Err(retired::refuse(&["attach", "show"], &args.rest)),

        Command::Attach {
            command: RetiredAttachCommand::Rm(args),
        } => Err(retired::refuse(&["attach", "rm"], &args.rest)),

        Command::Migrate { .. }
        | Command::Initiative {
            command: InitiativeCommand::Close { .. },
        }
        | Command::Initiative {
            command: InitiativeCommand::Clone { .. },
        }
        | Command::Initiative {
            command: InitiativeCommand::Import { .. },
        }
        | Command::Snapshot {
            command: SnapshotCommand::Diff { .. },
        }
        | Command::Graph {
            command: GraphCommand::Frontier { .. },
        }
        | Command::Graph {
            command: GraphCommand::History,
        }
        | Command::Graph {
            command: GraphCommand::Impact { .. },
        }
        | Command::Graph {
            command: GraphCommand::Split { .. },
        }
        | Command::Graph {
            command: GraphCommand::Merge { .. },
        }
        | Command::Graph {
            command: GraphCommand::Block { .. },
        }
        | Command::Graph {
            command: GraphCommand::Recover { .. },
        }
        | Command::Graph {
            command: GraphCommand::Abandon { .. },
        }
        | Command::Graph {
            command: GraphCommand::Supersede { .. },
        }
        | Command::Node { .. }
        | Command::Transition { .. }
        | Command::Artifact { .. }
        | Command::Work { .. }
        | Command::Sessions(_)
        | Command::Run { .. }
        | Command::Ticket(_)
        | Command::Search { .. }
        | Command::Dump { .. } => Err(commands::not_implemented()),
    }
}

/// Write a failure where the operator will find it, and say how it ended.
///
/// A refusal is an error document on standard error with the token the caller
/// matches on. Anything else is one line and exit 1, because there is no token
/// for it and inventing one would be a promise the program cannot keep.
pub fn report(error: &ShellError, err: &mut dyn Output) -> ExitCode {
    match error.rejection() {
        Some(rejection) => {
            let _ = err.text(&render::error::document(rejection));
            let _ = err.flush();
            ExitCode::from(rejection.exit_code())
        }
        None => {
            let _ = err.text(&format!("{error}\n"));
            let _ = err.flush();
            ExitCode::from(INTERNAL_ERROR)
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use clap::Parser;

    use super::*;
    use crate::context::SystemEnvironment;

    /// A store already created, so a command under test opens it rather than
    /// tripping the missing-store refusal that [`crate::commands::init`]'s own
    /// tests, and the `tests/init.rs` integration test, cover on their own.
    fn existing_store() -> (tempfile::TempDir, String) {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("wayfind2.sqlite");
        SqliteStore::initialize(&database).unwrap();
        let path = database.to_str().unwrap().to_string();
        (directory, path)
    }

    fn refusal_for(arguments: &[&str]) -> ShellError {
        let (_directory, database) = existing_store();
        let mut full = vec!["wayfind2", "--sqlite.database", database.as_str()];
        full.extend_from_slice(arguments);

        let cli = Cli::try_parse_from(full).unwrap();
        let mut out: Vec<u8> = Vec::new();
        run(&cli, &SystemEnvironment::new(), &mut out).unwrap_err()
    }

    #[test]
    fn a_command_this_slice_does_not_carry_out_refuses_with_the_usage_token() {
        let error = refusal_for(&["migrate"]);
        let rejection = error.rejection().unwrap();
        assert_eq!(rejection.exit_code(), 2);
        assert_eq!(rejection.body_text(), Some("not implemented in this slice"));
    }

    #[test]
    fn a_refusal_is_reported_as_an_error_document_with_its_own_exit_code() {
        let error = refusal_for(&["graph", "history"]);
        let mut err: Vec<u8> = Vec::new();
        let code = report(&error, &mut err);

        let document = String::from_utf8(err).unwrap();
        assert!(document.starts_with("+++\n"));
        assert!(document.contains("kind = \"error\""));
        assert!(document.contains("error = \"usage\""));
        assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::from(2)));
    }

    #[test]
    fn init_creates_a_store_and_says_so_then_says_it_already_exists() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("nested/wayfind2.sqlite");
        let path = database.to_str().unwrap();

        let cli = Cli::try_parse_from(["wayfind2", "--sqlite.database", path, "init"]).unwrap();
        let mut first: Vec<u8> = Vec::new();
        run(&cli, &SystemEnvironment::new(), &mut first).unwrap();
        assert_eq!(
            String::from_utf8(first).unwrap(),
            format!("created a store at {}\n", database.display())
        );

        let mut second: Vec<u8> = Vec::new();
        run(&cli, &SystemEnvironment::new(), &mut second).unwrap();
        assert_eq!(
            String::from_utf8(second).unwrap(),
            format!("a store already exists at {}\n", database.display())
        );
    }

    #[test]
    fn a_command_against_a_missing_store_refuses_with_the_not_found_token() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("wayfind2.sqlite");
        let path = database.to_str().unwrap();

        let cli =
            Cli::try_parse_from(["wayfind2", "--sqlite.database", path, "initiative", "list"])
                .unwrap();
        let mut out: Vec<u8> = Vec::new();
        let error = run(&cli, &SystemEnvironment::new(), &mut out).unwrap_err();

        let rejection = error.rejection().unwrap();
        assert_eq!(rejection.exit_code(), 3);
        assert!(rejection
            .body_text()
            .is_some_and(|body| body.contains("wayfind2 init")));
    }
}
