//! Reading what an operator hands the program.
//!
//! Three commands take a body of text — `ticket resolve`, `ticket amend`, and
//! `attach add` — and each of them accepts it typed on the command line, read
//! from a file, or piped in. The checks are the same wherever it came from, so
//! they are written once, here.
//!
//! Text typed as an argument is treated differently from text that was stored.
//! An argument has already been through a shell, which delimits it, forbids NUL
//! inside it, and bounds it by the argument limit; there is nothing left for
//! this module to check and nothing to trim. A file or a pipe has been through
//! none of that, so it is measured, scanned, and — because the script stored a
//! document without its terminating line feed — relieved of at most one.

use std::path::Path;

use wayfind_core::{strip_one_trailing_newline, ResolutionText};

use crate::args::TextSource;
use crate::context::Environment;
use crate::error::{ShellError, ShellResult};

/// The largest document the program will read.
///
/// One mebibyte, the script's own cap. It exists so that a mistyped path
/// pointing at a disk image is refused in a sentence instead of being loaded.
pub const MAX_CONTENT_BYTES: usize = 1_048_576;

/// A body of text that has passed every check.
///
/// The bytes are already in the form that is stored: whatever trailing line
/// feed a file carried is gone. The size is the size that was read, which is
/// what the caller compares against after a store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Content {
    bytes: Vec<u8>,
}

impl Content {
    /// The bytes as they will be stored.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// How many bytes that is.
    pub fn size(&self) -> u64 {
        self.bytes.len() as u64
    }

    /// The same bytes as text, or the reason they are not text.
    pub fn into_text(self) -> ShellResult<String> {
        String::from_utf8(self.bytes)
            .map_err(|_| ShellError::refused("content is not valid UTF-8 text"))
    }

    /// Accept text that arrived as a command-line argument.
    fn typed(text: &str) -> Self {
        Self {
            bytes: text.as_bytes().to_vec(),
        }
    }

    /// Accept bytes that arrived from a file or a pipe.
    fn stored(bytes: &[u8]) -> ShellResult<Self> {
        if bytes.is_empty() {
            return Err(ShellError::refused("content is empty"));
        }
        if bytes.len() > MAX_CONTENT_BYTES {
            return Err(ShellError::refused(format!(
                "content is {} bytes, above the {MAX_CONTENT_BYTES} byte cap",
                bytes.len()
            )));
        }
        if bytes.contains(&0) {
            return Err(ShellError::refused(
                "content holds a NUL byte; attachments are text only",
            ));
        }
        Ok(Self {
            bytes: strip_one_trailing_newline(bytes).to_vec(),
        })
    }
}

/// Read whatever the operator pointed at.
pub fn read_content(environment: &dyn Environment, source: &TextSource) -> ShellResult<Content> {
    match source {
        TextSource::Inline(text) => Ok(Content::typed(text)),
        TextSource::File(path) => Content::stored(&environment.read_file(path)?),
        TextSource::Stdin => Content::stored(&environment.read_input()?),
    }
}

/// Read a decision, in whichever of the three ways it was given.
pub fn read_resolution(
    environment: &dyn Environment,
    source: &TextSource,
) -> ShellResult<ResolutionText> {
    let text = read_content(environment, source)?.into_text()?;
    Ok(ResolutionText::new(text)?)
}

/// The name a stored document should be filed under.
///
/// A file lends its own base name when none was given. A pipe has no name to
/// lend, so one must be supplied. The script had a branch here that could never
/// run — it read the base name of `-` — and this is the narrow repair: say so
/// instead of filing a document as `-`.
pub fn attachment_name<'a>(
    source: &'a TextSource,
    chosen: Option<&'a str>,
) -> ShellResult<&'a str> {
    if let Some(name) = chosen {
        return Ok(name);
    }
    match source {
        TextSource::File(path) => base_name(path).ok_or_else(|| {
            ShellError::refused(format!(
                "{} has no file name to use; pass --name",
                path.display()
            ))
        }),
        TextSource::Stdin | TextSource::Inline(_) => Err(ShellError::refused(
            "a document read from standard input has no name; pass --name",
        )),
    }
}

/// The last component of a path, if it has one that could be a file name.
fn base_name(path: &Path) -> Option<&str> {
    path.file_name().and_then(|name| name.to_str())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::path::{Path, PathBuf};

    use wayfind_core::Timestamp;

    use super::{attachment_name, read_content, Content, MAX_CONTENT_BYTES};
    use crate::args::TextSource;
    use crate::context::Environment;
    use crate::error::{ShellError, ShellResult};

    /// A world that holds exactly one document, wherever it is asked for.
    struct OneDocument(Vec<u8>);

    impl Environment for OneDocument {
        fn working_directory(&self) -> ShellResult<PathBuf> {
            Ok(PathBuf::from("/work"))
        }

        fn physical_path(&self, path: &Path) -> ShellResult<PathBuf> {
            Ok(path.to_path_buf())
        }

        fn repository_root(&self, _from: &Path) -> Option<PathBuf> {
            None
        }

        fn variable(&self, _name: &str) -> Option<String> {
            None
        }

        fn now(&self) -> Timestamp {
            "2026-01-01 00:00:00".parse().unwrap()
        }

        fn read_file(&self, _path: &Path) -> ShellResult<Vec<u8>> {
            Ok(self.0.clone())
        }

        fn read_input(&self) -> ShellResult<Vec<u8>> {
            Ok(self.0.clone())
        }

        fn remove_file(&self, _path: &Path) -> ShellResult<()> {
            Ok(())
        }
    }

    fn from_file(bytes: &[u8]) -> ShellResult<Content> {
        let environment = OneDocument(bytes.to_vec());
        read_content(
            &environment,
            &TextSource::File(PathBuf::from("/work/note.md")),
        )
    }

    #[test]
    fn exactly_one_trailing_line_feed_is_removed() {
        assert_eq!(from_file(b"one\n").unwrap().bytes(), b"one");
        assert_eq!(from_file(b"one\n\n").unwrap().bytes(), b"one\n");
        assert_eq!(from_file(b"one").unwrap().bytes(), b"one");
    }

    #[test]
    fn an_empty_document_is_refused() {
        let error = from_file(b"").unwrap_err();
        assert_eq!(error.to_string(), "content is empty");
    }

    #[test]
    fn a_document_holding_a_nul_is_refused() {
        let error = from_file(b"before\0after").unwrap_err();
        assert!(error.to_string().contains("NUL"));
    }

    #[test]
    fn a_document_above_the_cap_is_refused() {
        let oversized = vec![b'x'; MAX_CONTENT_BYTES + 1];
        let error = from_file(&oversized).unwrap_err();
        assert!(error.to_string().contains("above the"));
    }

    #[test]
    fn a_document_at_the_cap_is_accepted() {
        let exact = vec![b'x'; MAX_CONTENT_BYTES];
        assert_eq!(from_file(&exact).unwrap().size(), MAX_CONTENT_BYTES as u64);
    }

    #[test]
    fn typed_text_is_neither_capped_nor_trimmed() {
        let environment = OneDocument(Vec::new());
        let long = "x".repeat(MAX_CONTENT_BYTES + 1);
        let source = TextSource::Inline(format!("{long}\n"));
        let content = read_content(&environment, &source).unwrap();
        assert_eq!(content.size() as usize, MAX_CONTENT_BYTES + 2);
    }

    #[test]
    fn a_file_lends_its_base_name_and_a_pipe_cannot() {
        let file = TextSource::File(PathBuf::from("/work/notes/backends.md"));
        assert_eq!(attachment_name(&file, None).unwrap(), "backends.md");
        assert_eq!(
            attachment_name(&file, Some("other.md")).unwrap(),
            "other.md"
        );

        let piped = TextSource::Stdin;
        assert!(attachment_name(&piped, None).is_err());
        assert_eq!(
            attachment_name(&piped, Some("piped.md")).unwrap(),
            "piped.md"
        );
    }

    #[test]
    fn a_refusal_reads_as_one_sentence() {
        let error: ShellError = ShellError::refused("content is empty");
        assert_eq!(error.to_string(), "content is empty");
    }
}
