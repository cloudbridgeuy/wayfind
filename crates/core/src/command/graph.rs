//! Commands that mutate the immutable graph.

use crate::id::{ProjectKey, SessionId};
use crate::time::Timestamp;

/// Ask the core to validate and prepare a new initiative.
pub struct CreateInitiativeCommand {
    pub project: ProjectKey,
    pub name: String,
    pub destination: String,
    pub notes: Option<String>,
    pub created_by: SessionId,
    pub now: Timestamp,
}
