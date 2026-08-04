//! The retired v1 spellings, kept parseable so they can answer with their
//! exact v2 replacement instead of Clap's own "unrecognized subcommand"
//! refusal.
//!
//! Every command here is hidden: an operator does not see it in `--help`,
//! but a script or muscle memory that still types it gets the exact v2
//! spelling to use instead, not a hard failure to guess from.

use clap::{Args, Subcommand};

/// Trailing words a retired command was invoked with.
///
/// Never inspected as arguments — they exist so the parse succeeds
/// regardless of what v1 shape followed the retired spelling, and so the
/// exact invocation can be echoed back onto the replacement message.
#[derive(Debug, Clone, Args)]
pub struct Rest {
    /// Whatever followed the retired spelling.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub rest: Vec<String>,
}

/// Retired `session` (singular) commands. v1's `session` and `sessions`
/// collapsed into v2's `sessions` alone (S1 D1).
#[derive(Debug, Clone, Subcommand)]
pub enum RetiredSessionCommand {
    /// Retired: errors naming `sessions resume`.
    #[command(hide = true)]
    Resume(Rest),

    /// Retired: errors naming `sessions list`.
    #[command(hide = true)]
    List(Rest),
}

/// Retired `fog` commands.
#[derive(Debug, Clone, Subcommand)]
pub enum RetiredFogCommand {
    /// Retired: errors naming `run fog add`.
    #[command(hide = true)]
    Add(Rest),
}

/// Retired `scope` commands.
#[derive(Debug, Clone, Subcommand)]
pub enum RetiredScopeCommand {
    /// Retired: errors naming `run scope exclude`.
    #[command(hide = true)]
    Exclude(Rest),
}

/// Retired `attach` commands.
#[derive(Debug, Clone, Subcommand)]
pub enum RetiredAttachCommand {
    /// Retired: errors naming `run attach add`.
    #[command(hide = true)]
    Add(Rest),

    /// Retired: errors naming `run attach ref`.
    #[command(hide = true)]
    Ref(Rest),

    /// Retired: errors naming `run attach unref`.
    #[command(hide = true)]
    Unref(Rest),

    /// Retired: errors naming `run attach list`.
    #[command(hide = true)]
    List(Rest),

    /// Retired: errors naming `run attach show`.
    #[command(hide = true)]
    Show(Rest),

    /// Retired: errors naming `run attach rm`.
    #[command(hide = true)]
    Rm(Rest),
}
