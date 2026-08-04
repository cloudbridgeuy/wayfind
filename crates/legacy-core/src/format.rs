//! The small text operations every rendered view shares.
//!
//! Each one is a total function from text to text. They are separated from
//! [`crate::render`] because they are the pieces most likely to be wrong in a
//! way a whole-document comparison would hide: a clamp that splits a character
//! in half, a size that rounds the wrong way, a quote that produces invalid
//! TOML.

use std::fmt::Write as _;

/// How many characters a gist keeps before it is cut short.
pub const GIST_LIMIT: usize = 200;

/// The mark that shows a gist was cut short.
pub const ELLIPSIS: char = '…';

/// Put a value on one line.
///
/// The row-per-line views — the frontier list, the session table, the tree —
/// break if a value carries a line ending, so every value that reaches them
/// passes through here first. Carriage returns go too, so a file written on
/// Windows does not leave a stray character at the end of a cell.
pub fn flatten_lines(text: &str) -> String {
    text.replace(['\r', '\n'], " ")
}

/// Collapse a value to one short line for an index view.
///
/// Tabs become spaces, runs of whitespace become one space, and the ends are
/// trimmed. What is left is cut to [`GIST_LIMIT`] characters, the last of which
/// is [`ELLIPSIS`], so the reader can see that there is more to read.
///
/// The cut counts characters, not bytes. Clamping an accented or ideographic
/// gist gives 200 characters of text, and never a broken one.
pub fn clamp_gist(text: &str) -> String {
    let mut collapsed = String::with_capacity(text.len());
    let mut pending_space = false;
    for character in text.chars() {
        if character.is_whitespace() {
            pending_space = !collapsed.is_empty();
            continue;
        }
        if pending_space {
            collapsed.push(' ');
            pending_space = false;
        }
        collapsed.push(character);
    }

    if collapsed.chars().count() <= GIST_LIMIT {
        return collapsed;
    }
    let mut clamped: String = collapsed.chars().take(GIST_LIMIT - 1).collect();
    clamped.push(ELLIPSIS);
    clamped
}

/// Report a byte count the way an operator reads one.
///
/// The fraction is truncated rather than rounded, so the number never claims
/// more content than the attachment holds.
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    if bytes < KB {
        return format!("{bytes} B");
    }
    let (unit, suffix) = if bytes < MB { (KB, "KB") } else { (MB, "MB") };
    let whole = bytes / unit;
    // Widened, because ten times a byte count near the top of the range does
    // not fit back into a byte count.
    let tenth = u128::from(bytes) * 10 / u128::from(unit) % 10;
    format!("{whole}.{tenth} {suffix}")
}

/// Drop at most one trailing line ending.
///
/// Storing and printing an attachment each add or remove one newline, and the
/// two cancel out. Dropping only one keeps a document that genuinely ends in a
/// blank line intact.
pub fn strip_one_trailing_newline(bytes: &[u8]) -> &[u8] {
    match bytes.split_last() {
        Some((b'\n', head)) => head,
        _ => bytes,
    }
}

/// Quote a value as a TOML basic string, escapes and all.
///
/// The shell script escaped three characters by hand and let every other
/// control character through, which produced front matter no TOML parser would
/// accept. A ticket titled with a tab was enough to break a reader. This uses a
/// TOML serializer instead, so the front matter is always parseable.
pub fn toml_string(text: &str) -> String {
    let mut quoted = String::with_capacity(text.len() + 2);
    quoted.push('"');
    for character in text.chars() {
        match character {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            '\u{8}' => quoted.push_str("\\b"),
            '\u{c}' => quoted.push_str("\\f"),
            control if control.is_control() => {
                // `write!` to a `String` cannot fail; the result is discarded
                // because there is no error to report.
                let _ = write!(quoted, "\\u{:04X}", control as u32);
            }
            plain => quoted.push(plain),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{
        clamp_gist, flatten_lines, format_size, strip_one_trailing_newline, toml_string, ELLIPSIS,
        GIST_LIMIT,
    };

    #[test]
    fn a_gist_loses_its_line_breaks_tabs_and_repeated_spaces() {
        assert_eq!(
            clamp_gist("  one\ttwo   three\nfour  \r\n"),
            "one two three four"
        );
    }

    #[test]
    fn a_short_gist_is_left_alone() {
        assert_eq!(clamp_gist("already short"), "already short");
        assert_eq!(clamp_gist(""), "");
        assert_eq!(clamp_gist("   \n\t  "), "");
    }

    #[test]
    fn a_gist_of_exactly_the_limit_is_not_cut() {
        let text = "a".repeat(GIST_LIMIT);
        let gist = clamp_gist(&text);
        assert_eq!(gist, text);
        assert!(!gist.ends_with(ELLIPSIS));
    }

    #[test]
    fn a_long_gist_keeps_the_limit_in_characters_and_ends_with_the_mark() {
        let text = "b".repeat(GIST_LIMIT + 1);
        let gist = clamp_gist(&text);
        assert_eq!(gist.chars().count(), GIST_LIMIT);
        assert!(gist.ends_with(ELLIPSIS));
        assert_eq!(gist.matches('b').count(), GIST_LIMIT - 1);
    }

    #[test]
    fn clamping_counts_characters_and_never_splits_one() {
        // Each of these is one character and several bytes. Counting bytes
        // would cut the last one in half and produce invalid text.
        let text = "日".repeat(GIST_LIMIT + 50);
        let gist = clamp_gist(&text);
        assert_eq!(gist.chars().count(), GIST_LIMIT);
        assert_eq!(gist.matches('日').count(), GIST_LIMIT - 1);

        let accented = "é".repeat(GIST_LIMIT + 1);
        assert_eq!(clamp_gist(&accented).chars().count(), GIST_LIMIT);
    }

    #[test]
    fn a_value_on_several_lines_becomes_a_value_on_one() {
        assert_eq!(flatten_lines("one\ntwo\r\nthree"), "one two  three");
        assert_eq!(flatten_lines("untouched"), "untouched");
    }

    #[test]
    fn a_size_under_a_kilobyte_is_reported_in_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1), "1 B");
        assert_eq!(format_size(1023), "1023 B");
    }

    #[test]
    fn a_size_fraction_is_truncated_rather_than_rounded_up() {
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        // 1.9990 KB. Rounding would claim a whole 2 KB that is not there.
        assert_eq!(format_size(2047), "1.9 KB");
        assert_eq!(format_size(1048575), "1023.9 KB");
        assert_eq!(format_size(1048576), "1.0 MB");
        assert_eq!(format_size(1572864), "1.5 MB");
    }

    #[test]
    fn a_size_near_the_top_of_the_range_does_not_overflow() {
        assert_eq!(format_size(u64::MAX), "17592186044415.9 MB");
    }

    #[test]
    fn only_one_trailing_newline_is_dropped() {
        assert_eq!(strip_one_trailing_newline(b"text\n"), b"text");
        assert_eq!(strip_one_trailing_newline(b"text\n\n"), b"text\n");
        assert_eq!(strip_one_trailing_newline(b"text"), b"text");
        assert_eq!(strip_one_trailing_newline(b""), b"");
        assert_eq!(strip_one_trailing_newline(b"\n"), b"");
    }

    #[test]
    fn stripping_a_newline_leaves_a_carriage_return_in_place() {
        // The pair is one line ending on Windows, but the stored form keeps
        // whatever the author wrote, and only the newline cancels out.
        assert_eq!(strip_one_trailing_newline(b"text\r\n"), b"text\r");
    }

    #[test]
    fn a_quoted_value_escapes_what_toml_requires() {
        assert_eq!(toml_string("plain"), "\"plain\"");
        assert_eq!(toml_string("say \"hello\""), "\"say \\\"hello\\\"\"");
        assert_eq!(toml_string("back\\slash"), "\"back\\\\slash\"");
        assert_eq!(toml_string("two\nlines"), "\"two\\nlines\"");
    }

    #[test]
    fn a_control_character_is_escaped_rather_than_written_raw() {
        assert_eq!(toml_string("a\tb"), "\"a\\tb\"");
        assert_eq!(toml_string("a\rb"), "\"a\\rb\"");
        assert_eq!(toml_string("a\u{1}b"), "\"a\\u0001b\"");
        assert_eq!(toml_string("a\u{7f}b"), "\"a\\u007Fb\"");
    }

    #[test]
    fn every_quoted_value_parses_back_to_what_went_in() {
        for text in [
            "plain",
            "say \"hello\"",
            "back\\slash",
            "two\nlines",
            "a\tb\rc\u{1}d",
            "日本語",
        ] {
            let document = format!("value = {}", toml_string(text));
            let parsed: toml::Table = toml::from_str(&document).expect("valid TOML");
            assert_eq!(parsed["value"].as_str(), Some(text));
        }
    }
}
