//! `wayfind search`: what the index found.

use std::fmt::Write as _;

use super::front_matter::FrontMatter;
use crate::format::flatten_lines;
use crate::search::SearchHit;

/// Everything `wayfind search` prints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchView {
    /// What was asked for.
    pub query: String,
    /// How many hits were asked for.
    pub limit: u32,
    /// How many hits were skipped.
    pub offset: u32,
    /// The hits, most relevant first.
    pub hits: Vec<SearchHit>,
}

/// Render search results.
pub fn render_search(model: &SearchView) -> String {
    let mut out = FrontMatter::new("search")
        .text("query", &model.query)
        .number("limit", i64::from(model.limit))
        .number("offset", i64::from(model.offset))
        .render();
    out.push_str("\n# Search results\n\n");
    for hit in &model.hits {
        let _ = writeln!(
            out,
            "- [{}] {} ({}) — {}",
            hit.ticket_id,
            flatten_lines(&hit.title),
            hit.status,
            flatten_lines(&hit.snippet)
        );
    }
    out
}
