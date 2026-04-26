use async_trait::async_trait;
use thiserror::Error;

mod projection;

use uc_core::search::{ContentType, QueryOperator, SearchError, SearchQuery, TimeRangeFilter};

pub use projection::SearchProjectionBuilder;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQueryInput {
    pub query: String,
    pub operator: Option<String>,
    pub time_preset: Option<String>,
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
    pub content_types: Option<String>,
    pub extensions: Option<String>,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchPageView {
    pub total: u32,
    pub has_more: bool,
    pub items: Vec<SearchResultView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResultView {
    pub entry_id: String,
    pub content_type: String,
    pub active_time_ms: i64,
    pub text_preview: Option<String>,
    pub mime_type: String,
    pub file_extensions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchStatusView {
    pub state: String,
    pub reason: Option<String>,
    pub last_rebuild_started_at_ms: Option<i64>,
    pub last_rebuild_completed_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRebuildAcceptedView {
    pub accepted: bool,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum SearchFacadeError {
    #[error("invalid query: {0}")]
    InvalidQuery(String),
    #[error("bad search request: {0}")]
    BadRequest(String),
    #[error("search session is locked")]
    SessionLocked,
    #[error("search index is not ready")]
    IndexNotReady,
    #[error("search index is unavailable")]
    IndexUnavailable,
    #[error("search service is unavailable: {0}")]
    ServiceUnavailable(String),
    #[error("search rebuild is already running")]
    RebuildAlreadyRunning,
    #[error("search failed: {0}")]
    Internal(String),
}

#[async_trait]
pub trait SearchGateway: Send + Sync {
    async fn query(&self, query: SearchQuery) -> Result<SearchPageView, SearchFacadeError>;

    async fn status(&self) -> Result<SearchStatusView, SearchFacadeError>;

    async fn request_rebuild(&self) -> Result<SearchRebuildAcceptedView, SearchFacadeError>;
}

pub struct SearchFacade {
    gateway: Box<dyn SearchGateway>,
}

impl SearchFacade {
    pub fn new(gateway: Box<dyn SearchGateway>) -> Self {
        Self { gateway }
    }

    pub async fn query(
        &self,
        input: SearchQueryInput,
    ) -> Result<SearchPageView, SearchFacadeError> {
        let query = parse_search_query(input)?;
        self.gateway.query(query).await
    }

    pub async fn status(&self) -> Result<SearchStatusView, SearchFacadeError> {
        self.gateway.status().await
    }

    pub async fn request_rebuild(&self) -> Result<SearchRebuildAcceptedView, SearchFacadeError> {
        self.gateway.request_rebuild().await
    }
}

fn parse_search_query(input: SearchQueryInput) -> Result<SearchQuery, SearchFacadeError> {
    let (query_string, inferred_operator) = strip_and_infer_operator(&input.query)?;

    let operator = if let Some(operator) = input.operator.as_deref() {
        match operator.to_lowercase().as_str() {
            "and" => QueryOperator::And,
            "or" => QueryOperator::Or,
            _ => {
                return Err(SearchFacadeError::BadRequest(format!(
                    "invalid operator: {operator}"
                )))
            }
        }
    } else {
        inferred_operator.unwrap_or(QueryOperator::And)
    };

    Ok(SearchQuery {
        query_string,
        operator,
        time_range: parse_time_range(&input)?,
        content_types: parse_content_types(input.content_types.as_deref())?,
        extensions: parse_extensions(input.extensions.as_deref()),
        limit: input.limit.min(200),
        offset: input.offset,
    })
}

fn strip_and_infer_operator(
    raw: &str,
) -> Result<(String, Option<QueryOperator>), SearchFacadeError> {
    let tokens: Vec<&str> = raw.split_whitespace().collect();

    let mut has_and = false;
    let mut has_or = false;
    let mut non_operator_tokens: Vec<&str> = Vec::new();

    for token in &tokens {
        match token.to_uppercase().as_str() {
            "AND" => has_and = true,
            "OR" => has_or = true,
            _ => non_operator_tokens.push(token),
        }
    }

    if has_and && has_or {
        return Err(SearchFacadeError::InvalidQuery(
            "mixed AND/OR operators are not supported".to_string(),
        ));
    }

    let inferred = if has_and {
        Some(QueryOperator::And)
    } else if has_or {
        Some(QueryOperator::Or)
    } else {
        None
    };

    Ok((non_operator_tokens.join(" "), inferred))
}

fn parse_time_range(
    input: &SearchQueryInput,
) -> Result<Option<TimeRangeFilter>, SearchFacadeError> {
    let has_from = input.from_ms.is_some();
    let has_to = input.to_ms.is_some();

    if has_from != has_to {
        return Err(SearchFacadeError::BadRequest(
            "fromMs and toMs must both be present or both absent".to_string(),
        ));
    }

    if let (Some(from_ms), Some(to_ms)) = (input.from_ms, input.to_ms) {
        if from_ms < 0 || to_ms < 0 {
            return Err(SearchFacadeError::BadRequest(
                "fromMs and toMs must be non-negative".to_string(),
            ));
        }
        return Ok(Some(TimeRangeFilter::Absolute {
            from_ms: from_ms as u64,
            to_ms: to_ms as u64,
        }));
    }

    let Some(preset) = input.time_preset.as_deref() else {
        return Ok(None);
    };

    let filter = match preset {
        "today" => TimeRangeFilter::Today,
        "yesterday" => TimeRangeFilter::Yesterday,
        "last_24h" => TimeRangeFilter::Last24h,
        "last_7d" => TimeRangeFilter::Last7d,
        "last_30d" => TimeRangeFilter::Last30d,
        "this_week" => TimeRangeFilter::ThisWeek,
        "this_month" => TimeRangeFilter::ThisMonth,
        other => {
            return Err(SearchFacadeError::BadRequest(format!(
                "invalid timePreset: {other}"
            )))
        }
    };
    Ok(Some(filter))
}

fn parse_content_types(raw: Option<&str>) -> Result<Vec<ContentType>, SearchFacadeError> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };

    let mut result = Vec::new();
    for value in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let content_type = match value {
            "text" => ContentType::Text,
            "html" => ContentType::Html,
            "link" => ContentType::Link,
            "file" => ContentType::File,
            "image" => ContentType::Image,
            "other" => ContentType::Other,
            unknown => {
                return Err(SearchFacadeError::BadRequest(format!(
                    "invalid fileType: {unknown}"
                )))
            }
        };
        result.push(content_type);
    }
    Ok(result)
}

fn parse_extensions(raw: Option<&str>) -> Vec<String> {
    let Some(raw) = raw else {
        return Vec::new();
    };
    raw.split(',')
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

pub fn map_search_error(error: SearchError) -> SearchFacadeError {
    match error {
        SearchError::InvalidQuery(message) => SearchFacadeError::InvalidQuery(message),
        SearchError::SessionLocked => SearchFacadeError::SessionLocked,
        SearchError::IndexNotReady => SearchFacadeError::IndexNotReady,
        SearchError::IndexUnavailable => SearchFacadeError::IndexUnavailable,
        SearchError::Internal(message) => SearchFacadeError::Internal(message),
    }
}
