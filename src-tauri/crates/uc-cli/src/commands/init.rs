//! `uniclip init` — Slice 1 A1 on this profile.
//!
//! Prompts for (or accepts `--passphrase`) + optional `--device-name`,
//! then drives [`SpaceSetupFacade::initialize_space`]. Refuses when a
//! daemon is already claiming this profile's socket.

use uc_application::facade::space_setup::{InitializeSpaceError, InitializeSpaceInput};

use crate::commands::app_session::{
    build_app_session, default_device_name, refuse_if_daemon_running,
};
use crate::exit_codes;
use crate::ui;

pub struct InitArgs {
    pub passphrase: Option<String>,
    pub device_name: Option<String>,
}

pub async fn run(args: InitArgs, verbose: bool) -> i32 {
    ui::header("Initialize space");

    if let Err(code) = refuse_if_daemon_running().await {
        return code;
    }

    let cli = match build_app_session(verbose).await {
        Ok(bundle) => bundle,
        Err(code) => return code,
    };

    // Collect passphrase: --passphrase wins; otherwise prompt with
    // confirmation. Empty strings are always rejected — the facade
    // would map that to `PassphraseTooShort`, but catching it here
    // gives a cleaner error line.
    let passphrase_str = match args.passphrase {
        Some(ref p) if p.trim().is_empty() => {
            ui::error("--passphrase is empty");
            cli.shutdown().await;
            return exit_codes::EXIT_ERROR;
        }
        Some(p) => p,
        None => match ui::password_with_confirm("New space passphrase", "Confirm passphrase") {
            Ok(p) if p.trim().is_empty() => {
                ui::error("Passphrase cannot be empty");
                cli.shutdown().await;
                return exit_codes::EXIT_ERROR;
            }
            Ok(p) => p,
            Err(e) => {
                ui::error(&e);
                cli.shutdown().await;
                return exit_codes::EXIT_ERROR;
            }
        },
    };

    let device_name = args.device_name.or_else(default_device_name);

    let input = InitializeSpaceInput {
        passphrase: passphrase_str.clone(),
        // A1 requires both fields; when the user supplied `--passphrase`
        // we take that as their confirmation too.
        passphrase_confirm: passphrase_str,
        device_name,
    };

    let spinner = ui::spinner("Creating encrypted space...");
    let exit = match cli.app_facade().initialize_space(input).await {
        Ok(result) => {
            ui::spinner_finish_success(&spinner, "Space initialized");
            ui::info("space_id", result.space_id.as_str());
            ui::info("device_id", result.self_device_id.as_str());
            ui::info("fingerprint", &result.fingerprint.to_string());
            exit_codes::EXIT_SUCCESS
        }
        Err(err) => {
            let msg = match &err {
                InitializeSpaceError::AlreadySetup => {
                    "This profile is already initialized — run `invite` instead.".to_string()
                }
                InitializeSpaceError::PassphraseMismatch => {
                    "Passphrase and confirmation do not match.".to_string()
                }
                other => format!("{other}"),
            };
            ui::spinner_finish_error(&spinner, &msg);
            exit_codes::EXIT_ERROR
        }
    };

    cli.shutdown().await;
    exit
}
