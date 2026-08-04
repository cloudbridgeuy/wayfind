//! The closed vocabularies a record's kind is drawn from.
//!
//! Every word here is a canonical-encoding field value: renaming, adding, or
//! removing a variant changes the hash of every record that carries it. The
//! spelling is the contract.

use std::fmt;
use std::str::FromStr;

use crate::error::{ErrorToken, Rejection};
use crate::render::Field;

/// Refuse a word that is not one of a vocabulary's accepted tokens.
fn refuse(field: &'static str, value: &str, accepted: &[&str]) -> Rejection {
    Rejection::new(ErrorToken::UnknownWord)
        .key("field", Field::Text(field.to_string()))
        .key("value", Field::Text(value.to_string()))
        .body(format!("Expected one of: {}.", accepted.join(", ")))
}

/// The kind of a result node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Destination,
    WayfindHandoff,
    DeliveryMap,
    DeliveryScope,
    Shape,
    Breadboard,
    ImplementationPlan,
    ImplementationResult,
    VerificationResult,
    CompletionResult,
    ClosureResult,
    ImportResult,
    BlockedResult,
    AbandonedResult,
    SupersedingResult,
    ImpactReview,
    ManualResult,
}

impl NodeKind {
    /// Every node kind, in the order the system design lists them.
    pub const ALL: [NodeKind; 17] = [
        Self::Destination,
        Self::WayfindHandoff,
        Self::DeliveryMap,
        Self::DeliveryScope,
        Self::Shape,
        Self::Breadboard,
        Self::ImplementationPlan,
        Self::ImplementationResult,
        Self::VerificationResult,
        Self::CompletionResult,
        Self::ClosureResult,
        Self::ImportResult,
        Self::BlockedResult,
        Self::AbandonedResult,
        Self::SupersedingResult,
        Self::ImpactReview,
        Self::ManualResult,
    ];

    /// The word this kind writes into a canonical encoding.
    pub fn as_token(self) -> &'static str {
        match self {
            Self::Destination => "destination",
            Self::WayfindHandoff => "wayfind-handoff",
            Self::DeliveryMap => "delivery-map",
            Self::DeliveryScope => "delivery-scope",
            Self::Shape => "shape",
            Self::Breadboard => "breadboard",
            Self::ImplementationPlan => "implementation-plan",
            Self::ImplementationResult => "implementation-result",
            Self::VerificationResult => "verification-result",
            Self::CompletionResult => "completion-result",
            Self::ClosureResult => "closure-result",
            Self::ImportResult => "import-result",
            Self::BlockedResult => "blocked-result",
            Self::AbandonedResult => "abandoned-result",
            Self::SupersedingResult => "superseding-result",
            Self::ImpactReview => "impact-review",
            Self::ManualResult => "manual-result",
        }
    }
}

impl fmt::Display for NodeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_token())
    }
}

impl FromStr for NodeKind {
    type Err = Rejection;

    fn from_str(text: &str) -> Result<Self, Rejection> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_token() == text)
            .ok_or_else(|| {
                let accepted: Vec<&str> = Self::ALL.iter().map(|kind| kind.as_token()).collect();
                refuse("node kind", text, &accepted)
            })
    }
}

/// The kind of a transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionKind {
    Wayfind,
    DefineMap,
    Shape,
    Breadboard,
    WritePlan,
    RecordImplementation,
    Verify,
    Complete,
    Close,
    Split,
    Merge,
    Block,
    Recover,
    Abandon,
    Supersede,
    Keep,
    Manual,
    Import,
}

impl TransitionKind {
    /// Every transition kind, in the order the system design lists them.
    ///
    /// `impact-review` is not a member: the system design's own text says an
    /// impact review "keeps, blocks, merges, supersedes, or abandons" the
    /// affected continuation — four of those five outcome verbs were already
    /// here, so `Keep` fills the fifth and `impact-review` remains only the
    /// node kind it names.
    pub const ALL: [TransitionKind; 18] = [
        Self::Wayfind,
        Self::DefineMap,
        Self::Shape,
        Self::Breadboard,
        Self::WritePlan,
        Self::RecordImplementation,
        Self::Verify,
        Self::Complete,
        Self::Close,
        Self::Split,
        Self::Merge,
        Self::Block,
        Self::Recover,
        Self::Abandon,
        Self::Supersede,
        Self::Keep,
        Self::Manual,
        Self::Import,
    ];

    /// The word this kind writes into a canonical encoding.
    pub fn as_token(self) -> &'static str {
        match self {
            Self::Wayfind => "wayfind",
            Self::DefineMap => "define-map",
            Self::Shape => "shape",
            Self::Breadboard => "breadboard",
            Self::WritePlan => "write-plan",
            Self::RecordImplementation => "record-implementation",
            Self::Verify => "verify",
            Self::Complete => "complete",
            Self::Close => "close",
            Self::Split => "split",
            Self::Merge => "merge",
            Self::Block => "block",
            Self::Recover => "recover",
            Self::Abandon => "abandon",
            Self::Supersede => "supersede",
            Self::Keep => "keep",
            Self::Manual => "manual",
            Self::Import => "import",
        }
    }
}

impl fmt::Display for TransitionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_token())
    }
}

impl FromStr for TransitionKind {
    type Err = Rejection;

    fn from_str(text: &str) -> Result<Self, Rejection> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_token() == text)
            .ok_or_else(|| {
                let accepted: Vec<&str> = Self::ALL.iter().map(|kind| kind.as_token()).collect();
                refuse("transition kind", text, &accepted)
            })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::str::FromStr;

    use super::{NodeKind, TransitionKind};
    use crate::error::ErrorToken;

    #[test]
    fn node_kind_covers_the_system_design_set() {
        assert_eq!(NodeKind::ALL.len(), 17);
        for kind in NodeKind::ALL {
            assert_eq!(NodeKind::from_str(kind.as_token()).unwrap(), kind);
        }
    }

    #[test]
    fn transition_kind_covers_the_system_design_set() {
        assert_eq!(TransitionKind::ALL.len(), 18);
        for kind in TransitionKind::ALL {
            assert_eq!(TransitionKind::from_str(kind.as_token()).unwrap(), kind);
        }
    }

    #[test]
    fn an_unknown_word_is_refused_and_lists_the_accepted_set() {
        let rejection = NodeKind::from_str("banana").unwrap_err();
        assert_eq!(rejection.token(), ErrorToken::UnknownWord);
        assert!(rejection
            .body_text()
            .is_some_and(|body| body.contains("destination")));

        let rejection = TransitionKind::from_str("banana").unwrap_err();
        assert_eq!(rejection.token(), ErrorToken::UnknownWord);
        assert!(rejection
            .body_text()
            .is_some_and(|body| body.contains("wayfind")));
    }
}
