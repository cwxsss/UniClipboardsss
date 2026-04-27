//! daemon 运行模式。

use std::fmt;

/// 桌面 daemon 的运行模式。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DaemonRunMode {
    /// 独立 daemon，由 CLI 或用户直接启动。
    Standalone,
    /// GUI sidecar，由 Tauri GUI 启动，生命周期跟随 GUI。
    GuiSidecar,
    /// 常驻 daemon，GUI 只是连接它的客户端。
    Hybrid,
}

/// daemon 运行模式参数错误。
#[derive(Debug)]
pub struct DaemonRunModeParseError {
    message: &'static str,
}

impl fmt::Display for DaemonRunModeParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message)
    }
}

impl std::error::Error for DaemonRunModeParseError {}

impl DaemonRunMode {
    /// 从 daemon 命令行标志转换到明确的运行模式。
    pub fn from_flags(gui_managed: bool, hybrid: bool) -> Result<Self, DaemonRunModeParseError> {
        match (gui_managed, hybrid) {
            (true, true) => Err(DaemonRunModeParseError {
                message: "--hybrid cannot be combined with --gui-managed",
            }),
            (true, false) => Ok(Self::GuiSidecar),
            (false, true) => Ok(Self::Hybrid),
            (false, false) => Ok(Self::Standalone),
        }
    }

    /// 是否需要跟随 GUI 父进程退出。
    pub fn follows_gui_parent(self) -> bool {
        matches!(self, Self::GuiSidecar)
    }

    /// 是否等 GUI 发出 ready 后再启动剪贴板相关服务。
    pub fn waits_for_gui_ready(self) -> bool {
        matches!(self, Self::GuiSidecar)
    }

    /// 是否使用用户设置里的自动解锁开关。
    pub fn uses_auto_unlock_setting(self) -> bool {
        matches!(self, Self::GuiSidecar | Self::Hybrid)
    }

    /// 解锁成功后是否由 daemon 自己触发延迟服务。
    pub fn auto_triggers_deferred_services(self) -> bool {
        !matches!(self, Self::GuiSidecar)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_flags_map_to_run_mode() {
        assert_eq!(
            DaemonRunMode::from_flags(true, false).unwrap(),
            DaemonRunMode::GuiSidecar
        );
        assert_eq!(
            DaemonRunMode::from_flags(false, false).unwrap(),
            DaemonRunMode::Standalone
        );
        assert_eq!(
            DaemonRunMode::from_flags(false, true).unwrap(),
            DaemonRunMode::Hybrid
        );
        assert!(DaemonRunMode::from_flags(true, true).is_err());
    }

    #[test]
    fn hybrid_behaves_as_persistent_daemon() {
        let mode = DaemonRunMode::Hybrid;

        assert!(!mode.follows_gui_parent());
        assert!(!mode.waits_for_gui_ready());
        assert!(mode.uses_auto_unlock_setting());
        assert!(mode.auto_triggers_deferred_services());
    }
}
