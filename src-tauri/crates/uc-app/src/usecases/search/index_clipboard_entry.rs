//! IndexClipboardEntry use case — indexes a clipboard entry into the search index (SIDX-01).

use std::sync::Arc;
use uc_core::ports::SearchIndexPort;
use uc_core::search::{SearchDocument, SearchError, SearchPosting};

/// Use case that indexes a clipboard entry's document and postings into the search index.
///
/// This is a thin orchestrator: the caller is responsible for constructing
/// `SearchDocument` and `Vec<SearchPosting>`. Tokenization and HMAC computation
/// live in Phase 90.
pub struct IndexClipboardEntry {
    search_index: Arc<dyn SearchIndexPort>,
}

impl IndexClipboardEntry {
    /// Construct from a `SearchIndexPort`.
    pub fn from_port(search_index: Arc<dyn SearchIndexPort>) -> Self {
        Self { search_index }
    }

    /// Index the given document and postings into the search index.
    ///
    /// Delegates directly to `SearchIndexPort::index_entry` and propagates the result unchanged.
    #[tracing::instrument(
        name = "usecase.index_clipboard_entry.execute",
        skip(self, document, postings),
        fields(entry_id = %document.entry_id, posting_count = postings.len())
    )]
    pub async fn execute(
        &self,
        document: SearchDocument,
        postings: Vec<SearchPosting>,
    ) -> Result<(), SearchError> {
        self.search_index.index_entry(document, postings).await?;
        tracing::debug!("entry indexed successfully");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_mocks::MockSearchIndex;
    use std::sync::Arc;
    use uc_core::ids::{EntryId, EventId};
    use uc_core::search::{ContentType, SearchDocument, SearchError, SearchPosting};

    fn make_test_document(entry_id: &str) -> SearchDocument {
        SearchDocument {
            entry_id: EntryId::from(entry_id),
            event_id: EventId::from("evt-test"),
            active_time_ms: 1000,
            captured_at_ms: 1000,
            content_type: ContentType::Text,
            file_extensions: vec![],
            mime_type: "text/plain".into(),
            indexed_at_ms: 1000,
            index_version: "v1".into(),
            text_preview: Some("hello world".into()),
        }
    }

    fn make_test_postings(entry_id: &str) -> Vec<SearchPosting> {
        vec![SearchPosting {
            term_tag: vec![1, 2, 3, 4],
            entry_id: EntryId::from(entry_id),
            field_mask: 1,
            term_freq: 1,
        }]
    }

    #[tokio::test]
    async fn execute_delegates_to_port_and_propagates_ok() {
        let last_doc = Arc::new(std::sync::Mutex::new(None::<SearchDocument>));
        let last_postings = Arc::new(std::sync::Mutex::new(None::<Vec<SearchPosting>>));
        let last_doc_clone = last_doc.clone();
        let last_postings_clone = last_postings.clone();

        let mut mock = MockSearchIndex::new();
        mock.expect_index_entry().returning(move |d, p| {
            *last_doc_clone.lock().unwrap() = Some(d);
            *last_postings_clone.lock().unwrap() = Some(p);
            Ok(())
        });

        let uc = IndexClipboardEntry::from_port(Arc::new(mock));
        let doc = make_test_document("entry-1");
        let postings = make_test_postings("entry-1");

        let result = uc.execute(doc.clone(), postings.clone()).await;
        assert!(result.is_ok());
        assert_eq!(*last_doc.lock().unwrap(), Some(doc));
        assert_eq!(*last_postings.lock().unwrap(), Some(postings));
    }

    #[tokio::test]
    async fn execute_propagates_port_error() {
        let mut mock = MockSearchIndex::new();
        mock.expect_index_entry()
            .returning(|_, _| Err(SearchError::IndexUnavailable));

        let uc = IndexClipboardEntry::from_port(Arc::new(mock));
        let result = uc
            .execute(make_test_document("entry-2"), make_test_postings("entry-2"))
            .await;

        assert!(matches!(result, Err(SearchError::IndexUnavailable)));
    }
}
