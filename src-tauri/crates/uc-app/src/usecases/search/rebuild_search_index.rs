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
