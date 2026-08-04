//! The storage boundary.
//!
//! Storage is small capabilities rather than one large interface, following
//! v1's pattern: a backend that cannot do one thing cheaply fails to
//! implement one trait instead of doing it slowly inside a method nobody
//! expected to be expensive.

pub mod coordination;
pub mod graph;
pub mod values;
