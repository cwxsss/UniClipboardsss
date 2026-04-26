use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardListInput {
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryProjectionView {
    pub id: String,
    pub preview: String,
    pub has_detail: bool,
    pub size_bytes: i64,
    pub captured_at: i64,
    pub content_type: String,
    pub thumbnail_url: Option<String>,
    pub is_encrypted: bool,
    pub is_favorited: bool,
    pub updated_at: i64,
    pub active_time: i64,
    pub file_transfer_status: Option<String>,
    pub file_transfer_reason: Option<String>,
    pub link_urls: Option<Vec<String>>,
    pub link_domains: Option<Vec<String>>,
    pub file_sizes: Option<Vec<i64>>,
    pub image_width: Option<i32>,
    pub image_height: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryDetailView {
    pub id: String,
    pub content: String,
    pub size_bytes: i64,
    pub created_at_ms: i64,
    pub active_time_ms: i64,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryResourceView {
    pub blob_id: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: i64,
    pub url: Option<String>,
    pub inline_data: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardStatsView {
    pub total_items: i64,
    pub total_size: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClearHistoryResultView {
    pub deleted_count: u64,
    pub failed_entries: Vec<(String, String)>,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ClipboardHistoryError {
    #[error("entry not found")]
    NotFound,
    #[error("unsupported clipboard content")]
    UnsupportedContent,
    #[error("clipboard history operation failed: {0}")]
    Internal(String),
}

#[async_trait]
pub trait ClipboardHistoryGateway: Send + Sync {
    async fn list_entries(
        &self,
        input: ClipboardListInput,
    ) -> Result<Vec<EntryProjectionView>, ClipboardHistoryError>;

    async fn get_entry(&self, entry_id: &str) -> Result<EntryDetailView, ClipboardHistoryError>;

    async fn delete_entry(&self, entry_id: &str) -> Result<(), ClipboardHistoryError>;

    async fn toggle_favorite(
        &self,
        entry_id: &str,
        is_favorited: bool,
    ) -> Result<bool, ClipboardHistoryError>;

    async fn stats(&self) -> Result<ClipboardStatsView, ClipboardHistoryError>;

    async fn get_entry_resource(
        &self,
        entry_id: &str,
    ) -> Result<EntryResourceView, ClipboardHistoryError>;

    async fn clear_history(&self) -> Result<ClearHistoryResultView, ClipboardHistoryError>;
}

pub struct ClipboardHistoryFacade {
    gateway: Box<dyn ClipboardHistoryGateway>,
}

impl ClipboardHistoryFacade {
    pub fn new(gateway: Box<dyn ClipboardHistoryGateway>) -> Self {
        Self { gateway }
    }

    pub async fn list_entries(
        &self,
        input: ClipboardListInput,
    ) -> Result<Vec<EntryProjectionView>, ClipboardHistoryError> {
        self.gateway.list_entries(input).await
    }

    pub async fn get_entry(
        &self,
        entry_id: &str,
    ) -> Result<EntryDetailView, ClipboardHistoryError> {
        self.gateway.get_entry(entry_id).await
    }

    pub async fn delete_entry(&self, entry_id: &str) -> Result<(), ClipboardHistoryError> {
        self.gateway.delete_entry(entry_id).await
    }

    pub async fn toggle_favorite(
        &self,
        entry_id: &str,
        is_favorited: bool,
    ) -> Result<bool, ClipboardHistoryError> {
        self.gateway.toggle_favorite(entry_id, is_favorited).await
    }

    pub async fn stats(&self) -> Result<ClipboardStatsView, ClipboardHistoryError> {
        self.gateway.stats().await
    }

    pub async fn get_entry_resource(
        &self,
        entry_id: &str,
    ) -> Result<EntryResourceView, ClipboardHistoryError> {
        self.gateway.get_entry_resource(entry_id).await
    }

    pub async fn clear_history(&self) -> Result<ClearHistoryResultView, ClipboardHistoryError> {
        self.gateway.clear_history().await
    }
}
