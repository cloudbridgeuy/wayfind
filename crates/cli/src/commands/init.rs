//! `wayfind2 init`.
//!
//! This is the one command whose output is a plain-text line rather than a
//! front-matter document (Detail A4's exception): there is nothing to key on
//! and nothing to render, only a path to name.

use super::Shell;
use crate::{error::ShellResult, output::Output};

/// Report what the composition root already did.
///
/// `initialize` runs unconditionally before this is called, because the
/// schema is `CREATE TABLE IF NOT EXISTS` throughout — running it again is
/// never wrong. What changes with `already_existed` is only which line the
/// operator reads.
pub fn run(shell: &Shell, already_existed: bool, output: &mut dyn Output) -> ShellResult<()> {
    let path = shell.database.display();
    if already_existed {
        output.text(&format!("a store already exists at {path}\n"))
    } else {
        output.text(&format!("created a store at {path}\n"))
    }
}
