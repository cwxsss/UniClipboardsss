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
