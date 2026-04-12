//! RebuildSearchIndex use case — triggers a full index rebuild via the search port (D-04).

use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use uc_core::ports::SearchIndexPort;
use uc_core::search::{RebuildProgress, SearchDocument, SearchError, SearchPosting};

/// Use case that triggers a full index rebuild.
///
/// The caller supplies an `mpsc::Sender<RebuildProgress>` so the daemon can forward
/// progress events over WebSocket without uc-core having any WebSocket knowledge (D-07).
/// The Sender is forwarded directly to the port — not cloned into a new channel.
pub struct RebuildSearchIndex {
    search_index: Arc<dyn SearchIndexPort>,
}

impl RebuildSearchIndex {
    /// Construct from a `SearchIndexPort`.
    pub fn from_port(search_index: Arc<dyn SearchIndexPort>) -> Self {
        Self { search_index }
    }

    /// Trigger a full index rebuild with the supplied entries and progress channel.
    ///
    /// Delegates directly to `SearchIndexPort::rebuild`, forwarding the caller-supplied
    /// `progress_tx` unchanged. Propagates the result unchanged.
    #[tracing::instrument(
        name = "usecase.rebuild_search_index.execute",
        skip(self, entries, progress_tx),
        fields(entry_count = entries.len())
    )]
    pub async fn execute(
        &self,
        entries: Vec<(SearchDocument, Vec<SearchPosting>)>,
        progress_tx: Sender<RebuildProgress>,
    ) -> Result<(), SearchError> {
        tracing::info!(entry_count = entries.len(), "starting search index rebuild");
        self.search_index.rebuild(entries, progress_tx).await?;
        tracing::info!("search index rebuild completed");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_mocks::MockSearchIndex;
    use std::sync::Arc;
    use uc_core::ids::{EntryId, EventId};
    use uc_core::search::{
        ContentType, RebuildProgress, RebuildStage, SearchDocument, SearchError,
    };

    fn make_test_document(entry_id: &str) -> SearchDocument {
        SearchDocument {
            entry_id: EntryId::from(entry_id),
            event_id: EventId::from("evt-rebuild"),
            active_time_ms: 1000,
            captured_at_ms: 1000,
            content_type: ContentType::Text,
            file_extensions: vec![],
            mime_type: "text/plain".into(),
            indexed_at_ms: 1000,
            index_version: "v1".into(),
            text_preview: None,
        }
    }

    #[tokio::test]
    async fn execute_forwards_sender_and_entries_to_port() {
        let last_count = Arc::new(std::sync::Mutex::new(None::<usize>));
        let last_count_clone = last_count.clone();

        let mut mock = MockSearchIndex::new();
        mock.expect_rebuild().returning(move |entries, tx| {
            let count = entries.len();
            *last_count_clone.lock().unwrap() = Some(count);
            // Send a progress event through the forwarded Sender to prove it was not dropped.
            let _ = tx.try_send(RebuildProgress {
                stage: RebuildStage::Started,
                indexed: 0,
                total: count as u32,
            });
            Ok(())
        });

        let uc = RebuildSearchIndex::from_port(Arc::new(mock));
        let entries = vec![
            (make_test_document("e1"), vec![]),
            (make_test_document("e2"), vec![]),
        ];
        let (tx, mut rx) = tokio::sync::mpsc::channel::<RebuildProgress>(4);

        let result = uc.execute(entries, tx).await;
        assert!(result.is_ok());

        // The mock sends progress via the forwarded Sender; the test receives it here.
        let progress = rx.recv().await.expect("expected a RebuildProgress message");
        assert_eq!(progress.stage, RebuildStage::Started);
        assert_eq!(progress.total, 2);

        assert_eq!(*last_count.lock().unwrap(), Some(2));
    }

    #[tokio::test]
    async fn execute_propagates_port_error() {
        let mut mock = MockSearchIndex::new();
        mock.expect_rebuild()
            .returning(|_, _| Err(SearchError::IndexNotReady));

        let uc = RebuildSearchIndex::from_port(Arc::new(mock));
        let (tx, _rx) = tokio::sync::mpsc::channel::<RebuildProgress>(4);

        let result = uc.execute(vec![], tx).await;
        assert!(matches!(result, Err(SearchError::IndexNotReady)));
    }
}
