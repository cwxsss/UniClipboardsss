//! UniClipboard daemon 二进制入口。
//!
//! 这里保留旧命令入口，实际宿主实现委托给 `uc-desktop` 暴露的
//! `uc_daemon::entrypoint::run()`。

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let gui_managed = args.iter().any(|arg| arg == "--gui-managed");
    let hybrid = args.iter().any(|arg| arg == "--hybrid");

    if gui_managed && hybrid {
        anyhow::bail!("--hybrid cannot be combined with --gui-managed");
    }

    let run_mode = if hybrid {
        uc_daemon::daemon::run_mode::DaemonRunMode::Hybrid
    } else {
        uc_daemon::daemon::run_mode::DaemonRunMode::from_gui_managed_flag(gui_managed)
    };
    uc_daemon::entrypoint::run(run_mode)
}
