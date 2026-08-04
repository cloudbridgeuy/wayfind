//! The small text operations every rendered view shares.
//!
//! Each one is a total function from text to text. They are separated from the
//! documents because they are the pieces most likely to be wrong in a way a
//! whole-document comparison would hide: a quote that produces invalid TOML.

use std::fmt::Write as _;

/// Quote a value as a TOML basic string, escapes and all.
///
/// The shell script escaped three characters by hand and let every other
/// control character through, which produced front matter no TOML parser would
/// accept. A title carrying a tab was enough to break a reader. This escapes
/// every control character instead, so the front matter is always parseable.
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

    use super::toml_string;

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
