//! UniClipboard Application Orchestration Layer
//!
//! This crate contains business logic use cases and runtime orchestration.

// Tracing support for use case instrumentation
pub use tracing;

pub mod app_paths;
pub mod deps;
pub mod models;
pub mod usecases;

pub use deps::AppDeps;

/// The application runtime.
pub struct App {
    /// Dependency grouping for direct construction
    pub deps: Option<AppDeps>,
}

impl App {
    /// Create new App instance from dependencies
    ///
    /// All dependencies must be provided - no defaults, no optionals.
    pub fn new(deps: AppDeps) -> Self {
        Self { deps: Some(deps) }
    }
}
