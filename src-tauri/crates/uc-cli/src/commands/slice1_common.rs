//! Shared bootstrap helpers for Slice 1 CLI commands (init/invite/join).
//!
//! Each command runs a self-contained [`uc_bootstrap::SpaceSetupAssembly`]
//! (no daemon in the loop), so these helpers centralize the daemon-probe
//! refusal, log-profile selection, and hostname-derived device-name default.

use std::sync::Arc;

use uc_bootstrap::{
    build_slice1_cli_context, build_space_setup_assembly, IrohNodeConfig, SpaceSetupAssembly,
};
use uc_core::ports::SettingsPort;

use crate::exit_codes;
use crate::local_daemon::probe_running;
use crate::ui;

/// Bundle returned by [`build_assembly`]. Exposes `settings` alongside the
/// assembly so commands that need to write to `Settings.general.device_name`
/// (join, in particular, because B2 reads it back from disk) don't have to
/// rebuild the whole wiring.
pub struct Slice1Cli {
    pub assembly: SpaceSetupAssembly,
    pub settings: Arc<dyn SettingsPort>,
}

/// Refuse to run when a daemon is already serving this profile.
///
/// Rationale: until IPC redirect lands, two processes on the same
/// profile would bind two iroh endpoints against the same Ed25519 secret
/// (rendezvous self-collision) and the daemon's own setup flow would
/// race the CLI. The CLI's purpose is local end-to-end testing, so we
/// simply refuse and tell the user to `stop` the daemon first.
pub async fn refuse_if_daemon_running() -> Result<(), i32> {
    match probe_running().await {
        Ok(true) => {
            ui::error(
                "A daemon is already running for this profile. Stop it first with \
                 `uniclipboard-cli stop`, or rerun under a different --profile.",
            );
            Err(exit_codes::EXIT_DAEMON_UNREACHABLE)
        }
        Ok(false) => Ok(()),
        // Probe-network errors: fall through (no daemon to conflict with).
        Err(err) => {
            tracing::debug!(error = %err, "daemon probe failed; assuming no daemon");
            Ok(())
        }
    }
}

/// Build the Slice 1 `SpaceSetupAssembly` for a CLI command.
///
/// Uses the `Cli` log profile unless `verbose` is set, in which case it
/// switches to `Dev` so tracing lands on stderr — handy when debugging a
/// single-machine two-process pairing run.
///
/// Sets `UC_DISABLE_SYSTEM_CLIPBOARD=1` before wiring so the bootstrap
/// substitutes a no-op clipboard adapter. Slice 1 init/invite/join never
/// touch the clipboard; the real adapter (`clipboard-rs`) eagerly calls
/// `+[NSPasteboard generalPasteboard]`, which returns NULL and panics in
/// any non-bundled CLI context on macOS.
pub async fn build_assembly(verbose: bool) -> Result<Slice1Cli, i32> {
    // Must land before `build_slice1_cli_context` fires `wire_dependencies`.
    std::env::set_var("UC_DISABLE_SYSTEM_CLIPBOARD", "1");

    let log_profile = if verbose {
        Some(uc_observability::LogProfile::Dev)
    } else {
        Some(uc_observability::LogProfile::Cli)
    };
    let (_config, wired) = match build_slice1_cli_context(log_profile) {
        Ok(ctx) => ctx,
        Err(err) => {
            ui::error(&format!("Failed to wire dependencies: {err}"));
            return Err(exit_codes::EXIT_ERROR);
        }
    };
    let settings = Arc::clone(&wired.deps.settings);
    match build_space_setup_assembly(&wired, IrohNodeConfig::default()).await {
        Ok(assembly) => Ok(Slice1Cli { assembly, settings }),
        Err(err) => {
            ui::error(&format!("Failed to bind iroh endpoint: {err}"));
            Err(exit_codes::EXIT_ERROR)
        }
    }
}

/// Default device name derived from the OS hostname, with the current
/// `UC_PROFILE` appended when set so two single-machine instances show
/// up distinctly in the peer's UI. Returns `None` if `hostname::get`
/// fails and the value cannot be coerced to UTF-8.
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
