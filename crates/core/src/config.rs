//! Deciding what Wayfind was told to open, before anything is opened.
//!
//! Settings arrive from four places — built-in defaults, a configuration file,
//! the environment, and the command line — and the later ones win. Merging them
//! and checking that what came out is usable is arithmetic over values, so it
//! lives here, where it can be tested without a file or an environment.
//!
//! The shell's job is only to collect the layers: read the file, read the
//! environment, read the flags, and hand over four [`ConfigSource`] values.
//!
//! v2 has one store and one index, and the index lives inside the store's file
//! unless it is pointed elsewhere. There is nothing to select, so what comes out
//! is two paths.

use std::path::PathBuf;

use serde::Deserialize;

use crate::error::{ErrorToken, Rejection};
use crate::render::Field;

/// A configuration file, exactly as written.
///
/// Unknown keys are refused rather than ignored. A misspelled key that is
/// quietly dropped looks identical to a setting that had no effect, and the
/// operator finds out only when the wrong database is opened.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawConfig {
    /// Settings for the SQLite store.
    #[serde(default)]
    pub sqlite: Option<RawSqlite>,
    /// Settings for the FTS5 index.
    #[serde(default, rename = "sqlite-fts5")]
    pub sqlite_fts5: Option<RawSqliteFts5>,
}

/// The `[sqlite]` section.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawSqlite {
    /// Where the database file is.
    #[serde(default)]
    pub database: Option<String>,
}

/// The `[sqlite-fts5]` section.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawSqliteFts5 {
    /// Where the indexed database file is. Defaults to the store's file.
    #[serde(default)]
    pub database: Option<String>,
}

impl RawConfig {
    /// The operator-facing name of this value, used in errors.
    pub const FIELD: &'static str = "configuration file";

    /// Read a configuration file's text.
    pub fn from_toml(text: &str) -> Result<Self, Rejection> {
        toml::from_str(text).map_err(|error| {
            Rejection::new(ErrorToken::Usage)
                .key("field", Field::Text(Self::FIELD.to_string()))
                .body(error.to_string())
        })
    }

    /// Flatten the file into one layer of settings.
    pub fn into_source(self) -> ConfigSource {
        ConfigSource {
            database: self.sqlite.unwrap_or_default().database.map(PathBuf::from),
            fts_database: self
                .sqlite_fts5
                .unwrap_or_default()
                .database
                .map(PathBuf::from),
        }
    }
}

/// One layer of settings. Every field is optional, because a layer says only
/// what it was told.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigSource {
    /// Where the SQLite database file is.
    pub database: Option<PathBuf>,
    /// Where the searched database file is.
    pub fts_database: Option<PathBuf>,
}

impl ConfigSource {
    /// Lay another source over this one. Anything the other says wins.
    pub fn overlay(mut self, other: ConfigSource) -> Self {
        self.database = other.database.or(self.database);
        self.fts_database = other.fts_database.or(self.fts_database);
        self
    }
}

/// The four layers, in the order they are laid down.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigInput {
    /// What Wayfind uses when nothing says otherwise.
    pub defaults: ConfigSource,
    /// What the configuration file said.
    pub file: ConfigSource,
    /// What the environment said.
    pub environment: ConfigSource,
    /// What the command line said.
    pub cli: ConfigSource,
}

impl ConfigInput {
    /// Collapse the layers into one, later layers winning.
    pub fn merge(self) -> ConfigSource {
        self.defaults
            .overlay(self.file)
            .overlay(self.environment)
            .overlay(self.cli)
    }
}

/// What Wayfind will open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedConfig {
    /// The store's database file.
    pub database: PathBuf,
    /// The database file holding the search index.
    pub fts_database: PathBuf,
}

/// Merge the layers and check that what is left names a store.
///
/// The index defaults to the store's file, because the FTS5 table lives inside
/// it. Pointing the index somewhere else is allowed and is the operator's
/// decision.
pub fn resolve_config(input: ConfigInput) -> Result<ResolvedConfig, Rejection> {
    let merged = input.merge();

    let database = merged.database.ok_or_else(|| {
        Rejection::new(ErrorToken::Usage)
            .key("field", Field::Text("sqlite.database".to_string()))
            .body(
                "Wayfind needs a database path. Set `[sqlite] database` in the \
                 configuration file, or pass --sqlite.database.",
            )
    })?;
    let fts_database = merged.fts_database.unwrap_or_else(|| database.clone());

    Ok(ResolvedConfig {
        database,
        fts_database,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::path::PathBuf;

    use super::{resolve_config, ConfigInput, ConfigSource, RawConfig, ResolvedConfig};

    fn path(text: &str) -> PathBuf {
        PathBuf::from(text)
    }

    fn with_database(text: &str) -> ConfigSource {
        ConfigSource {
            database: Some(path(text)),
            ..ConfigSource::default()
        }
    }

    fn resolve(input: ConfigInput) -> ResolvedConfig {
        resolve_config(input).expect("a resolvable configuration")
    }

    #[test]
    fn a_flag_wins_over_a_default() {
        let resolved = resolve(ConfigInput {
            defaults: with_database("/defaults.sqlite"),
            cli: with_database("/cli.sqlite"),
            ..ConfigInput::default()
        });
        assert_eq!(resolved.database, path("/cli.sqlite"));
    }

    #[test]
    fn a_later_layer_wins_over_an_earlier_one() {
        let resolved = resolve(ConfigInput {
            defaults: with_database("/defaults.sqlite"),
            file: with_database("/file.sqlite"),
            environment: with_database("/environment.sqlite"),
            cli: with_database("/cli.sqlite"),
        });
        assert_eq!(resolved.database, path("/cli.sqlite"));
    }

    #[test]
    fn a_layer_that_says_nothing_leaves_the_layer_below_it_alone() {
        let resolved = resolve(ConfigInput {
            defaults: with_database("/defaults.sqlite"),
            ..ConfigInput::default()
        });
        assert_eq!(resolved.database, path("/defaults.sqlite"));
    }

    #[test]
    fn each_setting_is_decided_on_its_own() {
        let resolved = resolve(ConfigInput {
            defaults: with_database("/defaults.sqlite"),
            cli: ConfigSource {
                fts_database: Some(path("/index.sqlite")),
                ..ConfigSource::default()
            },
            ..ConfigInput::default()
        });
        assert_eq!(resolved.database, path("/defaults.sqlite"));
        assert_eq!(resolved.fts_database, path("/index.sqlite"));
    }

    #[test]
    fn the_index_reads_the_store_s_file_unless_it_is_pointed_elsewhere() {
        let resolved = resolve(ConfigInput {
            defaults: with_database("/wayfind2.sqlite"),
            ..ConfigInput::default()
        });
        assert_eq!(resolved.fts_database, path("/wayfind2.sqlite"));
    }

    #[test]
    fn a_configuration_that_names_no_store_is_refused_by_field() {
        let refusal = resolve_config(ConfigInput::default()).expect_err("no database path");
        assert!(refusal.keys().iter().any(|(key, _)| *key == "field"));
        assert!(refusal
            .body_text()
            .unwrap_or_default()
            .contains("--sqlite.database"));
    }

    #[test]
    fn a_file_reads_into_one_layer() {
        let raw = RawConfig::from_toml(
            r#"
            [sqlite]
            database = "/data/wayfind2.sqlite"

            [sqlite-fts5]
            database = "/data/index.sqlite"
            "#,
        )
        .expect("a valid file");
        let source = raw.into_source();
        assert_eq!(source.database, Some(path("/data/wayfind2.sqlite")));
        assert_eq!(source.fts_database, Some(path("/data/index.sqlite")));
    }

    #[test]
    fn an_empty_file_says_nothing_rather_than_failing() {
        let source = RawConfig::from_toml("")
            .expect("an empty file")
            .into_source();
        assert_eq!(source, ConfigSource::default());
    }

    #[test]
    fn a_misspelled_key_is_refused_rather_than_ignored() {
        let refusal = RawConfig::from_toml("[sqlite]\ndatabse = \"/x.sqlite\"\n")
            .expect_err("a misspelled key");
        assert!(refusal.body_text().unwrap_or_default().contains("databse"));

        let unknown_section =
            RawConfig::from_toml("[postgres]\nurl = \"...\"\n").expect_err("an unknown section");
        assert!(unknown_section
            .body_text()
            .unwrap_or_default()
            .contains("postgres"));
    }
}
