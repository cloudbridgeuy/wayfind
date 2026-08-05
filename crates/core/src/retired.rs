//! The v1 command spellings v2 renamed, moved, or folded elsewhere.
//!
//! Every entry here answers `retired-command` and nothing else: the fate
//! itself — where the command moved to — was decided once, in the S1 CLI
//! specification's Detail A9 fate table, and this module is that decision
//! made checkable. A v1 path that is not here either survives unchanged
//! (`init`, `search`, `dump`, bare `ticket ID`) or collides with a real v2
//! command of the same name, which wins outright (S1 D2) and so is never
//! retired — `ticket create` is v2's own command, not a renamed v1 one.

use crate::error::{ErrorToken, Rejection};
use crate::render::Field;

/// Every retired v1 command path, and the v2 spelling that replaced it.
///
/// The left side is the v1 invocation, space-separated; the right side is
/// what an operator should type instead.
pub const REPLACEMENTS: &[(&str, &str)] = &[
    ("initiative clear", "run clear"),
    ("map", "run map"),
    ("tree", "run tree"),
    ("next", "run next"),
    ("handoff", "run handoff"),
    ("ticket claim", "run question claim"),
    ("ticket resolve", "run question resolve"),
    ("ticket amend", "run question amend"),
    ("ticket block", "run question block"),
    ("session resume", "sessions resume"),
    ("session list", "sessions list"),
    ("fog add", "run fog add"),
    ("scope exclude", "run scope exclude"),
    ("attach add", "run attach add"),
    ("attach ref", "run attach ref"),
    ("attach unref", "run attach unref"),
    ("attach list", "run attach list"),
    ("attach show", "run attach show"),
    ("attach rm", "run attach rm"),
];

/// The longest [`REPLACEMENTS`] entry whose words prefix `path`, if any.
fn longest_match(path: &[&str]) -> Option<(&'static str, &'static str)> {
    REPLACEMENTS
        .iter()
        .filter(|(v1, _)| {
            let words: Vec<&str> = v1.split(' ').collect();
            path.len() >= words.len() && path[..words.len()] == words[..]
        })
        .max_by_key(|(v1, _)| v1.split(' ').count())
        .copied()
}

/// Refuse a retired v1 command path, naming its exact v2 replacement.
///
/// `path` is the invoked words, including whatever the operator typed after
/// the retired spelling itself: `["ticket", "claim", "4"]` answers with
/// `run question claim 4`, not just the bare `run question claim` shape, so
/// the message names the exact command to run instead of leaving the
/// operator to guess where the trailing arguments go.
pub fn retired(path: &[&str]) -> Option<Rejection> {
    let (v1, v2) = longest_match(path)?;
    let matched_words = v1.split(' ').count();
    let trailing = &path[matched_words..];
    let replacement = if trailing.is_empty() {
        v2.to_string()
    } else {
        format!("{v2} {}", trailing.join(" "))
    };
    Some(
        Rejection::new(ErrorToken::RetiredCommand)
            .key("replacement", Field::Text(replacement.clone()))
            .body(format!(
                "`wayfind2 {v1}` is retired. Run `wayfind2 {replacement}` instead."
            )),
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{retired, REPLACEMENTS};
    use crate::error::ErrorToken;
    use crate::render::Field;

    #[test]
    fn the_table_covers_exactly_the_detail_a9_retired_rows() {
        // The literal list transcribed from S1 Detail A9, sorted.
        const EXPECTED: &[&str] = &[
            "attach add",
            "attach list",
            "attach ref",
            "attach rm",
            "attach show",
            "attach unref",
            "fog add",
            "handoff",
            "initiative clear",
            "map",
            "next",
            "scope exclude",
            "session list",
            "session resume",
            "ticket amend",
            "ticket block",
            "ticket claim",
            "ticket resolve",
            "tree",
        ];
        let mut actual: Vec<&str> = REPLACEMENTS.iter().map(|(v1, _)| *v1).collect();
        actual.sort_unstable();
        assert_eq!(actual, EXPECTED);
    }

    #[test]
    fn no_replacement_names_a_retired_spelling() {
        for (_, v2) in REPLACEMENTS {
            assert!(
                retired(&v2.split(' ').collect::<Vec<_>>()).is_none(),
                "{v2} is itself retired — the operator would loop"
            );
        }
    }

    #[test]
    fn the_three_survivors_are_absent() {
        for survivor in ["init", "search", "dump"] {
            assert!(retired(&[survivor]).is_none());
        }
    }

    #[test]
    fn a_v2_collision_is_absent_too() {
        // v1 `ticket create` and v2 `ticket create` share a spelling; v2
        // wins the collision (S1 D2), so it is not in the table.
        assert!(retired(&["ticket", "create"]).is_none());
    }

    #[test]
    fn trailing_words_are_echoed_onto_the_replacement() {
        let rejection = retired(&["ticket", "claim", "4"]).unwrap();
        assert_eq!(rejection.token(), ErrorToken::RetiredCommand);
        assert_eq!(
            rejection.keys(),
            &[(
                "replacement",
                Field::Text("run question claim 4".to_string())
            )]
        );
    }

    #[test]
    fn no_trailing_words_leaves_the_replacement_exactly_as_written() {
        let rejection = retired(&["map"]).unwrap();
        assert_eq!(
            rejection.keys(),
            &[("replacement", Field::Text("run map".to_string()))]
        );
    }
}
