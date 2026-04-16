use anyhow::Result;
use std::sync::Arc;
use tracing::{info, info_span, Instrument};
use uc_core::clipboard::ClipboardEntry;
use uc_core::ports::ClipboardEntryRepositoryPort;

/// Use case for listing clipboard entries with pagination
/// 列出剪贴板条目的用例（分页）
pub struct ListClipboardEntries {
    entry_repo: Arc<dyn ClipboardEntryRepositoryPort>,
    max_limit: usize,
}

impl ListClipboardEntries {
    /// Create a new use case instance from a trait object
    /// 从 trait 对象创建新的用例实例
    pub fn from_arc(entry_repo: Arc<dyn ClipboardEntryRepositoryPort>) -> Self {
        Self {
            entry_repo,
            max_limit: 1000, // Business rule: maximum 1000 entries per query
        }
    }

    /// Lists clipboard entries starting at `offset` and returning up to `limit` entries.
    ///
    /// Validates `limit` against the business maximum and returns repository errors with context.
    ///
    /// # Parameters
    ///
    /// * `limit` — Maximum number of entries to return; must be at least 1 and at most the use-case's configured max.
    /// * `offset` — Number of entries to skip from the start of the result set.
    ///
    /// # Returns
    ///
    /// A `Vec<ClipboardEntry>` containing up to `limit` entries beginning at `offset`.
    ///
    /// # Errors
    ///
    /// Returns an error if `limit` is 0, `limit` exceeds the configured maximum, or the repository query fails.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use uc_app::usecases::ListClipboardEntries;
    /// # use uc_core::ports::ClipboardEntryRepositoryPort;
    /// # async fn doc_example() -> anyhow::Result<()> {
    /// // `entry_repo` should be a concrete implementation of the port.
    /// let entry_repo: Arc<dyn ClipboardEntryRepositoryPort> = todo!();
    /// let usecase = ListClipboardEntries::from_arc(entry_repo);
    /// let entries = usecase.execute(10, 0).await?;
    /// assert!(entries.len() <= 10);
    /// # Ok(()) }
    /// ```
    pub async fn execute(&self, limit: usize, offset: usize) -> Result<Vec<ClipboardEntry>> {
        // Create use case span (child of command's root span)
        let span = info_span!(
            "usecase.list_clipboard_entries.execute",
            limit = limit,
            offset = offset,
        );
        async {
            info!("Starting clipboard entries query");

            // Validate limit
            if limit == 0 {
                return Err(anyhow::anyhow!(
                    "Invalid limit: {}. Must be at least 1",
                    limit
                ));
            }

            if limit > self.max_limit {
                return Err(anyhow::anyhow!(
                    "Invalid limit: {}. Must be at most {}",
                    limit,
                    self.max_limit
                ));
            }

            // Query repository
            let result = self
                .entry_repo
                .list_entries(limit, offset)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to query clipboard entries: {}", e))?;

            info!(count = result.len(), "Retrieved clipboard entries");
            Ok(result)
        }
        .instrument(span)
        .await
    }
}
