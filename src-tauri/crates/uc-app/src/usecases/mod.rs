//! Surviving uc-app use case shims.
//!
//! D14/D15 retired the legacy `CoreUseCases` accessor — daemon, Tauri,
//! and CLI now reach the application layer exclusively through
//! `uc_application::facade::*`. D16-1 removed every dead use case file
//! that once hung off `CoreUseCases`. Only three modules survive here,
//! and they exist purely to keep call sites compiling until D16-2 / D17:
//!
//! * `app_lifecycle` — re-exports `LifecycleStatusGateway` /
//!   `InMemoryLifecycleStatus` / `LifecycleStateView` from
//!   `uc_application::facade::lifecycle`. `crate::runtime::CoreRuntime`
//!   imports the alias, so the shim stays until D16-2 deletes
//!   `CoreRuntime`.
//! * `clipboard` — re-exports `ClipboardWriteCoordinator` /
//!   `ClipboardWriteIntent` from `uc_application::clipboard_write` plus
//!   `ClipboardIntegrationMode` from `uc-core`. uc-bootstrap and
//!   uc-tauri still import the type from this path; D16-2 rewrites them
//!   to point at the real location.
//! * `file_sync` — keeps `CleanupExpiredFilesUseCase` because
//!   uc-tauri's `start_background_tasks` still constructs it. D16-2
//!   either relocates the use case to `uc-application` or inlines it
//!   into the bootstrap task wiring.

pub mod app_lifecycle;
pub mod clipboard;
pub mod file_sync;

pub use app_lifecycle::{InMemoryLifecycleStatus, LifecycleState, LifecycleStatusPort};
pub use clipboard::clipboard_write_coordinator::{ClipboardWriteCoordinator, ClipboardWriteIntent};
