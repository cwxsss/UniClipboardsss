//! daemon 关闭信号接入。

use tokio_util::sync::CancellationToken;

use super::run_mode::DaemonRunMode;

/// GUI sidecar 模式下，父进程会保持 daemon 的 stdin 管道打开。
///
/// 一旦 GUI 正常退出、崩溃或被强制结束，stdin 会关闭；这里把 EOF 转成
/// `CancellationToken`，交给 daemon 主循环统一做优雅关闭。
pub fn build_external_shutdown_token(run_mode: DaemonRunMode) -> Option<CancellationToken> {
    if !run_mode.follows_gui_parent() {
        return None;
    }

    let token = CancellationToken::new();
    let token_clone = token.clone();
    std::thread::spawn(move || {
        use std::io::Read;

        let mut buf = [0u8; 1];
        let _ = std::io::stdin().read(&mut buf);
        token_clone.cancel();
    });

    Some(token)
}
