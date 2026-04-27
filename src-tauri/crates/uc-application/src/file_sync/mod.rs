//! File-sync use cases owned by the application layer.
//!
//! Slice 5 / D16-2 relocated `CleanupExpiredFilesUseCase` here from
//! `uc-app::usecases::file_sync` so the file-cache cleanup task can be
//! constructed without depending on the legacy `uc-app` crate.

pub mod cleanup;

pub use cleanup::CleanupExpiredFilesUseCase;
