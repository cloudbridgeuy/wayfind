//! Collecting the configuration layers from the world outside.
//!
//! There is no decision here. This module reads a file, reads the environment,
//! and works out where the default database lives; every one of those is an
//! effect, and none of them is tested. What the layers *mean* is decided by
//! [`wayfind_core::config::resolve_config`], which touches nothing.
//!
//! Paths are used exactly as they were given. A `~` or a `$VAR` inside a
//! configured path is not expanded: the shell that invoked Wayfind already does
//! that for anything typed on a command line, and quietly expanding what is
//! left would make a path mean two different things depending on who wrote it.

use std::path::{Path, PathBuf};

use wayfind_core::config::{resolve_config, ConfigInput, ConfigSource, RawConfig, ResolvedConfig};

use crate::error::{ShellError, ShellResult};

/// The directory Wayfind keeps its files in, under the configuration home.
///
/// v2 shares this directory with v1. The two stores are separate files inside
/// it, so an operator who runs both keeps one place to look.
pub const APP_DIRECTORY: &str = "wayfind";

/// The configuration file's name.
pub const CONFIG_FILE: &str = "config.v2.toml";

/// The database file's name.
pub const DATABASE_FILE: &str = "wayfind2.sqlite";

/// The prefix every Wayfind environment variable carries.
pub const ENV_PREFIX: &str = "WAYFIND_";

/// What the shell knows before it decides anything.
pub struct ConfigContext {
    /// A configuration file named on the command line, if there was one.
    pub explicit_file: Option<PathBuf>,
    /// What the command line said about paths.
    pub cli: ConfigSource,
}

/// Read every layer and decide what to open.
///
/// An explicitly named configuration file must exist and must parse. The
/// default one may be missing — a fresh machine has no configuration and still
/// runs — but a default file that exists and does not parse is an error, the
/// same as an explicit one.
pub fn load_config(context: &ConfigContext) -> ShellResult<ResolvedConfig> {
    let home = config_home();

    let file = match &context.explicit_file {
        Some(path) => read_config_file(path)?,
        None => match &home {
            Some(directory) => read_optional_config_file(&directory.join(CONFIG_FILE))?,
            None => ConfigSource::default(),
        },
    };

    let input = ConfigInput {
        defaults: defaults(home.as_deref()),
        file,
        environment: environment(),
        cli: context.cli.clone(),
    };

    // A machine with neither variable set has no default database, and the core
    // would only be able to say that a path is missing. Say why it is missing.
    if home.is_none() && input.clone().merge().database.is_none() {
        return Err(ShellError::NoConfigHome);
    }

    Ok(resolve_config(input)?)
}

/// Where Wayfind looks when nothing points it anywhere.
///
/// `$XDG_CONFIG_HOME/wayfind` if that is set, `$HOME/.config/wayfind`
/// otherwise, and nowhere at all if neither variable is set.
pub fn config_home() -> Option<PathBuf> {
    if let Some(xdg) = non_empty_var("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(xdg).join(APP_DIRECTORY));
    }
    non_empty_var("HOME").map(|home| PathBuf::from(home).join(".config").join(APP_DIRECTORY))
}

/// The database Wayfind opens when no layer names one.
fn defaults(home: Option<&Path>) -> ConfigSource {
    ConfigSource {
        database: home.map(|directory| directory.join(DATABASE_FILE)),
        ..ConfigSource::default()
    }
}

/// Read a configuration file the operator named. Missing is an error.
fn read_config_file(path: &Path) -> ShellResult<ConfigSource> {
    let text = std::fs::read_to_string(path).map_err(|source| ShellError::ConfigUnreadable {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(RawConfig::from_toml(&text)?.into_source())
}

/// Read the default configuration file. Missing is fine; malformed is not.
fn read_optional_config_file(path: &Path) -> ShellResult<ConfigSource> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(RawConfig::from_toml(&text)?.into_source()),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(ConfigSource::default()),
        Err(source) => Err(ShellError::ConfigUnreadable {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Read the layer the environment supplies.
///
/// A variable is named after the setting it sets, with `__` where the
/// configuration file has a dot and `_` where it has a dash: `[sqlite-fts5]
/// database` is `WAYFIND_SQLITE_FTS5__DATABASE`. An empty variable says
/// nothing, so `WAYFIND_SQLITE__DATABASE=` in a wrapper script does not point
/// Wayfind at a file named "".
fn environment() -> ConfigSource {
    ConfigSource {
        database: env_setting("SQLITE__DATABASE").map(PathBuf::from),
        fts_database: env_setting("SQLITE_FTS5__DATABASE").map(PathBuf::from),
    }
}

fn env_setting(suffix: &str) -> Option<String> {
    non_empty_var(&format!("{ENV_PREFIX}{suffix}"))
}

fn non_empty_var(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(value) if !value.is_empty() => Some(value),
        _ => None,
    }
}
