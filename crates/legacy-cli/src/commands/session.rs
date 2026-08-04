//! What a session is doing.
//!
//! A session is one agent working one initiative. It is created the first time
//! it says anything, it never moves to another project or another initiative,
//! and it may hold at most one ticket at a time. These two commands are how it
//! announces itself and how everyone else sees who else is on the map.

use wayfind_v1_core::{
    classify_initiative, render_session_list, render_session_resume, SessionListView,
    SessionResumeView,
};

use super::Shell;
use crate::error::ShellResult;
use crate::output::Output;

/// Say that this session is back, and show it what it was doing.
///
/// A session holding a ticket gets that ticket. One holding nothing gets the
/// state of the initiative and the guidance that goes with it.
pub fn resume(shell: &Shell<'_>, output: &mut dyn Output) -> ShellResult<()> {
    let session_id = shell.session()?;
    let initiative_id = shell.readable_initiative()?;
    let session = shell.touch_session(initiative_id)?;

    let view = shell.view(initiative_id)?;
    if let Some(ticket_id) = session.state.held_ticket() {
        let document = shell.ticket_document(&view, ticket_id)?;
        return output.text(&document);
    }
    output.text(&render_session_resume(&SessionResumeView {
        session_id,
        initiative_id,
        state: classify_initiative(&view),
    }))
}

/// Show every session working this initiative.
pub fn list(shell: &Shell<'_>, output: &mut dyn Output) -> ShellResult<()> {
    let view = shell.readable_view()?;
    output.text(&render_session_list(&SessionListView {
        initiative_id: view.id(),
        sessions: shell.session_rows(&view),
    }))
}
