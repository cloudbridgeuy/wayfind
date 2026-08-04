//! The one document a refused command prints.

use crate::error::Rejection;
use crate::render::FrontMatter;

/// Render a rejection.
///
/// `kind` and `error` come first and always, so a caller can decide what
/// happened by reading two lines. The rejection's own keys follow in the order
/// they were added, and the prose, when there is any, follows the fences.
pub fn document(rejection: &Rejection) -> String {
    let mut front = FrontMatter::new("error").text("error", rejection.token().as_token());
    for (key, value) in rejection.keys() {
        front = front.field(key, value.clone());
    }

    let mut out = front.render();
    if let Some(body) = rejection.body_text() {
        out.push('\n');
        out.push_str(body);
        if !body.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::document;
    use crate::error::{ErrorToken, Rejection};
    use crate::render::Field;

    #[test]
    fn error_document_carries_kind_error_and_the_token() {
        let r = Rejection::new(ErrorToken::NotFound).key("initiative", Field::Number(7));
        let out = document(&r);
        assert!(out.starts_with("+++\nkind = \"error\"\n"));
        assert!(out.contains("error = \"not-found\"\n"));
        assert!(out.contains("initiative = 7\n"));
    }

    #[test]
    fn the_keys_keep_the_order_the_rejection_gave_them() {
        let r = Rejection::new(ErrorToken::AmbiguousId)
            .key("id", Field::Text("R-a3".to_string()))
            .key(
                "candidates",
                Field::Ids(vec!["R-a3f9".to_string(), "R-a3b1".to_string()]),
            );
        let out = document(&r);
        let keys: Vec<&str> = out
            .lines()
            .filter(|line| *line != "+++")
            .filter_map(|line| line.split(" = ").next())
            .collect();
        assert_eq!(keys, vec!["kind", "error", "id", "candidates"]);
        assert!(out.contains("candidates = [\"R-a3f9\",\"R-a3b1\"]\n"));
    }

    #[test]
    fn the_body_follows_the_closing_fence() {
        let r = Rejection::new(ErrorToken::NotFound)
            .key("path", Field::Text("/tmp/wayfind2.sqlite".to_string()))
            .body("No store here. Run `wayfind2 init` first.");
        let out = document(&r);
        assert!(out.ends_with("+++\n\nNo store here. Run `wayfind2 init` first.\n"));
    }

    #[test]
    fn a_rejection_with_no_body_ends_at_the_closing_fence() {
        let out = document(&Rejection::new(ErrorToken::Usage));
        assert_eq!(out, "+++\nkind = \"error\"\nerror = \"usage\"\n+++\n");
    }
}
