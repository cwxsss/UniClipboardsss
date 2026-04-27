//! daemon 运行模式。

pub mod app_assembly;
pub mod app_facade_assembly;
pub mod background_tasks;
pub mod bootstrap;
pub mod host;
pub mod run_loop;
pub mod run_mode;
pub mod runtime_assembly;
pub mod runtime_controls;
pub mod search_assembly;
pub(crate) mod service;
pub mod service_assembly;
pub mod service_plan;
pub mod shutdown;
pub mod startup_recovery;
pub(crate) mod state;
pub mod tokio_runtime;

pub use host::run;
