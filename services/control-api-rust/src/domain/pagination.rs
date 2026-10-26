//! Cursor paging, sorting, and facets.
//!
//! Keyset paging, not `OFFSET` (ADR 0006). Page N with `OFFSET` scans and
//! discards N×page rows, so deep pages degrade linearly, and a concurrent insert
//! shifts rows between pages so a row can appear twice or vanish.
//!
//! A cursor encodes the last sort key plus the tiebreaker id. The id is not
//! optional: without a unique tiebreaker, two rows with the same timestamp can
//! page incorrectly.

use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// A page of results plus the facts needed to fetch the next one.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Page<T> {
    pub items: Vec<T>,
    /// `None` means this was the last page. Distinguished from "empty page",
    /// which can happen with a filter and is not the same thing.
    pub next_cursor: Option<String>,
    /// An estimate, not an exact count. Exact counts turn a page into a scan,
    /// and the UI only needs to say "about N".
    pub total_estimate: i64,
    /// Counts per value for the filter fields, computed in the same round trip
    /// so the filter UI does not issue a request per facet.
    pub facets: Facets,
}

impl<T> Page<T> {
    /// An empty page that still reports the facets.
    pub fn empty(facets: Facets, total_estimate: i64) -> Self {
        Self {
            items: Vec::new(),
            next_cursor: None,
            total_estimate,
            facets,
        }
    }
}

/// Facet counts keyed by field, then by value.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct Facets {
    /// e.g. `{"status": {"running": 8123, "stopped": 2309}}`
    #[serde(flatten)]
    pub values: std::collections::BTreeMap<String, std::collections::BTreeMap<String, i64>>,
}

impl Facets {
    /// Record a count for a field value.
    pub fn add(&mut self, field: &str, value: &str, count: i64) {
        self.values
            .entry(field.to_string())
            .or_default()
            .insert(value.to_string(), count);
    }
}

/// Which column to sort by.
///
/// An enum, not a string: the value is interpolated into an `ORDER BY`, so it
/// must come from a closed set and never from user input unchecked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SortField {
    UpdatedAt,
    CreatedAt,
    Name,
    Status,
}

impl SortField {
    /// The SQL column. Only ever called with a value of this enum, which is the
    /// property that makes an `ORDER BY` safe here.
    pub fn column(self) -> &'static str {
        match self {
            Self::UpdatedAt => "updated_at",
            Self::CreatedAt => "created_at",
            Self::Name => "name",
            Self::Status => "status",
        }
    }

    /// Parse from a query parameter. Unknown values fall back to the default
    /// rather than erroring: a bad sort is not worth a 400 on a list request.
    pub fn parse(value: &str) -> Self {
        match value {
            "created_at" => Self::CreatedAt,
            "name" => Self::Name,
            "status" => Self::Status,
            _ => Self::UpdatedAt,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    Asc,
    Desc,
}

impl SortDirection {
    pub fn sql(self) -> &'static str {
        match self {
            Self::Asc => "ASC",
            Self::Desc => "DESC",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "asc" => Self::Asc,
            _ => Self::Desc,
        }
    }
}

/// The position in a sorted result set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor {
    /// The sort key of the last row on the previous page.
    pub key: CursorKey,
    /// The tiebreaker id of that row.
    pub id: Uuid,
}

/// The sort key, typed so a cursor cannot carry a value the column cannot hold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CursorKey {
    Timestamp(DateTime<Utc>),
    Text(String),
}

impl Cursor {
    /// Encode for transport. Opaque on purpose: clients must not construct one,
    /// which keeps the encoding free to change.
    pub fn encode(&self) -> String {
        let raw = serde_json::to_vec(self).unwrap_or_default();
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw)
    }

    /// Decode a cursor from a query parameter.
    ///
    /// Returns `None` on anything malformed rather than panicking: a cursor is
    /// client-supplied input and must not be able to crash a request.
    pub fn decode(encoded: &str) -> Option<Self> {
        let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(encoded)
            .ok()?;
        serde_json::from_slice(&raw).ok()
    }
}

/// The parsed list query: filters, sort, and page size.
#[derive(Debug, Clone)]
pub struct ListParams {
    pub cursor: Option<Cursor>,
    pub limit: i64,
    pub sort: SortField,
    pub direction: SortDirection,
    /// Field/value pairs. The store turns these into bound parameters; they
    /// never become SQL text.
    pub filters: std::collections::BTreeMap<String, String>,
    pub search: Option<String>,
}

impl Default for ListParams {
    fn default() -> Self {
        Self {
            cursor: None,
            limit: 50,
            sort: SortField::UpdatedAt,
            direction: SortDirection::Desc,
            filters: Default::default(),
            search: None,
        }
    }
}

impl ListParams {
    /// Maximum page size. A client asking for more is clamped rather than
    /// rejected, because the intent ("give me as much as possible") is clear and
    /// the cap protects the database.
    pub const MAX_LIMIT: i64 = 200;
    /// Default page size.
    pub const DEFAULT_LIMIT: i64 = 50;

    /// Build from raw query parameters, clamping and defaulting.
    pub fn from_query(
        cursor: Option<String>,
        limit: Option<i64>,
        sort: Option<String>,
        direction: Option<String>,
        search: Option<String>,
        filters: std::collections::BTreeMap<String, String>,
    ) -> Self {
        Self {
            cursor: cursor.as_deref().and_then(Cursor::decode),
            limit: limit
                .unwrap_or(Self::DEFAULT_LIMIT)
                .clamp(1, Self::MAX_LIMIT),
            sort: sort.as_deref().map(SortField::parse).unwrap_or(SortField::UpdatedAt),
            direction: direction
                .as_deref()
                .map(SortDirection::parse)
                .unwrap_or(SortDirection::Desc),
            filters,
            search: search.filter(|s| !s.trim().is_empty()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cursor_round_trips() {
        let cursor = Cursor {
            key: CursorKey::Timestamp(Utc::now()),
            id: Uuid::new_v4(),
        };
        let encoded = cursor.encode();
        let decoded = Cursor::decode(&encoded).expect("decodes");
        assert_eq!(decoded.id, cursor.id);
    }

    #[test]
    fn a_malformed_cursor_is_none_not_a_panic() {
        // Client input must never be able to abort a request.
        assert!(Cursor::decode("not-base64!!").is_none());
        assert!(Cursor::decode("").is_none());
        assert!(Cursor::decode("YWJj").is_none()); // valid base64, invalid JSON
    }

    #[test]
    fn the_page_size_is_clamped() {
        let params = ListParams::from_query(None, Some(10_000), None, None, None, Default::default());
        assert_eq!(params.limit, ListParams::MAX_LIMIT);
        let params = ListParams::from_query(None, Some(0), None, None, None, Default::default());
        assert_eq!(params.limit, 1);
        let params = ListParams::from_query(None, None, None, None, None, Default::default());
        assert_eq!(params.limit, ListParams::DEFAULT_LIMIT);
    }

    #[test]
    fn an_unknown_sort_falls_back_to_the_default() {
        assert_eq!(SortField::parse("'; DROP TABLE sandboxes; --"), SortField::UpdatedAt);
        assert_eq!(SortField::parse("name"), SortField::Name);
    }

    #[test]
    fn sort_columns_come_from_a_closed_set() {
        // The whole safety argument for interpolating ORDER BY.
        for field in [
            SortField::UpdatedAt,
            SortField::CreatedAt,
            SortField::Name,
            SortField::Status,
        ] {
            let column = field.column();
            assert!(column.chars().all(|c| c.is_ascii_lowercase() || c == '_'));
        }
    }

    #[test]
    fn facets_accumulate_per_field() {
        let mut facets = Facets::default();
        facets.add("status", "running", 3);
        facets.add("status", "stopped", 1);
        let status = facets.values.get("status").expect("status facet");
        assert_eq!(status.get("running"), Some(&3));
        assert_eq!(status.get("stopped"), Some(&1));
    }
}
