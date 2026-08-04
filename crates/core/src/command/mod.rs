//! Commands the shell builds to ask the core for a mutation.
//!
//! One struct per operation, carrying already-parsed values plus the two
//! things only the shell knows: `now` and the session identity.

pub mod graph;
