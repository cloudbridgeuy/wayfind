//! The `+++` block at the top of a document.

use std::fmt::Write as _;

use crate::render::format::toml_string;

/// One front-matter value.
///
/// Only three shapes ever appear, and each renders one way, so no caller has to
/// decide how a number or a title should be quoted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    /// A count or an integer identifier.
    Number(i64),
    /// Text, quoted and escaped as a TOML basic string.
    Text(String),
    /// A list of record identifiers, such as `outputs`.
    ///
    /// v2 carries these as text rather than as integers: a record ID is
    /// `R-<64 hex>`, which is not a number.
    Ids(Vec<String>),
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

    /// Add a list of record identifiers.
    pub fn ids(mut self, key: &'static str, value: Vec<String>) -> Self {
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

    /// Add an already-built key. The error document builds its keys elsewhere.
    pub fn field(mut self, key: &'static str, value: Field) -> Self {
        self.entries.push((key, value));
        self
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
                        .map(|id| toml_string(id))
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{Field, FrontMatter};

    #[test]
    fn kind_is_always_the_first_key_and_the_block_is_fenced() {
        let rendered = FrontMatter::new("initiative")
            .number("initiative", 7)
            .render();
        assert_eq!(
            rendered,
            "+++\nkind = \"initiative\"\ninitiative = 7\n+++\n"
        );
    }

    #[test]
    fn keys_keep_the_order_they_were_added_in() {
        let rendered = FrontMatter::new("node")
            .text("node", "R-abc")
            .number("count", 2)
            .text("title", "A title")
            .render();
        let keys: Vec<&str> = rendered
            .lines()
            .filter(|line| *line != "+++")
            .filter_map(|line| line.split(" = ").next())
            .collect();
        assert_eq!(keys, vec!["kind", "node", "count", "title"]);
    }

    #[test]
    fn an_absent_optional_value_omits_its_key_rather_than_emptying_it() {
        let present = FrontMatter::new("node")
            .optional_text("summary", Some("a gist"))
            .render();
        assert!(present.contains("summary = \"a gist\"\n"));

        let absent = FrontMatter::new("node")
            .optional_text("summary", None::<String>)
            .render();
        assert!(!absent.contains("summary"));
    }

    #[test]
    fn record_ids_render_as_a_list_of_quoted_strings() {
        let rendered = FrontMatter::new("accepted")
            .ids(
                "outputs",
                vec!["R-a3f9c2e1".to_string(), "R-b70d1145".to_string()],
            )
            .render();
        assert!(rendered.contains("outputs = [\"R-a3f9c2e1\",\"R-b70d1145\"]\n"));

        let empty = FrontMatter::new("accepted")
            .ids("outputs", Vec::new())
            .render();
        assert!(empty.contains("outputs = []\n"));
    }

    #[test]
    fn every_rendered_block_parses_back_as_toml() {
        let rendered = FrontMatter::new("error")
            .text("error", "not-found")
            .text("id", "a \"quoted\"\ttitle")
            .number("initiative", 1)
            .ids("candidates", vec!["R-aaaa".to_string()])
            .render();
        let body = rendered
            .trim_start_matches("+++\n")
            .trim_end_matches("+++\n");
        let parsed: toml::Table = toml::from_str(body).expect("valid TOML");
        assert_eq!(parsed["kind"].as_str(), Some("error"));
        assert_eq!(parsed["id"].as_str(), Some("a \"quoted\"\ttitle"));
        assert_eq!(parsed["initiative"].as_integer(), Some(1));
    }

    #[test]
    fn an_already_built_field_renders_the_same_as_a_named_one() {
        let built = FrontMatter::new("error")
            .field("initiative", Field::Number(7))
            .render();
        let named = FrontMatter::new("error").number("initiative", 7).render();
        assert_eq!(built, named);
    }
}
