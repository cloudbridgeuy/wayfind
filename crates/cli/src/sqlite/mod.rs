//! The SQLite store.
//!
//! One file holds both domains. "One persistence model per domain" is a rule
//! about table discipline and about which capability trait may touch which
//! table, not about how many files are on disk.

pub mod graph_write;
pub mod schema;
pub mod store;

pub use store::SqliteStore;
