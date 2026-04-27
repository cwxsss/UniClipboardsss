//! daemon 兼容入口。
//!
//! 独立的 `uniclipboard-daemon` 二进制和 `uniclipboard-cli daemon` 子命令
//! 都通过这里启动同一套 daemon 进程。

pub use crate::daemon::host::run;
