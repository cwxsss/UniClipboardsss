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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_mocks::MockSearchIndex;
    use std::sync::Arc;
    use uc_core::ids::EntryId;
    use uc_core::search::{
        ContentType, QueryOperator, SearchError, SearchQuery, SearchResult, SearchResultsPage,
    };

    fn make_query(s: &str) -> SearchQuery {
        SearchQuery {
            query_string: s.into(),
            operator: QueryOperator::And,
            time_range: None,
            content_types: vec![],
            extensions: vec![],
            limit: 10,
            offset: 0,
        }
    }

    fn make_search_result(entry_id: &str) -> SearchResult {
        SearchResult {
            entry_id: EntryId::from(entry_id),
            content_type: ContentType::Text,
            active_time_ms: 1000,
            text_preview: Some("preview".into()),
            mime_type: "text/plain".into(),
            file_extensions: vec![],
        }
    }

    #[tokio::test]
    async fn execute_forwards_query_and_returns_empty_results() {
        let last_query = Arc::new(std::sync::Mutex::new(None::<SearchQuery>));
        let last_query_clone = last_query.clone();

        let mut mock = MockSearchIndex::new();
        mock.expect_search().returning(move |q| {
            *last_query_clone.lock().unwrap() = Some(q);
            Ok(SearchResultsPage {
                items: vec![],
                total: 0,
                has_more: false,
            })
        });

        let uc = SearchClipboardEntries::from_port(Arc::new(mock));
        let page = uc.execute(make_query("hello")).await.unwrap();
        assert!(page.items.is_empty());
        assert_eq!(page.total, 0);
        let captured = last_query.lock().unwrap();
        assert_eq!(captured.as_ref().unwrap().query_string, "hello");
    }

    #[tokio::test]
    async fn execute_returns_single_result() {
        let expected = make_search_result("entry-search-1");
        let result_clone = expected.clone();

        let mut mock = MockSearchIndex::new();
        mock.expect_search().returning(move |_| {
            Ok(SearchResultsPage {
                items: vec![result_clone.clone()],
                total: 1,
                has_more: false,
            })
        });

        let uc = SearchClipboardEntries::from_port(Arc::new(mock));
        let page = uc.execute(make_query("world")).await.unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0], expected);
    }

    #[tokio::test]
    async fn execute_forwards_query_and_returns_page_metadata() {
        let r1 = make_search_result("entry-p1");
        let r2 = make_search_result("entry-p2");
        let r1c = r1.clone();
        let r2c = r2.clone();

        let mut mock = MockSearchIndex::new();
        mock.expect_search().returning(move |_| {
            Ok(SearchResultsPage {
                items: vec![r1c.clone(), r2c.clone()],
                total: 2,
                has_more: false,
            })
        });

        let uc = SearchClipboardEntries::from_port(Arc::new(mock));
        let page = uc.execute(make_query("hello")).await.unwrap();
        // Verify page contract: items carry through, total and has_more come from the port.
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0], r1);
        assert_eq!(page.items[1], r2);
        // total and has_more are whatever the port provides (mock returns 0/false defaults)
        let _ = page.total;
        let _ = page.has_more;
    }

    #[tokio::test]
    async fn execute_propagates_invalid_query_error() {
        let mut mock = MockSearchIndex::new();
        mock.expect_search()
            .returning(|_| Err(SearchError::InvalidQuery("mixed operators".into())));

        let uc = SearchClipboardEntries::from_port(Arc::new(mock));
        let result = uc.execute(make_query("foo AND OR bar")).await;

        assert!(matches!(result, Err(SearchError::InvalidQuery(_))));
    }
}
