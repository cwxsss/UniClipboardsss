//! Space status command -- shows encryption state via direct bootstrap (no daemon required).

use serde::Serialize;
use std::fmt;

use crate::exit_codes;
use crate::output;

#[derive(Serialize)]
struct SpaceStatusOutput {
    encryption_ready: bool,
    setup_completed: bool,
}

impl fmt::Display for SpaceStatusOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ready_str = if self.encryption_ready { "yes" } else { "no" };
        let setup_str = if self.setup_completed { "yes" } else { "no" };
        writeln!(f, "Encryption ready: {}", ready_str)?;
        write!(f, "Setup completed: {}", setup_str)?;
        Ok(())
    }
}

/// Run the space-status command.
///
/// Uses `build_cli_app_facade()` to query encryption state directly without
/// requiring the daemon to be running.
pub async fn run(json: bool, verbose: bool) -> i32 {
    let profile = if verbose {
        Some(uc_observability::LogProfile::Dev)
    } else {
        Some(uc_observability::LogProfile::Cli)
    };

    let app_facade = match uc_bootstrap::build_cli_app_facade(profile) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error: failed to build CLI runtime: {}", e);
            return exit_codes::EXIT_ERROR;
        }
    };

    let state = match app_facade.encryption.state().await {
        Ok(state) => state,
        Err(e) => {
            eprintln!("Error: failed to query setup status: {}", e);
            return exit_codes::EXIT_ERROR;
        }
    };

    let result = SpaceStatusOutput {
        encryption_ready: state.session_ready,
        setup_completed: state.initialized,
    };

    if let Err(e) = output::print_result(&result, json) {
        eprintln!("Error: {}", e);
        return exit_codes::EXIT_ERROR;
    }

    exit_codes::EXIT_SUCCESS
}
