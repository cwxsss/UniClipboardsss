//! `uniclipboard-cli join` — joiner side of Slice 1 pairing.
//!
//! Takes an invitation code and passphrase, then drives
//! [`SpaceSetupFacade::redeem_pairing_invitation`] to completion.
//! Unlike [`invite`](super::invite), this command is a single blocking
//! RPC — B2 owns its own dial/wait loop internally, so we simply await
//! the result (with Ctrl+C handling to guarantee clean iroh teardown).

use tokio::select;
use tokio::signal;

use uc_application::facade::space_setup::{
    RedeemPairingInvitationCommand, RedeemPairingInvitationError,
};
use uc_core::crypto::domain::Passphrase;
use uc_core::pairing::InvitationCode;

use crate::commands::slice1_common::{
    build_assembly, default_device_name, refuse_if_daemon_running,
};
use crate::exit_codes;
use crate::ui;

const EXIT_SIGINT: i32 = 130;

pub struct JoinArgs {
    pub code: Option<String>,
    pub passphrase: Option<String>,
    pub device_name: Option<String>,
}

pub async fn run(args: JoinArgs, verbose: bool) -> i32 {
    ui::header("Join a space");

    if let Err(code) = refuse_if_daemon_running().await {
        return code;
    }

    let bundle = match build_assembly(verbose).await {
        Ok(b) => b,
        Err(code) => return code,
    };

    let code_str = match args.code {
        Some(c) if !c.trim().is_empty() => c.trim().to_string(),
        Some(_) => {
            ui::error("--code is empty");
            bundle.assembly.shutdown().await;
            return exit_codes::EXIT_ERROR;
        }
        None => match ui::password("Invitation code") {
            Ok(c) if !c.trim().is_empty() => c.trim().to_string(),
            Ok(_) => {
                ui::error("Invitation code cannot be empty");
                bundle.assembly.shutdown().await;
                return exit_codes::EXIT_ERROR;
            }
            Err(e) => {
                ui::error(&e);
                bundle.assembly.shutdown().await;
                return exit_codes::EXIT_ERROR;
            }
        },
    };

    let passphrase_str = match args.passphrase {
        Some(p) if !p.trim().is_empty() => p,
        Some(_) => {
            ui::error("--passphrase is empty");
            bundle.assembly.shutdown().await;
            return exit_codes::EXIT_ERROR;
        }
        None => match ui::password("Space passphrase") {
            Ok(p) if !p.trim().is_empty() => p,
            Ok(_) => {
                ui::error("Passphrase cannot be empty");
                bundle.assembly.shutdown().await;
                return exit_codes::EXIT_ERROR;
            }
            Err(e) => {
                ui::error(&e);
                bundle.assembly.shutdown().await;
                return exit_codes::EXIT_ERROR;
            }
        },
    };

    // B2's use case reads `Settings.general.device_name` from disk rather
    // than taking it in the command, so if this is a brand-new profile
    // the setting will be absent and `redeem` fails with
    // `DeviceNameRequired`. Mirror the init command's behaviour:
    // `--device-name` overrides, otherwise default to the OS hostname.
    // Persist to settings before dialing.
    let device_name = args
        .device_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .or_else(default_device_name);
    let device_name = match device_name {
        Some(n) => n,
        None => {
            ui::error("device name is required (pass --device-name or set a system hostname)");
            bundle.assembly.shutdown().await;
            return exit_codes::EXIT_ERROR;
        }
    };
    if let Err(err) = persist_device_name(&bundle.settings, &device_name).await {
        ui::error(&format!("failed to persist device_name: {err}"));
        bundle.assembly.shutdown().await;
        return exit_codes::EXIT_ERROR;
    }

    let cmd = RedeemPairingInvitationCommand {
        code: InvitationCode::new(&code_str),
        passphrase: Passphrase::new(&passphrase_str),
    };

    let spinner = ui::spinner("Dialing sponsor and running handshake...");

    // Clone the Arc so the in-flight future does not borrow `bundle`
    // — otherwise `bundle.assembly.shutdown().await` below can't take
    // ownership.
    let facade = std::sync::Arc::clone(&bundle.assembly.facade);
    let redeem = async move { facade.redeem_pairing_invitation(cmd).await };
    tokio::pin!(redeem);

    let exit = select! {
        result = &mut redeem => match result {
            Ok(out) => {
                ui::spinner_finish_success(&spinner, "Joined space");
                ui::info("space_id", out.space_id.as_str());
                ui::info("self_device_id", out.self_device_id.as_str());
                ui::info("self_device_name", &device_name);
                ui::info("self_fingerprint", &out.self_identity_fingerprint.to_string());
                ui::info("sponsor_device_id", out.sponsor_device_id.as_str());
                ui::info("sponsor_fingerprint", &out.sponsor_identity_fingerprint.to_string());
                exit_codes::EXIT_SUCCESS
            }
            Err(err) => {
                let hint = match &err {
                    RedeemPairingInvitationError::InvitationNotFound => {
                        "Double-check the code — sponsor may have let it expire or reissued."
                    }
                    RedeemPairingInvitationError::InvitationExpired => {
                        "Ask the sponsor to run `invite` again to issue a fresh code."
                    }
                    RedeemPairingInvitationError::PassphraseMismatch => {
                        "Passphrase did not match the sponsor's. Retry `join`."
                    }
                    RedeemPairingInvitationError::SponsorUnreachable => {
                        "Sponsor is online in rendezvous but could not be reached. Check NAT / relay."
                    }
                    RedeemPairingInvitationError::ServiceUnavailable => {
                        "Rendezvous service is unreachable."
                    }
                    _ => "",
                };
                ui::spinner_finish_error(&spinner, &format!("Join failed: {err}"));
                if !hint.is_empty() {
                    ui::info("hint", hint);
                }
                exit_codes::EXIT_ERROR
            }
        },
        _ = signal::ctrl_c() => {
            ui::spinner_finish_error(&spinner, "Interrupted by user");
            EXIT_SIGINT
        }
    };

    bundle.assembly.shutdown().await;
    exit
}

async fn persist_device_name(
    settings: &std::sync::Arc<dyn uc_core::ports::SettingsPort>,
    device_name: &str,
) -> anyhow::Result<()> {
    let mut current = settings.load().await?;
    // Only write if changed — keeps the join command idempotent across
    // reruns on the same profile.
    if current.general.device_name.as_deref() != Some(device_name) {
        current.general.device_name = Some(device_name.to_string());
        settings.save(&current).await?;
    }
    Ok(())
}
