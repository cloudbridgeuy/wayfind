//! A vector that cannot be empty.

use std::num::NonZeroUsize;

use serde::{Deserialize, Deserializer, Serialize};

use crate::error::{Error, Result};

/// A vector that is known to hold at least one element.
///
/// This is what lets [`InitiativeState::Ready`] mean "ready": a ready
/// initiative has a frontier, and a frontier with nothing in it cannot be
/// built.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct NonEmptyVec<T>(Vec<T>);

impl<T> NonEmptyVec<T> {
    /// The operator-facing name of this value, used in errors.
    pub const FIELD: &'static str = "non-empty list";

    /// The first element, which always exists.
    pub fn first(&self) -> &T {
        // `self.0` is non-empty by construction, so indexing is total here.
        &self.0[0]
    }

    /// A borrowed view of every element.
    pub fn as_slice(&self) -> &[T] {
        &self.0
    }

    /// Iterate over the elements.
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.0.iter()
    }

    /// How many elements there are, which is never zero.
    pub fn count(&self) -> NonZeroUsize {
        NonZeroUsize::new(self.0.len()).unwrap_or(NonZeroUsize::MIN)
    }

    /// Consume the wrapper and return the plain vector.
    pub fn into_vec(self) -> Vec<T> {
        self.0
    }
}

impl<T> TryFrom<Vec<T>> for NonEmptyVec<T> {
    type Error = Error;

    fn try_from(values: Vec<T>) -> Result<Self> {
        if values.is_empty() {
            return Err(Error::invalid_value(
                Self::FIELD,
                "must hold at least one element",
            ));
        }
        Ok(Self(values))
    }
}

impl<T> IntoIterator for NonEmptyVec<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a NonEmptyVec<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for NonEmptyVec<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let values = Vec::<T>::deserialize(deserializer)?;
        Self::try_from(values).map_err(serde::de::Error::custom)
    }
}
