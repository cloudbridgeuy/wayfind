//! The search boundary.
//!
//! Search is separate from [`crate::storage`] on purpose. A store answers "what
//! is this record"; a search backend answers "which records are about this". The
//! first is exact and cheap, the second is ranked and expensive, and a backend
//! can be good at one without being good at the other. Today SQLite's FTS5 does
//! both; keeping the traits apart means a later move of search alone costs one
//! adapter rather than a rewrite.
//!
//! Two things this boundary promises.
//!
//! **The order is total.** Relevance decides first, and the ticket identifier
//! breaks every tie. Two hits therefore never trade places between calls, which
//! is what makes paging safe: page two of an unchanged corpus holds what page
//! one did not.
//!
//! **The backend's own vocabulary stays inside the backend.** A hit carries a
//! `metadata` map for backend-specific numbers — an FTS5 rank, a vector
//! distance — under namespaced keys. Nothing above this boundary reads them, so
//! adding one never changes a render.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};

use crate::error::{Error, Result};
use crate::id::{InitiativeId, TicketId};
use crate::model::TicketStatusLabel;

/// The metadata key prefix an FTS5 backend uses.
///
/// Namespacing is the whole point: a hit from two backends merged into one page
/// must not have two different numbers under one key.
pub const FTS5_METADATA_NAMESPACE: &str = "fts5";

/// What the operator is looking for.
///
/// The text is passed to the backend's own query language, so it is only
/// checked for being present here. A backend rejects its own syntax errors
/// through [`SearchError::InvalidQuery`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct SearchQuery(String);

impl SearchQuery {
    /// The operator-facing name of this value, used in errors.
    pub const FIELD: &'static str = "search query";

    /// Parse raw text into a query.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(Error::invalid_value(Self::FIELD, "must not be empty"));
        }
        Ok(Self(value))
    }

    /// The query text as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume the query and return the owned text.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl FromStr for SearchQuery {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self> {
        Self::new(text)
    }
}

impl fmt::Display for SearchQuery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for SearchQuery {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// How many hits one page may hold.
///
/// A limit of zero would ask a backend to do the work and report none of it, so
/// it is rejected rather than treated as "no limit". A caller that wants
/// everything asks for [`SearchLimit::MAX`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct SearchLimit(u32);

impl SearchLimit {
    /// The operator-facing name of this value, used in errors.
    pub const FIELD: &'static str = "search limit";

    /// The largest page Wayfind asks a backend for.
    pub const MAX: SearchLimit = SearchLimit(500);

    /// The page size a command uses when the operator does not say.
    pub const DEFAULT: SearchLimit = SearchLimit(20);

    /// Parse a raw count into a page size.
    pub fn new(value: u32) -> Result<Self> {
        if value == 0 {
            return Err(Error::invalid_value(Self::FIELD, "must be at least 1"));
        }
        if value > Self::MAX.0 {
            return Err(Error::invalid_value(
                Self::FIELD,
                format!("must be at most {}, got {value}", Self::MAX.0),
            ));
        }
        Ok(Self(value))
    }

    /// The count, for an adapter to bind.
    pub fn get(self) -> u32 {
        self.0
    }
}

impl Default for SearchLimit {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl fmt::Display for SearchLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<'de> Deserialize<'de> for SearchLimit {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let value = u32::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// How many hits to skip before the page starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize)]
#[serde(transparent)]
pub struct SearchOffset(u32);

impl SearchOffset {
    /// The start of the results.
    pub const START: SearchOffset = SearchOffset(0);

    /// Wrap a raw skip count. Any count is valid, including zero.
    pub fn new(value: u32) -> Self {
        Self(value)
    }

    /// The count, for an adapter to bind.
    pub fn get(self) -> u32 {
        self.0
    }

    /// The offset of the page after one of the given size.
    ///
    /// Saturating rather than wrapping: an offset that stops moving returns an
    /// empty page, while one that wraps would silently return page one again.
    pub fn advance(self, limit: SearchLimit) -> Self {
        Self(self.0.saturating_add(limit.get()))
    }
}

impl fmt::Display for SearchOffset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<'de> Deserialize<'de> for SearchOffset {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        Ok(Self(u32::deserialize(deserializer)?))
    }
}

/// One complete search request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchRequest {
    /// What to look for.
    pub query: SearchQuery,
    /// The initiative to look inside. Search never crosses initiatives.
    pub initiative_id: InitiativeId,
    /// How many hits this page may hold.
    pub limit: SearchLimit,
    /// How many hits to skip first.
    pub offset: SearchOffset,
}

impl SearchRequest {
    /// The first page of a query, at the default page size.
    pub fn first_page(query: SearchQuery, initiative_id: InitiativeId) -> Self {
        Self {
            query,
            initiative_id,
            limit: SearchLimit::DEFAULT,
            offset: SearchOffset::START,
        }
    }

    /// The same query, one page further on.
    pub fn next_page(&self) -> Self {
        Self {
            query: self.query.clone(),
            initiative_id: self.initiative_id,
            limit: self.limit,
            offset: self.offset.advance(self.limit),
        }
    }
}

/// One matching ticket.
///
/// A hit carries what a result list prints and nothing more. It does not carry
/// the ticket's question or its resolution: printing a list of thirty hits must
/// not load thirty full records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    /// The matching ticket.
    pub ticket_id: TicketId,
    /// The ticket's title.
    pub title: String,
    /// Where the ticket sits in its lifecycle.
    pub status: TicketStatusLabel,
    /// The matching passage, with the backend's emphasis markers already in it.
    pub snippet: String,
    /// Backend-specific numbers under namespaced keys, such as `fts5.rank`.
    ///
    /// A [`BTreeMap`] and not a hash map, so two runs of the same search print
    /// their metadata in the same order.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

/// One page of results.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchPage {
    /// The hits, in the total order this boundary promises: most relevant
    /// first, ties broken by ascending ticket identifier.
    pub hits: Vec<SearchHit>,
    /// The request that produced them, so a caller can page on without keeping
    /// its own bookkeeping.
    pub request: SearchRequest,
    /// Whether asking for the next page could return anything.
    pub has_more: bool,
}

impl SearchPage {
    /// The request for the page after this one, if there could be one.
    pub fn next_request(&self) -> Option<SearchRequest> {
        self.has_more.then(|| self.request.next_page())
    }
}

/// Why a search could not be answered.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SearchError {
    /// The backend rejected the query's syntax.
    #[error("search query is not valid: {detail}")]
    InvalidQuery {
        /// What the backend reported.
        detail: String,
    },
    /// The backend could not be reached, or failed for a reason the domain has
    /// no opinion about.
    #[error("search failed during {operation}: {detail}")]
    Infrastructure {
        /// The operation that failed, for the operator's benefit.
        operation: &'static str,
        /// What the backend reported.
        detail: String,
    },
    /// The backend returned a hit the domain says cannot exist.
    #[error("{0}")]
    CorruptData(#[from] Error),
}

impl SearchError {
    /// Build a [`SearchError::InvalidQuery`] without repeating the syntax.
    pub fn invalid_query(detail: impl Into<String>) -> Self {
        SearchError::InvalidQuery {
            detail: detail.into(),
        }
    }

    /// Build a [`SearchError::Infrastructure`] without repeating the syntax.
    pub fn infrastructure(operation: &'static str, detail: impl Into<String>) -> Self {
        SearchError::Infrastructure {
            operation,
            detail: detail.into(),
        }
    }
}

/// The result type a search backend returns.
pub type SearchResult<T> = std::result::Result<T, SearchError>;

/// Answering "which tickets are about this".
pub trait SearchBackend {
    /// Run one search and return one page.
    fn search(&self, request: &SearchRequest) -> SearchResult<SearchPage>;
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::collections::BTreeMap;

    use super::{
        SearchBackend, SearchError, SearchHit, SearchLimit, SearchOffset, SearchPage, SearchQuery,
        SearchRequest, FTS5_METADATA_NAMESPACE,
    };
    use crate::id::{InitiativeId, TicketId};
    use crate::model::TicketStatusLabel;

    /// Compiles only while [`SearchBackend`] stays object safe.
    fn accepts_any_backend(_backend: &dyn SearchBackend) {}

    fn request() -> SearchRequest {
        SearchRequest::first_page(
            SearchQuery::new("cache").unwrap(),
            InitiativeId::new(1).unwrap(),
        )
    }

    #[test]
    fn search_backend_is_object_safe() {
        let _: fn(&dyn SearchBackend) = accepts_any_backend;
    }

    #[test]
    fn queries_must_carry_text() {
        assert_eq!(SearchQuery::new("cache").unwrap().as_str(), "cache");
        assert!(SearchQuery::new("").is_err());
        assert!(SearchQuery::new("   ").is_err());
    }

    #[test]
    fn limits_reject_zero_and_anything_past_the_ceiling() {
        assert_eq!(SearchLimit::new(1).unwrap().get(), 1);
        assert_eq!(SearchLimit::default(), SearchLimit::DEFAULT);
        assert!(SearchLimit::new(0).is_err());
        assert_eq!(SearchLimit::new(500).unwrap(), SearchLimit::MAX);
        assert!(SearchLimit::new(501).is_err());
    }

    #[test]
    fn paging_walks_forward_by_one_page_at_a_time() {
        let first = request();
        assert_eq!(first.offset, SearchOffset::START);
        let second = first.next_page();
        assert_eq!(second.offset.get(), SearchLimit::DEFAULT.get());
        assert_eq!(second.query, first.query);
        assert_eq!(
            second.next_page().offset.get(),
            SearchLimit::DEFAULT.get() * 2
        );
    }

    #[test]
    fn an_offset_stops_rather_than_wrapping_back_to_the_first_page() {
        let last = SearchOffset::new(u32::MAX);
        assert_eq!(last.advance(SearchLimit::DEFAULT), last);
    }

    #[test]
    fn a_last_page_offers_no_next_request() {
        let page = SearchPage {
            hits: Vec::new(),
            request: request(),
            has_more: false,
        };
        assert!(page.next_request().is_none());
        let page = SearchPage {
            has_more: true,
            ..page
        };
        assert_eq!(page.next_request(), Some(request().next_page()));
    }

    #[test]
    fn hit_metadata_is_namespaced_and_ordered() {
        let mut metadata = BTreeMap::new();
        metadata.insert(
            format!("{FTS5_METADATA_NAMESPACE}.rank"),
            serde_json::json!(-1.25),
        );
        let hit = SearchHit {
            ticket_id: TicketId::new(4).unwrap(),
            title: "Cache the map".to_string(),
            status: TicketStatusLabel::Open,
            snippet: "the **cache** is cold".to_string(),
            metadata,
        };
        let encoded = serde_json::to_string(&hit).unwrap();
        assert!(encoded.contains("\"fts5.rank\":-1.25"));
        assert_eq!(serde_json::from_str::<SearchHit>(&encoded).unwrap(), hit);
    }

    #[test]
    fn a_hit_without_metadata_omits_the_field_entirely() {
        let hit = SearchHit {
            ticket_id: TicketId::new(4).unwrap(),
            title: "Cache the map".to_string(),
            status: TicketStatusLabel::Open,
            snippet: "the **cache** is cold".to_string(),
            metadata: BTreeMap::new(),
        };
        let encoded = serde_json::to_string(&hit).unwrap();
        assert!(!encoded.contains("metadata"));
        assert_eq!(serde_json::from_str::<SearchHit>(&encoded).unwrap(), hit);
    }

    #[test]
    fn search_errors_separate_a_bad_query_from_a_broken_backend() {
        assert_eq!(
            SearchError::invalid_query("unterminated phrase").to_string(),
            "search query is not valid: unterminated phrase"
        );
        assert_eq!(
            SearchError::infrastructure("search", "no such table: ticket_search").to_string(),
            "search failed during search: no such table: ticket_search"
        );
    }
}
