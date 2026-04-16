use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceMetadata {
    pub trace_id: Uuid,
    pub timestamp: u64,
}

pub type OptionalTrace = Option<TraceMetadata>;

#[derive(Debug, Error)]
pub enum TraceParseError {
    #[error("Failed to parse trace metadata: {0}")]
    InvalidTrace(String),
}

pub fn extract_trace(args: &serde_json::Value) -> Result<OptionalTrace, TraceParseError> {
    let trace_value = match args.get("_trace") {
        Some(value) => value,
        None => return Ok(None),
    };

    serde_json::from_value(trace_value.clone())
        .map(Some)
        .map_err(|err| TraceParseError::InvalidTrace(err.to_string()))
}
