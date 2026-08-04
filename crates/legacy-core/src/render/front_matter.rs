//! The `+++` block at the top of a document.

use std::fmt::Write as _;

use crate::format::toml_string;
use crate::id::TicketId;

/// One front-matter value.
///
/// Only three shapes ever appear, and each renders one way, so no caller has to
/// decide how a number or a title should be quoted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    /// A count or an identifier.
    Number(i64),
    /// Text, quoted and escaped as a TOML basic string.
    Text(String),
    /// A list of ticket identifiers, such as `blocked_by`.
    Ids(Vec<TicketId>),
}

/// The `+++` block at the top of a document.
///
/// Keys keep insertion order. The order is part of the output, so it is decided
/// once, where the document is built, rather than by a map's iteration order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontMatter {
    entries: Vec<(&'static str, Field)>,
}

impl FrontMatter {
    /// Start a block. `kind` is always the first key, so a reader can dispatch
    /// on it without parsing the rest.
    pub fn new(kind: &'static str) -> Self {
        Self {
            entries: vec![("kind", Field::Text(kind.to_string()))],
        }
    }

    /// Add a numeric key.
    pub fn number(mut self, key: &'static str, value: impl Into<i64>) -> Self {
        self.entries.push((key, Field::Number(value.into())));
        self
    }

    /// Add a text key.
    pub fn text(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.entries.push((key, Field::Text(value.into())));
        self
    }

    /// Add a list of ticket identifiers.
    pub fn ids(mut self, key: &'static str, value: Vec<TicketId>) -> Self {
        self.entries.push((key, Field::Ids(value)));
        self
    }

    /// Add a text key only when there is something to say.
    pub fn optional_text(self, key: &'static str, value: Option<impl Into<String>>) -> Self {
        match value {
            Some(value) => self.text(key, value),
            None => self,
        }
    }

    /// Render the block, `+++` fences included, ending in one newline.
    pub fn render(&self) -> String {
        let mut out = String::from("+++\n");
        for (key, value) in &self.entries {
            match value {
                Field::Number(number) => {
                    let _ = writeln!(out, "{key} = {number}");
                }
                Field::Text(text) => {
                    let _ = writeln!(out, "{key} = {}", toml_string(text));
                }
                Field::Ids(ids) => {
                    let joined = ids
                        .iter()
                        .map(|id| id.get().to_string())
                        .collect::<Vec<_>>()
                        .join(",");
                    let _ = writeln!(out, "{key} = [{joined}]");
                }
            }
        }
        out.push_str("+++\n");
        out
    }
}
