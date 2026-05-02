//! 独立 CLI 业务命令共享的启动辅助函数。
//!
//! 每个业务命令都运行一个自包含 application session,不再走 daemon HTTP。
//! 这里集中处理 daemon 冲突探测、日志 profile 和默认设备名。

use crate::exit_codes;
use crate::local_daemon::probe_running;
use crate::ui;

/// [`build_app_session`] 返回的 CLI 会话。
pub struct CliAppSession {
    pub runtime: uc_bootstrap::CliAppRuntime,
}

impl CliAppSession {
    pub fn app_facade(&self) -> &std::sync::Arc<uc_application::facade::AppFacade> {
        self.runtime.app_facade()
    }

    pub async fn shutdown(self) {
        self.runtime.shutdown().await;
    }
}

/// 当同 profile 已有 daemon 运行时拒绝执行业务命令。
///
/// 在 IPC 转发落地前,同一个 profile 的两个进程会用同一个 Ed25519
/// secret 绑定两个 iroh endpoint,并且 daemon 自己的流程会和 CLI 竞争。
/// 因此独立 CLI 业务命令要求用户先 `stop` daemon。
pub async fn refuse_if_daemon_running() -> Result<(), i32> {
    match probe_running().await {
        Ok(true) => {
            ui::error(
                "A daemon is already running for this profile. Stop it first with \
                 `uniclip stop`, or rerun under a different --profile.",
            );
            Err(exit_codes::EXIT_DAEMON_UNREACHABLE)
        }
        Ok(false) => Ok(()),
        // 探测网络错误按“没有可冲突 daemon”处理。
        Err(err) => {
            tracing::debug!(error = %err, "daemon probe failed; assuming no daemon");
            Ok(())
        }
    }
}

/// 为 CLI 业务命令构造独立 application session。
///
/// 默认使用 `Cli` 日志 profile;`verbose` 打开时切到 `Dev`,方便调试
/// 单机双进程 pairing。
///
/// wiring 前设置 `UC_DISABLE_SYSTEM_CLIPBOARD=1`,避免独立 CLI 命令提前触碰
/// 系统剪贴板适配器。
pub async fn build_app_session(verbose: bool) -> Result<CliAppSession, i32> {
    // 必须在 bootstrap wiring 前设置,避免 CLI 进程触碰系统剪贴板适配器。
    std::env::set_var("UC_DISABLE_SYSTEM_CLIPBOARD", "1");

    let log_profile = if verbose {
        Some(uc_observability::LogProfile::Dev)
    } else {
        Some(uc_observability::LogProfile::Cli)
    };
    match uc_bootstrap::build_cli_app_runtime(log_profile).await {
        Ok(runtime) => Ok(CliAppSession { runtime }),
        Err(err) => {
            ui::error(&format!("Failed to wire dependencies: {err}"));
            Err(exit_codes::EXIT_ERROR)
        }
    }
}

/// 从系统 hostname 推导默认设备名。
///
/// 设置了 `UC_PROFILE` 时追加 profile 后缀,方便单机双实例时区分设备。
/// hostname 读取失败或不是 UTF-8 时返回 `None`。
pub fn default_device_name() -> Option<String> {
    let raw = hostname::get().ok()?.into_string().ok()?;
    let trimmed = raw.trim().to_string();
    if trimmed.is_empty() {
        return None;
    }
    match std::env::var("UC_PROFILE") {
        Ok(p) if !p.is_empty() => Some(format!("{trimmed} ({p})")),
        _ => Some(trimmed),
    }
}
