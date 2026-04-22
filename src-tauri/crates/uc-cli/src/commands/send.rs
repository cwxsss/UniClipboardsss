//! `uniclipboard-cli send` — one-shot clipboard dispatch.
//!
//! Self-contained direct-mode command (no daemon), mirroring the `init` /
//! `invite` / `join` / `members` pattern. Builds a `SpaceSetupAssembly`,
//! resumes the cached session, refreshes presence so dispatch can route to
//! online peers, then wraps the user-supplied text as a single-text-rep
//! `SystemClipboardSnapshot` and fans it out via
//! `ClipboardSyncFacade::dispatch_snapshot`.
//!
//! Phase 3 upgrade(T9):text → V3 envelope。Phase 2 raw-bytes path retired
//! so daemon-on-the-other-side(Phase 3 T7/T8)can decode us uniformly.
//! CLI callers with daemon receivers will now interoperate natively.
//!
//! Still doesn't read the system clipboard — bootstrap sets
//! `UC_DISABLE_SYSTEM_CLIPBOARD=1` so non-bundled CLI runs don't panic
//! in `+[NSPasteboard generalPasteboard]`. The daemon owns the OS
//! clipboard in production.

use std::io::Read;

use serde::Serialize;

use uc_application::facade::space_setup::TryResumeSessionError;
use uc_application::facade::{ClipboardSyncError, DispatchEntryOutcome, DispatchEntryPerTarget};
use uc_core::ids::{FormatId, RepresentationId};
use uc_core::ports::DispatchAck;
use uc_core::{
    ClipboardChangeOrigin, MimeType, ObservedClipboardRepresentation, SystemClipboardSnapshot,
};

use crate::commands::slice1_common::{build_assembly, refuse_if_daemon_running};
use crate::exit_codes;
use crate::ui;

pub struct SendArgs {
    /// Plaintext to dispatch. When `None`, the command reads from stdin
    /// until EOF — handy for `echo hi | uniclipboard-cli send` and the
    /// dual-profile test recipe.
    pub text: Option<String>,
}

pub async fn run(args: SendArgs, json: bool, verbose: bool) -> i32 {
    if !json {
        ui::header("Send clipboard");
    }

    if let Err(code) = refuse_if_daemon_running().await {
        return code;
    }

    let plaintext = match read_plaintext(args.text) {
        Ok(text) if text.is_empty() => {
            ui::error("Empty plaintext — nothing to send.");
            return exit_codes::EXIT_ERROR;
        }
        Ok(text) => text,
        Err(msg) => {
            ui::error(&msg);
            return exit_codes::EXIT_ERROR;
        }
    };

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

    // Refresh presence so `dispatch_entry`'s Online-only filter sees the
    // current peer state instead of stale `Unknown`. Same pattern as the
    // `members` command.
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
            &format!("Probe round failed: {err} (proceeding with last-known state)"),
        ),
    }

    // Phase 3 · T9:wrap the CLI text into a single-representation
    // `SystemClipboardSnapshot` with `text/plain` mime — same shape the
    // daemon would emit if the user copied this text through a real
    // `SystemClipboardPort`. `dispatch_snapshot` handles V3 envelope
    // encoding + canonical `snapshot_hash` (which matches the receiver's
    // local `clipboard_event.snapshot_hash` for dedup).
    let snapshot = SystemClipboardSnapshot {
        ts_ms: chrono::Utc::now().timestamp_millis(),
        representations: vec![ObservedClipboardRepresentation::new(
            RepresentationId::new(),
            FormatId::from("text"),
            Some(MimeType("text/plain".to_string())),
            plaintext.into_bytes(),
        )],
    };

    let dispatch_spinner = ui::spinner("Dispatching to online peers...");
    let outcome = assembly
        .clipboard_sync
        .dispatch_snapshot(snapshot, ClipboardChangeOrigin::LocalCapture)
        .await;

    let outcome = match outcome {
        Ok(o) => {
            ui::spinner_finish_success(
                &dispatch_spinner,
                &format!(
                    "{} accepted, {} duplicate, {} offline, {} error(s)",
                    o.total_accepted, o.total_duplicate, o.total_offline, o.total_errored
                ),
            );
            o
        }
        Err(err) => {
            ui::spinner_finish_error(&dispatch_spinner, &render_dispatch_error(&err));
            assembly.shutdown().await;
            return exit_codes::EXIT_ERROR;
        }
    };

    if json {
        let dto = SendOutcomeDto::from_outcome(&outcome);
        match serde_json::to_string_pretty(&dto) {
            Ok(s) => println!("{s}"),
            Err(err) => {
                ui::error(&format!("Failed to serialize outcome: {err}"));
                assembly.shutdown().await;
                return exit_codes::EXIT_ERROR;
            }
        }
    } else {
        render_human(&outcome);
    }

    assembly.shutdown().await;
    if outcome.total_accepted == 0 && outcome.total_duplicate == 0 {
        // Nothing actually landed: at least one peer was online but every
        // attempt errored (or no peers paired at all). Surface as non-zero
        // so dual-profile shell harness can detect failure.
        exit_codes::EXIT_ERROR
    } else {
        exit_codes::EXIT_SUCCESS
    }
}

fn read_plaintext(arg: Option<String>) -> Result<String, String> {
    if let Some(text) = arg {
        return Ok(text);
    }
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .map_err(|err| format!("read stdin failed: {err}"))?;
    // Trim a single trailing newline so `echo foo | send` matches `send foo`.
    if buf.ends_with('\n') {
        buf.pop();
        if buf.ends_with('\r') {
            buf.pop();
        }
    }
    Ok(buf)
}

fn render_dispatch_error(err: &ClipboardSyncError) -> String {
    match err {
        ClipboardSyncError::LockedSpace => {
            "Space is locked — unlock or re-init before sending.".to_string()
        }
        ClipboardSyncError::CipherFailure(msg) => format!("Encryption failed: {msg}"),
        ClipboardSyncError::Repository(msg) => format!("Peer address lookup failed: {msg}"),
        ClipboardSyncError::LocalIdentity(msg) => format!("Local identity unavailable: {msg}"),
    }
}

fn render_human(outcome: &DispatchEntryOutcome) {
    ui::bar();
    ui::info("hash", short_hash(&outcome.content_hash));
    if outcome.per_target.is_empty() {
        ui::info("targets", "(none — no online peers)");
    } else {
        for entry in &outcome.per_target {
            let line = format!(
                "{} → {}",
                entry.device_id.as_str(),
                render_per_target(entry),
            );
            ui::info("·", &line);
        }
    }
    ui::bar();
}

fn render_per_target(entry: &DispatchEntryPerTarget) -> String {
    match &entry.outcome {
        Ok(DispatchAck::Accepted) => "accepted".to_string(),
        Ok(DispatchAck::DuplicateIgnored) => "duplicate (peer already had it)".to_string(),
        Err(reason) => format!("failed: {reason}"),
    }
}

fn short_hash(hash: &str) -> &str {
    if hash.len() > 16 {
        &hash[..16]
    } else {
        hash
    }
}

#[derive(Serialize)]
struct SendOutcomeDto<'a> {
    content_hash: &'a str,
    total_accepted: usize,
    total_duplicate: usize,
    total_offline: usize,
    total_errored: usize,
    at_ms: i64,
    per_target: Vec<PerTargetDto<'a>>,
}

#[derive(Serialize)]
struct PerTargetDto<'a> {
    device_id: &'a str,
    /// `accepted` | `duplicate` | `error`.
    outcome: &'static str,
    /// Populated only when `outcome == "error"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a str>,
}

impl<'a> SendOutcomeDto<'a> {
    fn from_outcome(o: &'a DispatchEntryOutcome) -> Self {
        Self {
            content_hash: &o.content_hash,
            total_accepted: o.total_accepted,
            total_duplicate: o.total_duplicate,
            total_offline: o.total_offline,
            total_errored: o.total_errored,
            at_ms: o.at_ms,
            per_target: o
                .per_target
                .iter()
                .map(|p| match &p.outcome {
                    Ok(DispatchAck::Accepted) => PerTargetDto {
                        device_id: p.device_id.as_str(),
                        outcome: "accepted",
                        error: None,
                    },
                    Ok(DispatchAck::DuplicateIgnored) => PerTargetDto {
                        device_id: p.device_id.as_str(),
                        outcome: "duplicate",
                        error: None,
                    },
                    Err(msg) => PerTargetDto {
                        device_id: p.device_id.as_str(),
                        outcome: "error",
                        error: Some(msg.as_str()),
                    },
                })
                .collect(),
        }
    }
}
