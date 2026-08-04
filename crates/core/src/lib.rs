//! Wayfind v2's functional core.
//!
//! Every rule the system has lives here, and nothing here reaches outside the
//! process. There is no clock, no filesystem, no environment, and no database:
//! the shell reads all of that and hands the values in. What comes back is a
//! decision or a rendered document.
//!
//! The split with `wayfind_cli` is the one that matters. That crate holds every
//! effect and decides nothing. This crate holds every rule and touches nothing.
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod config;
pub mod encode;
pub mod error;
pub mod id;
pub mod kinds;
pub mod record;
pub mod render;
pub mod storage;
pub mod time;
