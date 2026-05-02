//! `uc-daemon` 现在只保留兼容出口。
//!
//! 桌面宿主实现已迁入 `uc-desktop`。外部仍可继续使用
//! `uniclipboard-daemon` 二进制和 `uc_daemon::*` 路径，后续迁移完成后再
//! 逐步收窄这个兼容层。

/// 旧 `uc_daemon::entrypoint::run` 路径。
pub mod entrypoint {
    pub use uc_desktop::daemon::run;
}

/// 旧 `uc_daemon::daemon::*` 路径。
pub mod daemon {
    pub mod run_mode {
        pub use uc_desktop::daemon::run_mode::*;
    }
}

/// 旧 `uc_daemon::process_metadata::*` 路径。
pub mod process_metadata {
    pub use uc_daemon_local::process_metadata::*;
}
