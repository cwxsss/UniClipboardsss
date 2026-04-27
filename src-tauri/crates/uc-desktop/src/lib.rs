//! UniClipboard 桌面宿主。
//!
//! 本 crate 承载桌面运行环境里的宿主能力：daemon 模式、后台服务、本地
//! HTTP/WS 接口接入和桌面事件源。业务动作仍然通过 `uc-application`
//! 的 facade 进入，不能在这里重新实现业务规则。

pub const DAEMON_VERSION: &str = env!("CARGO_PKG_VERSION");
pub use uc_daemon_contract::DAEMON_API_REVISION;

pub mod app;
pub mod daemon {
    //! daemon 运行模式入口。

    pub use crate::entrypoint::run;
}
pub mod entrypoint;
pub mod peers;
pub mod process_metadata;
pub mod search;
pub mod service;
pub mod state;
pub mod workers;
