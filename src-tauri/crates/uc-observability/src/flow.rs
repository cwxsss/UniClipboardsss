/// Flow correlation ID for clipboard capture pipeline tracing.
///
/// Wraps a UUID v7 (time-ordered) to provide monotonic, unique identifiers
/// that can be attached as tracing span fields via the `Display` impl.
use std::fmt;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FlowId(Uuid);

impl FlowId {
    /// Generate a new flow ID using UUID v7 (time-ordered).
    pub fn generate() -> Self {
        Self(Uuid::now_v7())
    }
}

impl fmt::Display for FlowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
