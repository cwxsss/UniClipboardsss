//! RemoveIndexedEntry use case — removes a clipboard entry from the search index (D-05).

use std::sync::Arc;
use uc_core::ids::EntryId;
use uc_core::ports::SearchIndexPort;
use uc_core::search::SearchError;

/// Use case that removes a clipboard entry (document + all postings) from the search index.
///
/// Called synchronously by the delete integration path (Phase 89, D-05) — hard-delete semantic.
pub struct RemoveIndexedEntry {
    search_index: Arc<dyn SearchIndexPort>,
}

impl RemoveIndexedEntry {
    /// Construct from a `SearchIndexPort`.
    pub fn from_port(search_index: Arc<dyn SearchIndexPort>) -> Self {
        Self { search_index }
    }

    /// Remove the search index document and postings for the given entry.
    ///
    /// Delegates directly to `SearchIndexPort::remove_entry` and propagates the result unchanged.
    #[tracing::instrument(
        name = "usecase.remove_indexed_entry.execute",
        skip(self),
        fields(entry_id = %entry_id)
    )]
    pub async fn execute(&self, entry_id: &EntryId) -> Result<(), SearchError> {
        self.search_index.remove_entry(entry_id).await?;
        tracing::debug!("entry removed from search index");
        Ok(())
    }
}
