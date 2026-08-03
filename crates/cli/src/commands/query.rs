//! Asking the initiative a question.
//!
//! Two commands read across a whole map rather than at one point in it:
//! full-text search, and a bulk export of every ticket. Neither writes
//! anything, and both work on a cleared initiative, because looking back at
//! finished work is a normal thing to do.

use wayfind_core::{
    render_csv, render_search, DumpRow, SearchLimit, SearchOffset, SearchQuery, SearchRequest,
    SearchView,
};

use super::Shell;
use crate::error::{ShellError, ShellResult};
use crate::output::Output;

/// Search the tickets of the initiative in play.
pub fn search(
    shell: &Shell<'_>,
    query: &str,
    limit: u32,
    offset: u32,
    output: &mut dyn Output,
) -> ShellResult<()> {
    let initiative_id = shell.readable_initiative()?;
    let request = SearchRequest {
        query: SearchQuery::new(query)?,
        initiative_id,
        limit: SearchLimit::new(limit)?,
        offset: SearchOffset::new(offset),
    };
    let page = shell.search.search(&request)?;
    output.text(&render_search(&SearchView {
        query: query.to_owned(),
        limit,
        offset,
        hits: page.hits,
    }))
}

/// Write every ticket of the initiative as comma-separated records.
pub fn dump(
    shell: &Shell<'_>,
    limit: u32,
    offset: u32,
    output: &mut dyn Output,
) -> ShellResult<()> {
    let view = shell.readable_view()?;
    let skip = usize::try_from(offset)
        .map_err(|_| ShellError::refused("the offset is larger than this machine can count to"))?;
    let take = usize::try_from(limit)
        .map_err(|_| ShellError::refused("the limit is larger than this machine can count to"))?;

    let rows: Vec<DumpRow> = view
        .tickets()
        .iter()
        .skip(skip)
        .take(take)
        .map(|ticket| DumpRow {
            id: ticket.id,
            title: ticket.title.clone(),
            ticket_type: ticket.ticket_type,
            status: ticket.state.label(),
            question: ticket.question.clone(),
            resolution: ticket.state.resolution().unwrap_or_default().to_owned(),
        })
        .collect();

    output.text(&render_csv(&rows)?)
}
