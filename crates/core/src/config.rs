//! Deciding what Wayfind was told to use, before anything is opened.
//!
//! Settings arrive from four places — built-in defaults, a configuration file,
//! the environment, and the command line — and the later ones win. Merging them
//! and checking that what came out is usable is arithmetic over values, so it
//! lives here, where it can be tested without a file or an environment.
//!
//! The shell's job is only to collect the layers: read the file, read the
//! environment, read the flags, and hand over four [`ConfigSource`] values.
//!
//! What comes out is a [`ResolvedConfig`], and it names one storage backend and
//! one search backend, each with the settings that backend actually needs. A
//! setting for a backend that was not selected is not an error and not carried
//! forward: it is ignored, so an operator can keep a second backend configured
//! while working on the first.

use std::path::PathBuf;

use serde::Deserialize;

use crate::error::{Error, Result};

/// The backend used when nothing selects one.
pub const DEFAULT_STORAGE_BACKEND: &str = "sqlite";

/// The search backend used when nothing selects one.
pub const DEFAULT_SEARCH_BACKEND: &str = "sqlite-fts5";

/// The FTS5 table the Bash script created and this port keeps writing.
pub const DEFAULT_SEARCH_TABLE: &str = "ticket_search";

/// Backends this build can open.
pub const AVAILABLE_STORAGE_BACKENDS: [&str; 1] = ["sqlite"];

/// Search backends this build can open.
pub const AVAILABLE_SEARCH_BACKENDS: [&str; 1] = ["sqlite-fts5"];

/// Names the configuration format keeps for later, and this build cannot open.
///
/// A file may hold settings for one of these. Selecting one is refused, and the
/// refusal says so, rather than reading as a spelling mistake.
pub const RESERVED_BACKENDS: [&str; 1] = ["dynamodb"];

// ---------------------------------------------------------------------------
// Selectors
// ---------------------------------------------------------------------------

/// Which store holds the data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StorageSelector {
    /// The SQLite file the Bash script created.
    Sqlite,
}

/// Which index answers `wayfind search`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchSelector {
    /// The FTS5 table inside the same SQLite file.
    SqliteFts5,
}

impl StorageSelector {
    /// The name this selector is written as.
    pub fn as_str(self) -> &'static str {
        match self {
            StorageSelector::Sqlite => "sqlite",
        }
    }
}

impl SearchSelector {
    /// The name this selector is written as.
    pub fn as_str(self) -> &'static str {
        match self {
            SearchSelector::SqliteFts5 => "sqlite-fts5",
        }
    }
}

fn refuse(field: &'static str, name: &str, available: &[&str]) -> Error {
    if RESERVED_BACKENDS.contains(&name) {
        return Error::invalid_value(
            field,
            format!("`{name}` is reserved for a later build and cannot be opened yet"),
        );
    }
    Error::invalid_value(
        field,
        format!(
            "unknown backend `{name}`; this build offers {}",
            available.join(", ")
        ),
    )
}

impl std::str::FromStr for StorageSelector {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self> {
        match text {
            "sqlite" => Ok(StorageSelector::Sqlite),
            other => Err(refuse("backend", other, &AVAILABLE_STORAGE_BACKENDS)),
        }
    }
}

impl std::str::FromStr for SearchSelector {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self> {
        match text {
            "sqlite-fts5" => Ok(SearchSelector::SqliteFts5),
            other => Err(refuse("search backend", other, &AVAILABLE_SEARCH_BACKENDS)),
        }
    }
}

// ---------------------------------------------------------------------------
// The file
// ---------------------------------------------------------------------------

/// A configuration file, exactly as written.
///
/// Unknown keys are refused rather than ignored. A misspelled key that is
/// quietly dropped looks identical to a setting that had no effect, and the
/// operator finds out only when the wrong database is opened.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawConfig {
    /// Which store to open.
    #[serde(default)]
    pub backend: Option<String>,
    /// Which index to search.
    #[serde(default)]
    pub search_backend: Option<String>,
    /// Settings for the SQLite store.
    #[serde(default)]
    pub sqlite: Option<RawSqlite>,
    /// Settings for the FTS5 index.
    #[serde(default, rename = "sqlite-fts5")]
    pub sqlite_fts5: Option<RawSqliteFts5>,
    /// Settings kept for a later build. Parsed, never selected.
    #[serde(default)]
    pub dynamodb: Option<RawReserved>,
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
    /// Which FTS5 table to query.
    #[serde(default)]
    pub table: Option<String>,
}

/// A section this build parses and cannot act on.
///
/// Reserved settings are held as raw TOML, which can carry a floating-point
/// number, so this value compares but does not claim a total equality.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct RawReserved {
    /// Whatever the operator wrote, kept so the file still parses.
    #[serde(flatten)]
    pub settings: std::collections::BTreeMap<String, toml::Value>,
}

impl RawConfig {
    /// The operator-facing name of this value, used in errors.
    pub const FIELD: &'static str = "configuration file";

    /// Read a configuration file's text.
    pub fn from_toml(text: &str) -> Result<Self> {
        toml::from_str(text).map_err(|error| Error::invalid_value(Self::FIELD, error.to_string()))
    }

    /// Flatten the file into one layer of settings.
    pub fn into_source(self) -> ConfigSource {
        let sqlite = self.sqlite.unwrap_or_default();
        let search = self.sqlite_fts5.unwrap_or_default();
        ConfigSource {
            backend: self.backend,
            search_backend: self.search_backend,
            sqlite_database: sqlite.database.map(PathBuf::from),
            search_database: search.database.map(PathBuf::from),
            search_table: search.table,
        }
    }
}

// ---------------------------------------------------------------------------
// Layers
// ---------------------------------------------------------------------------

/// One layer of settings. Every field is optional, because a layer says only
/// what it was told.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigSource {
    /// Which store to open.
    pub backend: Option<String>,
    /// Which index to search.
    pub search_backend: Option<String>,
    /// Where the SQLite database file is.
    pub sqlite_database: Option<PathBuf>,
    /// Where the searched database file is.
    pub search_database: Option<PathBuf>,
    /// Which FTS5 table to query.
    pub search_table: Option<String>,
}

impl ConfigSource {
    /// Lay another source over this one. Anything the other says wins.
    pub fn overlay(mut self, other: ConfigSource) -> Self {
        self.backend = other.backend.or(self.backend);
        self.search_backend = other.search_backend.or(self.search_backend);
        self.sqlite_database = other.sqlite_database.or(self.sqlite_database);
        self.search_database = other.search_database.or(self.search_database);
        self.search_table = other.search_table.or(self.search_table);
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

// ---------------------------------------------------------------------------
// The result
// ---------------------------------------------------------------------------

/// Everything the SQLite store needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteSettings {
    /// The database file to open.
    pub database: PathBuf,
}

/// Everything the FTS5 index needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteFts5Settings {
    /// The database file holding the index.
    pub database: PathBuf,
    /// The FTS5 table to query.
    pub table: String,
}

/// The selected store, with its settings and nothing else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageConfig {
    /// Open this SQLite file.
    Sqlite(SqliteSettings),
}

/// The selected index, with its settings and nothing else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchConfig {
    /// Query this FTS5 table.
    SqliteFts5(SqliteFts5Settings),
}

/// What Wayfind will open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedConfig {
    /// The store.
    pub storage: StorageConfig,
    /// The index.
    pub search: SearchConfig,
}

impl ResolvedConfig {
    /// The database file the store will open.
    pub fn storage_database(&self) -> &PathBuf {
        match &self.storage {
            StorageConfig::Sqlite(settings) => &settings.database,
        }
    }
}

/// Merge the layers and refine what is left into the selected backends.
///
/// The search database defaults to the store's database, because the FTS5 table
/// lives in the same file the Bash script created. Setting it separately is
/// allowed, and pointing it somewhere else is the operator's decision.
pub fn resolve_config(input: ConfigInput) -> Result<ResolvedConfig> {
    let merged = input.merge();

    let storage_name = merged
        .backend
        .as_deref()
        .unwrap_or(DEFAULT_STORAGE_BACKEND)
        .to_string();
    let search_name = merged
        .search_backend
        .as_deref()
        .unwrap_or(DEFAULT_SEARCH_BACKEND)
        .to_string();

    let storage = match storage_name.parse::<StorageSelector>()? {
        StorageSelector::Sqlite => {
            let database = merged.sqlite_database.clone().ok_or_else(|| {
                Error::invalid_value(
                    "sqlite.database",
                    "the sqlite backend needs a database path; set `[sqlite] database` \
                     or pass --sqlite.database",
                )
            })?;
            StorageConfig::Sqlite(SqliteSettings { database })
        }
    };

    let search = match search_name.parse::<SearchSelector>()? {
        SearchSelector::SqliteFts5 => {
            let StorageConfig::Sqlite(store) = &storage;
            let database = merged
                .search_database
                .unwrap_or_else(|| store.database.clone());
            let table = merged
                .search_table
                .unwrap_or_else(|| DEFAULT_SEARCH_TABLE.to_string());
            if table.trim().is_empty() {
                return Err(Error::invalid_value(
                    "sqlite-fts5.table",
                    "an FTS5 table name cannot be empty",
                ));
            }
            SearchConfig::SqliteFts5(SqliteFts5Settings { database, table })
        }
    };

    Ok(ResolvedConfig { storage, search })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::path::PathBuf;

    use super::{
        resolve_config, ConfigInput, ConfigSource, RawConfig, SearchConfig, SearchSelector,
        StorageConfig, StorageSelector, DEFAULT_SEARCH_TABLE,
    };
    use crate::error::Error;

    fn path(text: &str) -> PathBuf {
        PathBuf::from(text)
    }

    fn with_database(text: &str) -> ConfigSource {
        ConfigSource {
            sqlite_database: Some(path(text)),
            ..ConfigSource::default()
        }
    }

    fn resolve(input: ConfigInput) -> super::ResolvedConfig {
        resolve_config(input).expect("a resolvable configuration")
    }

    // -- precedence --------------------------------------------------------

    #[test]
    fn a_later_layer_wins_over_an_earlier_one() {
        let resolved = resolve(ConfigInput {
            defaults: with_database("/defaults.sqlite"),
            file: with_database("/file.sqlite"),
            environment: with_database("/environment.sqlite"),
            cli: with_database("/cli.sqlite"),
        });
        assert_eq!(resolved.storage_database(), &path("/cli.sqlite"));
    }

    #[test]
    fn a_layer_that_says_nothing_leaves_the_layer_below_it_alone() {
        let resolved = resolve(ConfigInput {
            defaults: with_database("/defaults.sqlite"),
            file: ConfigSource::default(),
            environment: ConfigSource::default(),
            cli: ConfigSource::default(),
        });
        assert_eq!(resolved.storage_database(), &path("/defaults.sqlite"));
    }

    #[test]
    fn each_setting_is_decided_on_its_own() {
        let resolved = resolve(ConfigInput {
            defaults: with_database("/defaults.sqlite"),
            file: ConfigSource {
                search_table: Some("other_search".to_string()),
                ..ConfigSource::default()
            },
            environment: ConfigSource::default(),
            cli: ConfigSource::default(),
        });
        assert_eq!(resolved.storage_database(), &path("/defaults.sqlite"));
        let SearchConfig::SqliteFts5(search) = &resolved.search;
        assert_eq!(search.table, "other_search");
    }

    // -- selectors ---------------------------------------------------------

    #[test]
    fn nothing_selected_means_the_backends_this_build_was_written_for() {
        let resolved = resolve(ConfigInput {
            defaults: with_database("/wayfind.sqlite"),
            ..ConfigInput::default()
        });
        assert_eq!(
            resolved.storage,
            StorageConfig::Sqlite(super::SqliteSettings {
                database: path("/wayfind.sqlite")
            })
        );
        let SearchConfig::SqliteFts5(search) = &resolved.search;
        assert_eq!(search.table, DEFAULT_SEARCH_TABLE);
    }

    #[test]
    fn a_reserved_backend_is_refused_as_reserved_rather_than_as_a_typing_mistake() {
        let refusal = resolve_config(ConfigInput {
            defaults: with_database("/wayfind.sqlite"),
            cli: ConfigSource {
                backend: Some("dynamodb".to_string()),
                ..ConfigSource::default()
            },
            ..ConfigInput::default()
        })
        .expect_err("dynamodb cannot be opened");
        assert_eq!(
            refusal,
            Error::invalid_value(
                "backend",
                "`dynamodb` is reserved for a later build and cannot be opened yet"
            )
        );
    }

    #[test]
    fn an_unknown_backend_says_what_this_build_offers() {
        let refusal = resolve_config(ConfigInput {
            defaults: with_database("/wayfind.sqlite"),
            cli: ConfigSource {
                backend: Some("postgres".to_string()),
                ..ConfigSource::default()
            },
            ..ConfigInput::default()
        })
        .expect_err("postgres is not a backend");
        assert!(refusal.to_string().contains("this build offers sqlite"));
    }

    #[test]
    fn a_selector_reads_back_as_it_was_written() {
        assert_eq!(
            "sqlite".parse::<StorageSelector>().unwrap().as_str(),
            "sqlite"
        );
        assert_eq!(
            "sqlite-fts5".parse::<SearchSelector>().unwrap().as_str(),
            "sqlite-fts5"
        );
        assert!("sqlite".parse::<SearchSelector>().is_err());
    }

    // -- required fields ---------------------------------------------------

    #[test]
    fn the_selected_backend_must_have_the_settings_it_needs() {
        let refusal = resolve_config(ConfigInput::default()).expect_err("no database path");
        assert!(refusal.to_string().contains("needs a database path"));
    }

    #[test]
    fn an_empty_table_name_is_refused() {
        let refusal = resolve_config(ConfigInput {
            defaults: with_database("/wayfind.sqlite"),
            cli: ConfigSource {
                search_table: Some("  ".to_string()),
                ..ConfigSource::default()
            },
            ..ConfigInput::default()
        })
        .expect_err("an empty table name");
        assert!(refusal.to_string().contains("cannot be empty"));
    }

    #[test]
    fn the_index_reads_the_store_s_file_unless_it_is_pointed_elsewhere() {
        let resolved = resolve(ConfigInput {
            defaults: with_database("/wayfind.sqlite"),
            ..ConfigInput::default()
        });
        let SearchConfig::SqliteFts5(search) = &resolved.search;
        assert_eq!(search.database, path("/wayfind.sqlite"));

        let split = resolve(ConfigInput {
            defaults: with_database("/wayfind.sqlite"),
            cli: ConfigSource {
                search_database: Some(path("/index.sqlite")),
                ..ConfigSource::default()
            },
            ..ConfigInput::default()
        });
        let SearchConfig::SqliteFts5(search) = &split.search;
        assert_eq!(search.database, path("/index.sqlite"));
    }

    // -- the file ----------------------------------------------------------

    #[test]
    fn a_file_reads_into_one_layer() {
        let raw = RawConfig::from_toml(
            r#"
            backend = "sqlite"
            search_backend = "sqlite-fts5"

            [sqlite]
            database = "/data/wayfind.sqlite"

            [sqlite-fts5]
            table = "ticket_search"
            "#,
        )
        .expect("a valid file");
        let source = raw.into_source();
        assert_eq!(source.backend.as_deref(), Some("sqlite"));
        assert_eq!(source.sqlite_database, Some(path("/data/wayfind.sqlite")));
        assert_eq!(source.search_table.as_deref(), Some("ticket_search"));
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
        assert!(refusal.to_string().contains("databse"));

        let unknown_section =
            RawConfig::from_toml("[postgres]\nurl = \"...\"\n").expect_err("an unknown section");
        assert!(unknown_section.to_string().contains("postgres"));
    }

    #[test]
    fn a_reserved_section_parses_and_stays_inactive() {
        let raw = RawConfig::from_toml(
            r#"
            [sqlite]
            database = "/data/wayfind.sqlite"

            [dynamodb]
            table = "wayfind"
            region = "eu-west-1"
            "#,
        )
        .expect("a file holding a reserved section");
        assert!(raw.dynamodb.is_some());

        // Parsed, and it changes nothing about what gets opened.
        let resolved = resolve(ConfigInput {
            file: raw.into_source(),
            ..ConfigInput::default()
        });
        assert_eq!(resolved.storage_database(), &path("/data/wayfind.sqlite"));
    }

    #[test]
    fn a_setting_for_a_backend_that_was_not_selected_is_ignored() {
        // The store is SQLite, so the FTS5 table name is read; nothing else in
        // the file has any say over what is opened.
        let raw = RawConfig::from_toml(
            r#"
            [sqlite]
            database = "/data/wayfind.sqlite"

            [sqlite-fts5]
            database = "/data/index.sqlite"
            table = "ticket_search"
            "#,
        )
        .expect("a valid file");
        let resolved = resolve(ConfigInput {
            file: raw.into_source(),
            ..ConfigInput::default()
        });
        assert_eq!(resolved.storage_database(), &path("/data/wayfind.sqlite"));
    }

    #[test]
    fn malformed_text_is_refused_with_the_reason() {
        let refusal = RawConfig::from_toml("backend = ").expect_err("malformed text");
        assert!(matches!(refusal, Error::InvalidValue { field, .. } if field == RawConfig::FIELD));
    }
}
