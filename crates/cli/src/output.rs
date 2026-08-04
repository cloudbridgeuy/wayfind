//! Where a command's answer goes.
//!
//! Every handler writes through this trait rather than through `println!`, for
//! two reasons. A test can hand a command a buffer and read back exactly what
//! an operator would have seen; and `wayfind attach show --raw` can put bytes
//! that are not text on standard output without a formatter in the way.
//!
//! There is no error channel here. A refusal is a [`crate::error::ShellError`]
//! travelling back up to `main`, which prints it on standard error — so a
//! handler cannot accidentally report a failure on the stream a pipeline is
//! reading.

use std::io::Write;

use crate::error::{ShellError, ShellResult};

/// A sink for what a command produces.
pub trait Output {
    /// Write a rendered document.
    fn text(&mut self, text: &str) -> ShellResult<()>;

    /// Write bytes exactly as they are.
    fn bytes(&mut self, bytes: &[u8]) -> ShellResult<()>;

    /// Make sure everything written has left the program.
    fn flush(&mut self) -> ShellResult<()>;
}

impl<W: Write> Output for W {
    fn text(&mut self, text: &str) -> ShellResult<()> {
        self.bytes(text.as_bytes())
    }

    fn bytes(&mut self, bytes: &[u8]) -> ShellResult<()> {
        Write::write_all(self, bytes).map_err(|error| ShellError::stream("write output", error))
    }

    fn flush(&mut self) -> ShellResult<()> {
        Write::flush(self).map_err(|error| ShellError::stream("write output", error))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::Output;

    #[test]
    fn a_buffer_collects_what_an_operator_would_have_seen() {
        let mut buffer: Vec<u8> = Vec::new();
        {
            let sink: &mut dyn Output = &mut buffer;
            sink.text("# Map\n").unwrap();
            sink.bytes(&[0xff, 0xfe]).unwrap();
            sink.flush().unwrap();
        }
        assert_eq!(buffer, b"# Map\n\xff\xfe");
    }
}
