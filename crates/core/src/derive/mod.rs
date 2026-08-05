//! Pure derivations over the immutable graph.
//!
//! Everything here is computed from records the shell already read — no
//! query, no clock. A value that can be derived is never stored twice.

pub mod membership;

pub use membership::{members_at, GraphState};
