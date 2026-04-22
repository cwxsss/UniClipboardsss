//! `uniclipboard-cli send` — one-shot clipboard dispatch (Slice 2 Phase 2 · T11).
//!
//! Self-contained direct-mode command (no daemon), mirroring the `init` /
//! `invite` / `join` / `members` pattern. Builds a `SpaceSetupAssembly`,
//! resumes the cached session, refreshes presence so dispatch can route to
//! online peers, then encodes the user-supplied text as a Phase 2 payload
//! and fans it out via `ClipboardSyncFacade::dispatch_entry`.
//!
//! Phase 2 deliberately takes the plaintext from the CLI (positional arg
//! or stdin) instead of reading the system clipboard — the bootstrap sets
//! `UC_DISABLE_SYSTEM_CLIPBOARD=1` so non-bundled CLI runs don't panic in
//! `+[NSPasteboard generalPasteboard]`. Daemon-driven system-clipboard
//! capture lands in Phase 3.

use std::io::Read;

use bytes::Bytes;
use serde::Serialize;
use sha2::{Digest, Sha256};

use uc_application::facade::space_setup::TryResumeSessionError;
use uc_application::facade::{
    ClipboardSyncError, DispatchEntryInput, DispatchEntryOutcome, DispatchEntryPerTarget,
};
use uc_core::ports::DispatchAck;

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

    let plaintext_bytes = Bytes::from(plaintext.into_bytes());
    let content_hash = sha256_hex(&plaintext_bytes);

    let dispatch_spinner = ui::spinner("Dispatching to online peers...");
    let outcome = assembly
        .clipboard_sync
        .dispatch_entry(DispatchEntryInput {
            plaintext: plaintext_bytes,
            content_hash: content_hash.clone(),
            // Phase 2 sends raw text bytes; payload_version reserved for a
            // future inner-codec version bump (V3 ClipboardBinaryPayload
            // when daemon takes over capture).
            payload_version: 3,
        })
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

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    hex_lower(&digest)
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0F) as usize] as char);
    }
    out
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
