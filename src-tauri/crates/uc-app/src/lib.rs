//! UniClipboard Application Orchestration Layer (D16-1 residue).
//!
//! D14/D15 retired the legacy `CoreUseCases` accessor and Tauri/CLI/daemon
//! now reach the application layer through `uc_application::facade::*`.
//! D16-1 deletes every dead use case file and the `App` struct that wrapped
//! `AppDeps`. What remains is a transition surface used by uc-bootstrap and
//! uc-tauri only:
//!
//! * `runtime::CoreRuntime` — bootstrap-only handle, scheduled for removal
//!   alongside D16-2's wider migration.
//! * `deps::{AppDeps, ClipboardPorts, DevicePorts, SecurityPorts,
//!   StoragePorts, SystemPorts}` — the port bundle still consumed by
//!   uc-bootstrap; D16-2 relocates it into `uc-application`.
//! * `task_registry::TaskRegistry` — the tokio task lifecycle helper still
//!   shared with uc-bootstrap; D16-2 moves it into uc-bootstrap proper.
//! * `usecases::*` — three slim re-export shims (`app_lifecycle`,
//!   `clipboard`, `file_sync`) kept so existing import paths resolve until
//!   D16-2 rewrites them.
//! * `app_paths` / `shared::host_event` — pre-existing re-export shims that
//!   still serve as compatibility aliases for `uc_application::facade`.

// Tracing support for use case instrumentation
pub use tracing;

pub mod app_paths;
pub mod deps;
pub mod runtime;
pub mod shared;
pub mod task_registry;
pub mod usecases;

pub use deps::{AppDeps, ClipboardPorts, DevicePorts, SecurityPorts, StoragePorts, SystemPorts};
pub use runtime::CoreRuntime;
