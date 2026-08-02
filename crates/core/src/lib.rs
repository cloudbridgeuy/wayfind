//! Wayfind's functional core.
//!
//! Every business rule lives here as a pure function over strict values. The
//! crate performs no input or output: it never reads a clock, a file, an
//! environment variable, or a database. The shell in `wayfind_cli` supplies
//! every effect as data and applies every decision this crate returns.
#![deny(clippy::unwrap_used, clippy::expect_used)]
