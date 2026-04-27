//! daemon 运行模式。

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

impl DaemonRunMode {
    /// 从旧的 `--gui-managed` 标志转换到明确的运行模式。
    pub fn from_gui_managed_flag(gui_managed: bool) -> Self {
        if gui_managed {
            Self::GuiSidecar
        } else {
            Self::Standalone
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
    fn legacy_gui_managed_flag_maps_to_gui_sidecar() {
        assert_eq!(
            DaemonRunMode::from_gui_managed_flag(true),
            DaemonRunMode::GuiSidecar
        );
        assert_eq!(
            DaemonRunMode::from_gui_managed_flag(false),
            DaemonRunMode::Standalone
        );
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
