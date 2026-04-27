//! UniClipboard daemon 二进制入口。
//!
//! 这里保留旧命令入口，实际宿主实现委托给 `uc-desktop` 暴露的
//! `uc_daemon::entrypoint::run()`。

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let gui_managed = args.iter().any(|arg| arg == "--gui-managed");
    let hybrid = args.iter().any(|arg| arg == "--hybrid");

    let run_mode = uc_daemon::daemon::run_mode::DaemonRunMode::from_flags(gui_managed, hybrid)?;
    uc_daemon::entrypoint::run(run_mode)
}
