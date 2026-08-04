//! The stable error tokens and the rejection they travel in.
//!
//! A caller reads the token, not the prose. The spelling and the exit code are
//! therefore part of the contract, and they are pinned by a table test rather
//! than left to whatever each call site happens to write.

use crate::render::Field;

/// The reason a command refused to act.
///
/// Exit code 1 — an internal fault — has no token on purpose: it is not a
/// refusal the operator can answer, so there is nothing stable to name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorToken {
    /// The arguments do not name a command this binary can run.
    Usage,
    /// The command existed once and was withdrawn.
    RetiredCommand,
    /// Nothing here carries that name or identifier.
    NotFound,
    /// An identifier prefix matches more than one record.
    AmbiguousId,
    /// A manifest was read but does not hold together.
    InvalidManifest,
    /// A record's stored hash does not match its content.
    BadHash,
    /// The edge would close a loop in a graph that must stay acyclic.
    Cycle,
    /// A vocabulary term is not one of the words the domain accepts.
    UnknownWord,
    /// The named source cannot be imported from.
    InvalidSource,
    /// Another record in the same project already holds that name.
    NameTaken,
    /// The record moved between the read and the write.
    StaleWrite,
    /// The row belongs to the immutable graph and cannot be changed.
    Immutable,
    /// Importing across initiatives has to go through a fork.
    ImportRequiresFork,
    /// Someone else holds the claim.
    ClaimHeld,
    /// The caller does not hold the claim it is trying to use.
    Unclaimed,
    /// The initiative is closed and takes no further work.
    ClosedInitiative,
    /// The destination already holds records.
    TargetNotEmpty,
}

impl ErrorToken {
    /// The stable spelling a caller matches on.
    pub fn as_token(self) -> &'static str {
        match self {
            Self::Usage => "usage",
            Self::RetiredCommand => "retired-command",
            Self::NotFound => "not-found",
            Self::AmbiguousId => "ambiguous-id",
            Self::InvalidManifest => "invalid-manifest",
            Self::BadHash => "bad-hash",
            Self::Cycle => "cycle",
            Self::UnknownWord => "unknown-word",
            Self::InvalidSource => "invalid-source",
            Self::NameTaken => "name-taken",
            Self::StaleWrite => "stale-write",
            Self::Immutable => "immutable",
            Self::ImportRequiresFork => "import-requires-fork",
            Self::ClaimHeld => "claim-held",
            Self::Unclaimed => "unclaimed",
            Self::ClosedInitiative => "closed-initiative",
            Self::TargetNotEmpty => "target-not-empty",
        }
    }

    /// The process exit code this token belongs to.
    ///
    /// Codes group tokens by class, so a script can branch on the code alone
    /// when the exact token does not matter.
    pub fn exit_code(self) -> u8 {
        match self {
            Self::Usage | Self::RetiredCommand => 2,
            Self::NotFound | Self::AmbiguousId => 3,
            Self::InvalidManifest
            | Self::BadHash
            | Self::Cycle
            | Self::UnknownWord
            | Self::InvalidSource
            | Self::NameTaken => 4,
            Self::StaleWrite => 5,
            Self::Immutable
            | Self::ImportRequiresFork
            | Self::ClaimHeld
            | Self::Unclaimed
            | Self::ClosedInitiative
            | Self::TargetNotEmpty => 6,
        }
    }
}

/// A refusal, ready to be rendered.
///
/// The keys are what the caller needs to act: which initiative, which path,
/// which candidates. They keep insertion order, because the order is part of
/// the document. The body is prose for a human and carries no contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rejection {
    token: ErrorToken,
    keys: Vec<(&'static str, Field)>,
    body: Option<String>,
}

impl Rejection {
    /// Refuse with this token and nothing else said yet.
    pub fn new(token: ErrorToken) -> Self {
        Self {
            token,
            keys: Vec::new(),
            body: None,
        }
    }

    /// Add a front-matter key naming what was refused.
    pub fn key(mut self, name: &'static str, value: Field) -> Self {
        self.keys.push((name, value));
        self
    }

    /// Add prose after the front matter, usually the next step to take.
    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// The token this rejection carries.
    pub fn token(&self) -> ErrorToken {
        self.token
    }

    /// The keys, in the order they were added.
    pub fn keys(&self) -> &[(&'static str, Field)] {
        &self.keys
    }

    /// The prose, when there is any.
    pub fn body_text(&self) -> Option<&str> {
        self.body.as_deref()
    }

    /// The process exit code this rejection ends the command with.
    pub fn exit_code(&self) -> u8 {
        self.token.exit_code()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{ErrorToken, Rejection};
    use crate::render::Field;

    #[test]
    fn every_token_has_the_s1_spelling_and_exit_code() {
        let table = [
            (ErrorToken::Usage, "usage", 2),
            (ErrorToken::RetiredCommand, "retired-command", 2),
            (ErrorToken::NotFound, "not-found", 3),
            (ErrorToken::AmbiguousId, "ambiguous-id", 3),
            (ErrorToken::InvalidManifest, "invalid-manifest", 4),
            (ErrorToken::BadHash, "bad-hash", 4),
            (ErrorToken::Cycle, "cycle", 4),
            (ErrorToken::UnknownWord, "unknown-word", 4),
            (ErrorToken::InvalidSource, "invalid-source", 4),
            (ErrorToken::NameTaken, "name-taken", 4),
            (ErrorToken::StaleWrite, "stale-write", 5),
            (ErrorToken::Immutable, "immutable", 6),
            (ErrorToken::ImportRequiresFork, "import-requires-fork", 6),
            (ErrorToken::ClaimHeld, "claim-held", 6),
            (ErrorToken::Unclaimed, "unclaimed", 6),
            (ErrorToken::ClosedInitiative, "closed-initiative", 6),
            (ErrorToken::TargetNotEmpty, "target-not-empty", 6),
        ];
        for (token, spelling, code) in table {
            assert_eq!(token.as_token(), spelling);
            assert_eq!(token.exit_code(), code);
        }
    }

    #[test]
    fn a_rejection_keeps_its_keys_in_the_order_they_were_added() {
        let rejection = Rejection::new(ErrorToken::AmbiguousId)
            .key("id", Field::Text("R-a3".to_string()))
            .key("candidates", Field::Ids(vec!["R-a3f9".to_string()]))
            .body("Name more of the hash.");
        assert_eq!(rejection.token(), ErrorToken::AmbiguousId);
        assert_eq!(
            rejection.keys().iter().map(|(k, _)| *k).collect::<Vec<_>>(),
            vec!["id", "candidates"]
        );
        assert_eq!(rejection.body_text(), Some("Name more of the hash."));
        assert_eq!(rejection.exit_code(), 3);
    }

    #[test]
    fn a_rejection_says_nothing_it_was_not_told_to_say() {
        let rejection = Rejection::new(ErrorToken::NotFound);
        assert!(rejection.keys().is_empty());
        assert_eq!(rejection.body_text(), None);
    }
}
