//! Wayfind v2's imperative shell, as a library.
//!
//! The binary in `main.rs` is a thin wrapper around this crate. Everything is
//! here instead of there so the shell's own parts — argument shapes,
//! configuration collection, the SQLite store — can be exercised by a test that
//! links against them rather than only by running the binary.
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod config;
pub mod context;
pub mod error;
pub mod output;
