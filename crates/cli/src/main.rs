//! Wayfind's imperative shell.
//!
//! This binary owns every effect: argument parsing, layered configuration,
//! filesystem and standard-input access, the clock, the SQLite adapters, and
//! writing output. It holds no business rule; it asks `wayfind_core` for every
//! decision and applies the result.
#![deny(clippy::unwrap_used, clippy::expect_used)]

fn main() {
    println!("wayfind");
}
