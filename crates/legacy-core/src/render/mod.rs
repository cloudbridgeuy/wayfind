//! Turning a read view into the document an operator or an agent reads.
//!
//! Every command that prints a document builds a view model here and hands it
//! to a render function. The models hold only what the document shows, so a
//! rendering test states a situation instead of standing up a database, and a
//! change of wording cannot quietly change what was read.
//!
//! Most documents are TOML front matter followed by Markdown. The front matter
//! is what a program reads: a `kind` key first, then the few facts worth acting
//! on. The Markdown below it is what a person reads. Both come out of one pass,
//! so the two halves cannot disagree.
//!
//! Four outputs are deliberately not front matter, because their readers are
//! not front-matter readers: `init` and `initiative clear` report one line to a
//! human, `tree` draws a diagram, and `dump --csv` writes records for a
//! spreadsheet.

mod attachment;
mod dump;
mod front_matter;
mod guidance;
mod handoff;
mod map;
mod markdown;
mod rows;
mod search;
mod session;
mod ticket;

#[cfg(test)]
mod tests;

pub use attachment::{
    render_attachment_header, render_attachment_list, AttachmentListView, AttachmentView,
};
pub use dump::{render_csv, render_init, render_initiative_cleared, DumpRow, DUMP_HEADER};
pub use front_matter::{Field, FrontMatter};
pub use guidance::state_guidance;
pub use handoff::{render_handoff, HandoffView};
pub use map::{render_map, MapView};
pub use rows::{
    AttachmentRow, DecisionRow, FrontierRow, FullDecision, InitiativeHeader, OwnedAttachmentRow,
    ReferencedAttachmentRow, SessionRow, UnresolvedRow,
};
pub use search::{render_search, SearchView};
pub use session::{
    render_next_unavailable, render_session_list, render_session_resume, NextView, SessionListView,
    SessionResumeView,
};
pub use ticket::{render_ticket, TicketView};
