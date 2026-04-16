//! SearchClipboardEntries use case — executes a structured search query against the index (D-06).

use std::sync::Arc;
use uc_core::ports::SearchIndexPort;
use uc_core::search::{SearchError, SearchQuery, SearchResultsPage};

/// Use case that executes a structured search query against the local encrypted search index.
///
/// Returns `SearchResultsPage` — paged result with items, total, and has_more
/// (per D-01, D-02 Phase 92 — avoids a second query for UI hydration or pagination metadata).
pub struct SearchClipboardEntries {
    search_index: Arc<dyn SearchIndexPort>,
}

impl SearchClipboardEntries {
    /// Construct from a `SearchIndexPort`.
    pub fn from_port(search_index: Arc<dyn SearchIndexPort>) -> Self {
        Self { search_index }
    }

    /// Execute the given search query and return matching results.
    ///
    /// Delegates directly to `SearchIndexPort::search` and propagates the result unchanged.
    #[tracing::instrument(
        name = "usecase.search_clipboard_entries.execute",
        skip(self, query),
        fields(query_len = query.query_string.len(), operator = ?query.operator, limit = query.limit, offset = query.offset)
    )]
    pub async fn execute(&self, query: SearchQuery) -> Result<SearchResultsPage, SearchError> {
        let page = self.search_index.search(query).await?;
        tracing::debug!(
            total = page.total,
            returned = page.items.len(),
            has_more = page.has_more,
            "search completed"
        );
        Ok(page)
    }
}
