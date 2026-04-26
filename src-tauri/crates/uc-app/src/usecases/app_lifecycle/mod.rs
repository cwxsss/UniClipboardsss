//! 已迁移到 `uc_application::facade::lifecycle`。

pub mod adapters;

pub use uc_application::facade::{
    InMemoryLifecycleStatus, LifecycleStateView as LifecycleState,
    LifecycleStatusGateway as LifecycleStatusPort,
};
