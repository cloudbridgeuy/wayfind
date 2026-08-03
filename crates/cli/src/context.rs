//! The boundary between the shell and the machine it runs on.
//!
//! Everything the program cannot compute for itself comes through
//! [`Environment`]: the working directory, the repository around it, the
//! environment variables, the clock, the filesystem, and standard input. One
//! trait holds all of it so that a test can hand a command a whole world
//! without a temporary directory, and so that no command handler ever reaches
//! for `std::env` on its own.
//!
//! Two derived facts also live here, because both are read from the same
//! world. A **project key** is the physical path of the tree the operator is
//! standing in; a **session identifier** is whoever is doing the standing. The
//! rules for both are the script's, unchanged.

use std::path::{Path, PathBuf};

use chrono::Utc;
use wayfind_core::{ProjectKey, SessionId, Timestamp};

use crate::error::{ShellError, ShellResult};

/// The environment variables that can name a session, in the order they are
/// consulted.
///
/// `WAYFIND_SESSION_ID` is deliberate; `CLAUDE_SESSION_ID` is what an agent
/// already has set. A flag beats both.
pub const SESSION_VARIABLES: [&str; 2] = ["WAYFIND_SESSION_ID", "CLAUDE_SESSION_ID"];

/// Everything outside the program that a command may need.
///
/// Every method is fallible in the way its effect is fallible and in no other
/// way; none of them decides anything.
pub trait Environment {
    /// Where the process was started.
    fn working_directory(&self) -> ShellResult<PathBuf>;

    /// The path with every symbolic link resolved, as `pwd -P` reports it.
    fn physical_path(&self, path: &Path) -> ShellResult<PathBuf>;

    /// The root of the version-controlled tree containing `from`, if there is
    /// one.
    ///
    /// Absence is not a failure: plenty of work happens outside a repository,
    /// and the caller falls back to the directory itself.
    fn repository_root(&self, from: &Path) -> Option<PathBuf>;

    /// The value of an environment variable, treating empty as absent.
    fn variable(&self, name: &str) -> Option<String>;

    /// The current instant.
    fn now(&self) -> Timestamp;

    /// The whole content of a file.
    fn read_file(&self, path: &Path) -> ShellResult<Vec<u8>>;

    /// The whole of standard input.
    fn read_input(&self) -> ShellResult<Vec<u8>>;

    /// Delete a file.
    fn remove_file(&self, path: &Path) -> ShellResult<()>;
}

/// The real machine.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemEnvironment;

impl SystemEnvironment {
    /// The environment as the operating system presents it.
    pub fn new() -> Self {
        Self
    }
}

impl Environment for SystemEnvironment {
    fn working_directory(&self) -> ShellResult<PathBuf> {
        std::env::current_dir()
            .map_err(|error| ShellError::stream("read the working directory", error))
    }

    fn physical_path(&self, path: &Path) -> ShellResult<PathBuf> {
        std::fs::canonicalize(path).map_err(|error| ShellError::file("resolve", path, error))
    }

    fn repository_root(&self, from: &Path) -> Option<PathBuf> {
        // `git` is asked rather than walked to, because it knows about work
        // trees, submodules, and `$GIT_DIR` and this program does not. A
        // machine without `git`, or a directory outside a repository, simply
        // gets `None`.
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(from)
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let root = String::from_utf8(output.stdout).ok()?;
        let root = root.trim_end_matches('\n');
        if root.is_empty() {
            return None;
        }
        Some(PathBuf::from(root))
    }

    fn variable(&self, name: &str) -> Option<String> {
        // An exported-but-empty variable is how a shell says "unset" in
        // practice, and the script treated it that way.
        std::env::var(name).ok().filter(|value| !value.is_empty())
    }

    fn now(&self) -> Timestamp {
        Timestamp::from_utc(Utc::now())
    }

    fn read_file(&self, path: &Path) -> ShellResult<Vec<u8>> {
        std::fs::read(path).map_err(|error| ShellError::file("read", path, error))
    }

    fn read_input(&self) -> ShellResult<Vec<u8>> {
        use std::io::Read;

        let mut bytes = Vec::new();
        std::io::stdin()
            .read_to_end(&mut bytes)
            .map_err(|error| ShellError::stream("read standard input", error))?;
        Ok(bytes)
    }

    fn remove_file(&self, path: &Path) -> ShellResult<()> {
        std::fs::remove_file(path).map_err(|error| ShellError::file("delete", path, error))
    }
}

/// Which tree the operator is working in.
///
/// An explicit `--project` wins. Otherwise the repository root wins over the
/// working directory, so that the same key is produced from anywhere inside a
/// checkout. Whichever path is chosen is resolved physically, because a key is
/// compared as text and two spellings of one directory must not become two
/// projects.
pub fn project_key(
    environment: &dyn Environment,
    chosen: Option<&Path>,
) -> ShellResult<ProjectKey> {
    let start = match chosen {
        Some(path) => environment.physical_path(path)?,
        None => {
            let here = environment.working_directory()?;
            match environment.repository_root(&here) {
                Some(root) => environment.physical_path(&root)?,
                None => environment.physical_path(&here)?,
            }
        }
    };
    Ok(ProjectKey::new(start.to_string_lossy().into_owned())?)
}

/// Who is asking.
///
/// A session is never invented. Commands that hold, settle, or attach need one,
/// and if nothing names it the command is refused rather than run under a
/// guessed identity.
pub fn session_id(environment: &dyn Environment, chosen: Option<&str>) -> ShellResult<SessionId> {
    if let Some(value) = chosen {
        return Ok(SessionId::new(value)?);
    }
    for name in SESSION_VARIABLES {
        if let Some(value) = environment.variable(name) {
            return Ok(SessionId::new(value)?);
        }
    }
    Err(ShellError::refused(
        "a session is required; pass --session ID or set WAYFIND_SESSION_ID",
    ))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    use wayfind_core::Timestamp;

    use super::{project_key, session_id, Environment};
    use crate::error::{ShellError, ShellResult};

    /// A world with nothing in it but what a test puts there.
    struct FakeEnvironment {
        working_directory: PathBuf,
        repository_root: Option<PathBuf>,
        variables: BTreeMap<String, String>,
    }

    impl FakeEnvironment {
        fn at(directory: &str) -> Self {
            Self {
                working_directory: PathBuf::from(directory),
                repository_root: None,
                variables: BTreeMap::new(),
            }
        }

        fn in_repository(mut self, root: &str) -> Self {
            self.repository_root = Some(PathBuf::from(root));
            self
        }

        fn with(mut self, name: &str, value: &str) -> Self {
            self.variables.insert(name.to_string(), value.to_string());
            self
        }
    }

    impl Environment for FakeEnvironment {
        fn working_directory(&self) -> ShellResult<PathBuf> {
            Ok(self.working_directory.clone())
        }

        fn physical_path(&self, path: &Path) -> ShellResult<PathBuf> {
            Ok(path.to_path_buf())
        }

        fn repository_root(&self, _from: &Path) -> Option<PathBuf> {
            self.repository_root.clone()
        }

        fn variable(&self, name: &str) -> Option<String> {
            self.variables.get(name).cloned()
        }

        fn now(&self) -> Timestamp {
            "2026-01-01 00:00:00".parse().unwrap()
        }

        fn read_file(&self, _path: &Path) -> ShellResult<Vec<u8>> {
            Err(ShellError::refused("no files here"))
        }

        fn read_input(&self) -> ShellResult<Vec<u8>> {
            Err(ShellError::refused("no input here"))
        }

        fn remove_file(&self, _path: &Path) -> ShellResult<()> {
            Err(ShellError::refused("no files here"))
        }
    }

    #[test]
    fn a_checkout_keys_on_its_root_from_any_depth() {
        let environment = FakeEnvironment::at("/work/repo/crates/cli").in_repository("/work/repo");
        assert_eq!(
            project_key(&environment, None).unwrap().as_str(),
            "/work/repo"
        );
    }

    #[test]
    fn a_directory_outside_a_repository_keys_on_itself() {
        let environment = FakeEnvironment::at("/work/loose");
        assert_eq!(
            project_key(&environment, None).unwrap().as_str(),
            "/work/loose"
        );
    }

    #[test]
    fn an_explicit_project_beats_the_repository_around_it() {
        let environment = FakeEnvironment::at("/work/repo").in_repository("/work/repo");
        let chosen = PathBuf::from("/elsewhere");
        assert_eq!(
            project_key(&environment, Some(&chosen)).unwrap().as_str(),
            "/elsewhere"
        );
    }

    #[test]
    fn the_flag_beats_both_variables_and_wayfind_beats_claude() {
        let environment = FakeEnvironment::at("/work")
            .with("WAYFIND_SESSION_ID", "wayfind")
            .with("CLAUDE_SESSION_ID", "claude");
        assert_eq!(
            session_id(&environment, Some("flag")).unwrap().as_str(),
            "flag"
        );
        assert_eq!(session_id(&environment, None).unwrap().as_str(), "wayfind");

        let only_claude = FakeEnvironment::at("/work").with("CLAUDE_SESSION_ID", "claude");
        assert_eq!(session_id(&only_claude, None).unwrap().as_str(), "claude");
    }

    #[test]
    fn an_unnamed_session_is_refused_rather_than_invented() {
        let environment = FakeEnvironment::at("/work");
        let error = session_id(&environment, None).unwrap_err();
        assert!(error.to_string().contains("--session"));
    }
}
