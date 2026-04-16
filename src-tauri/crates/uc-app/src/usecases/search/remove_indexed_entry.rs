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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_mocks::MockSearchIndex;
    use std::sync::Arc;
    use uc_core::ids::EntryId;
    use uc_core::search::SearchError;

    #[tokio::test]
    async fn execute_forwards_entry_id_to_port() {
        let captured = Arc::new(std::sync::Mutex::new(None::<EntryId>));
        let captured_clone = captured.clone();

        let mut mock = MockSearchIndex::new();
        mock.expect_remove_entry().returning(move |id| {
            *captured_clone.lock().unwrap() = Some(id.clone());
            Ok(())
        });

        let uc = RemoveIndexedEntry::from_port(Arc::new(mock));
        let entry_id = EntryId::from("entry-to-remove");

        let result = uc.execute(&entry_id).await;
        assert!(result.is_ok());
        assert_eq!(*captured.lock().unwrap(), Some(entry_id));
    }

    #[tokio::test]
    async fn execute_propagates_port_error() {
        let mut mock = MockSearchIndex::new();
        mock.expect_remove_entry()
            .returning(|_| Err(SearchError::IndexNotReady));

        let uc = RemoveIndexedEntry::from_port(Arc::new(mock));
        let result = uc.execute(&EntryId::from("entry-error")).await;

        assert!(matches!(result, Err(SearchError::IndexNotReady)));
    }
}
