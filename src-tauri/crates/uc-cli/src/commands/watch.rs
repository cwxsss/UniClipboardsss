//! `uniclipboard-cli watch` — foreground inbound clipboard observer
//! (Slice 2 Phase 2 · T11).
//!
//! Self-contained direct-mode command (no daemon). The backing
//! `SpaceSetupAssembly` auto-spawns the ingest loop at construction
//! (Phase 2 · T10), so this command's job is purely to subscribe to the
//! application-level notice broadcast and render each delivery until
//! Ctrl-C.
//!
//! Phase 2 deliberately does **not** write to the system clipboard
//! (plan §5.3): a short-lived CLI process writing the OS clipboard would
//! collide with the daemon's own watcher and trigger a sync echo. Daemon
//! integration arrives in Phase 3 / Slice 4.

use serde::Serialize;
use tokio::sync::broadcast;

use uc_application::facade::space_setup::TryResumeSessionError;
use uc_application::facade::{InboundAction, InboundNotice};

use crate::commands::slice1_common::{build_assembly, refuse_if_daemon_running};
use crate::exit_codes;
use crate::ui;

pub async fn run(json: bool, verbose: bool) -> i32 {
    if !json {
        ui::header("Watch inbound clipboard");
    }

    if let Err(code) = refuse_if_daemon_running().await {
        return code;
    }

    let assembly = match build_assembly(verbose).await {
        Ok(bundle) => bundle.assembly,
        Err(code) => return code,
    };

    let resume_spinner = ui::spinner("Resuming space session...");
    match assembly.facade.try_resume_session().await {
        Ok(true) => ui::spinner_finish_success(&resume_spinner, "Session resumed"),
        Ok(false) => {
            ui::spinner_finish_error(
                &resume_spinner,
                "No space on this profile — run `init` or `join` first.",
            );
            assembly.shutdown().await;
            return exit_codes::EXIT_ERROR;
        }
        Err(TryResumeSessionError::CorruptedKeyMaterial) => {
            ui::spinner_finish_error(
                &resume_spinner,
                "Key material is corrupted — consider resetting this profile.",
            );
            assembly.shutdown().await;
            return exit_codes::EXIT_ERROR;
        }
        Err(TryResumeSessionError::KeyringMiss) => {
            ui::spinner_finish_error(
                &resume_spinner,
                "Keychain cannot silently unlock this space.",
            );
            assembly.shutdown().await;
            return exit_codes::EXIT_ERROR;
        }
        Err(TryResumeSessionError::Internal(msg)) => {
            ui::spinner_finish_error(&resume_spinner, &format!("Resume failed: {msg}"));
            assembly.shutdown().await;
            return exit_codes::EXIT_ERROR;
        }
    }

    // Refresh presence so the sender side, when it dispatches, doesn't
    // skip us under "Unknown" — same routine as `members` / `send`.
    let probe_spinner = ui::spinner("Probing paired peers...");
    match assembly.facade.refresh_presence().await {
        Ok(report) => ui::spinner_finish_success(
            &probe_spinner,
            &format!(
                "Probed {} peer(s): {} online, {} offline, {} error(s)",
                report.total,
                report.online,
                report.offline,
                report.errors.len()
            ),
        ),
        Err(err) => ui::spinner_finish_error(
            &probe_spinner,
            &format!("Probe round failed: {err} (proceeding)"),
        ),
    }

    let mut rx = assembly.clipboard_sync.subscribe_inbound_notices();
    if !json {
        ui::info("status", "Listening — press Ctrl-C to stop");
        ui::bar();
    }

    let exit_code = loop {
        tokio::select! {
            biased;
            _ = tokio::signal::ctrl_c() => {
                if !json {
                    ui::end("Stopped");
                }
                break exit_codes::EXIT_SUCCESS;
            }
            recv = rx.recv() => {
                match recv {
                    Ok(notice) => render_notice(&notice, json),
                    Err(broadcast::error::RecvError::Lagged(missed)) => {
                        if !json {
                            ui::warn(&format!(
                                "Lagged: dropped {missed} notice(s); next frame catches up"
                            ));
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        if !json {
                            ui::warn("Inbound channel closed; exiting.");
                        }
                        break exit_codes::EXIT_ERROR;
                    }
                }
            }
        }
    };

    assembly.shutdown().await;
    exit_code
}

fn render_notice(notice: &InboundNotice, json: bool) {
    if json {
        let dto = NoticeDto::from(notice);
        match serde_json::to_string(&dto) {
            Ok(line) => println!("{line}"),
            Err(err) => ui::error(&format!("Failed to serialize notice: {err}")),
        }
        return;
    }

    let preview = String::from_utf8_lossy(&notice.plaintext);
    let line = format!(
        "[{}] {} ({})",
        notice.from_device.as_str(),
        truncate_preview(&preview),
        format_action(notice.action),
    );
    ui::info("·", &line);
}

fn truncate_preview(text: &str) -> String {
    const MAX: usize = 120;
    let single_line = text.replace('\n', "\\n");
    if single_line.chars().count() > MAX {
        let truncated: String = single_line.chars().take(MAX).collect();
        format!("{truncated}…")
    } else {
        single_line
    }
}

fn format_action(action: InboundAction) -> &'static str {
    match action {
        InboundAction::NewEntry => "new",
        InboundAction::DuplicateIgnored => "duplicate",
    }
}

#[derive(Serialize)]
struct NoticeDto<'a> {
    from_device: &'a str,
    content_hash: &'a str,
    plaintext_utf8: String,
    action: &'static str,
    at_ms: i64,
}

impl<'a> From<&'a InboundNotice> for NoticeDto<'a> {
    fn from(n: &'a InboundNotice) -> Self {
        Self {
            from_device: n.from_device.as_str(),
            content_hash: &n.content_hash,
            plaintext_utf8: String::from_utf8_lossy(&n.plaintext).into_owned(),
            action: format_action(n.action),
            at_ms: n.at_ms,
        }
    }
}
