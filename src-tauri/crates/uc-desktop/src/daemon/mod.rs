//! daemon 运行模式。

pub mod run_mode;
pub mod runtime_assembly;
pub mod search_assembly;
pub mod service_plan;
pub mod shutdown;
pub mod startup_recovery;

pub use crate::entrypoint::run;
