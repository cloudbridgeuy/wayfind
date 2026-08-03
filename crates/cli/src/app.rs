//! Putting one command together and running it.
//!
//! The order here is the whole of the module's content. Configuration is
//! resolved before anything is opened, so a command aimed at a database that
//! cannot be named fails before it touches a disk. Exactly one store and
//! exactly one search index are opened, so a command can never end up reading
//! one file and writing another. The clock is read once, so everything a single
//! command writes carries a single time. Only then is the command dispatched.

use std::path::Path;

use wayfind_core::{ResolvedConfig, SearchConfig, StorageConfig};

use crate::args::{
    text_source, AttachCommand, Cli, Command, FogCommand, InitiativeCommand, ScopeCommand,
    SessionCommand, SessionsCommand, TicketCommand,
};
use crate::commands::{attachment, initiative, query, session, ticket, Shell};
use crate::config::{self, ConfigContext};
use crate::context::{self, Environment};
use crate::error::{ShellError, ShellResult};
use crate::output::Output;
use crate::sqlite::search::SqliteFts5Search;
use crate::sqlite::SqliteStorage;

/// Carry out one command.
pub fn run(cli: Cli, environment: &dyn Environment, output: &mut dyn Output) -> ShellResult<()> {
    let overrides = cli.globals.config_source();
    let resolved = config::load_config(ConfigContext {
        explicit_file: cli.globals.config.clone(),
        cli: overrides,
    })?;

    let project = context::project_key(environment, cli.globals.project.as_deref())?;
    let StorageConfig::Sqlite(store) = &resolved.storage;
    let SearchConfig::SqliteFts5(index) = &resolved.search;

    // Only the two commands that are allowed to bring a database into being use
    // `initialize`. Everything else opens what is there, so a mistyped path
    // says so instead of leaving an empty database behind it.
    let storage = if creates_database(&cli.command) {
        SqliteStorage::initialize(&store.database)?
    } else {
        SqliteStorage::open(&store.database)?
    };
    let search = SqliteFts5Search::open(index)?;

    let shell = Shell {
        storage: &storage,
        search: &search,
        environment,
        project,
        chosen_session: cli.globals.session.clone(),
        chosen_initiative: cli.globals.initiative,
        now: environment.now(),
    };

    dispatch(&shell, cli.command, &resolved, output)?;
    output.flush()
}

/// Whether this command may create the database file.
fn creates_database(command: &Command) -> bool {
    matches!(
        command,
        Command::Init
            | Command::Initiative {
                command: InitiativeCommand::Create { .. }
            }
    )
}

/// Send one parsed command to its handler.
fn dispatch(
    shell: &Shell<'_>,
    command: Command,
    resolved: &ResolvedConfig,
    output: &mut dyn Output,
) -> ShellResult<()> {
    match command {
        Command::Init => {
            let database: &Path = resolved.storage_database();
            initiative::init(shell, database, &shell.project, output)
        }

        Command::Initiative { command } => match command {
            InitiativeCommand::Create {
                name,
                destination,
                notes,
            } => initiative::create(shell, &name, &destination, notes.as_deref(), output),
            InitiativeCommand::Clear => initiative::clear(shell, output),
        },

        Command::Map => initiative::map(shell, output),
        Command::Tree => initiative::tree(shell, output),
        Command::Next => ticket::next(shell, output),
        Command::Handoff => initiative::handoff(shell, output),

        Command::Ticket { id, command } => match (id, command) {
            (
                _,
                Some(TicketCommand::Create {
                    title,
                    r#type,
                    question,
                }),
            ) => ticket::create(shell, &title, r#type.into(), &question, output),
            (_, Some(TicketCommand::Claim { id })) => ticket::claim(shell, id, output),
            (_, Some(TicketCommand::Resolve { id, resolution })) => {
                ticket::resolve(shell, id, &resolution.source(), output)
            }
            (_, Some(TicketCommand::Amend { id, resolution })) => {
                ticket::amend(shell, id, &resolution.source(), output)
            }
            (_, Some(TicketCommand::Block { id, blocker })) => {
                ticket::block(shell, id, blocker, output)
            }
            (Some(id), None) => ticket::show(shell, id, output),
            (None, None) => Err(ShellError::refused(
                "ticket needs an ID to show, or a subcommand; try `wayfind ticket --help`",
            )),
        },

        Command::Attach { command } => match command {
            AttachCommand::Add {
                ticket,
                file,
                description,
                name,
                move_source,
            } => attachment::add(
                shell,
                ticket,
                &text_source(file),
                &description,
                name.as_deref(),
                move_source,
                output,
            ),
            AttachCommand::Ref { ticket, attachment } => {
                attachment::add_reference(shell, ticket, attachment, output)
            }
            AttachCommand::Unref { ticket, attachment } => {
                attachment::remove_reference(shell, ticket, attachment, output)
            }
            AttachCommand::List { ticket } => attachment::list(shell, ticket, output),
            AttachCommand::Show { id, raw } => attachment::show(shell, id, raw, output),
            AttachCommand::Rm { id } => attachment::remove(shell, id, output),
        },

        Command::Session { command } => match command {
            SessionCommand::Resume => session::resume(shell, output),
            SessionCommand::List => session::list(shell, output),
        },

        Command::Sessions { command } => match command {
            SessionsCommand::List => session::list(shell, output),
        },

        Command::Fog { command } => match command {
            FogCommand::Add { note } => initiative::fog(shell, &note, output),
        },

        Command::Scope { command } => match command {
            ScopeCommand::Exclude { note } => initiative::exclude(shell, &note, output),
        },

        Command::Search {
            query,
            limit,
            offset,
        } => query::search(shell, &query, limit, offset, output),

        // `--csv` is required and is the only format, so it carries no choice.
        Command::Dump {
            csv: _,
            limit,
            offset,
        } => query::dump(shell, limit, offset, output),
    }
}
