//! `uc-daemon` 现在只保留兼容出口。
//!
//! 桌面宿主实现已迁入 `uc-desktop`。外部仍可继续使用
//! `uniclipboard-daemon` 二进制和 `uc_daemon::*` 路径，后续迁移完成后再
//! 逐步收窄这个兼容层。

pub use uc_desktop::*;
