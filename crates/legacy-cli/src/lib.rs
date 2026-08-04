//! Wayfind's imperative shell, as a library.
//!
//! The binary in `main.rs` is a thin wrapper around this crate. Everything is
//! here instead of there so the shell's own parts — argument shapes,
//! configuration collection, the SQLite adapters — can be exercised by a test
//! that links against them rather than only by running the binary.
//!
//! The split with `wayfind_v1_core` is the one that matters. That crate holds
//! every rule and touches nothing. This crate holds every effect and decides
//! nothing.
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod app;
pub mod args;
pub mod commands;
pub mod config;
pub mod context;
pub mod error;
pub mod input;
pub mod output;
pub mod sqlite;
