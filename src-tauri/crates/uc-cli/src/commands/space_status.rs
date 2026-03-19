//! Space status command -- shows encryption state via direct bootstrap (no daemon required).

use serde::Serialize;
use std::fmt;

use crate::exit_codes;
use crate::output;

#[derive(Serialize)]
struct SpaceStatusOutput {
    encryption_ready: bool,
    encryption_state: String,
}

impl fmt::Display for SpaceStatusOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ready_str = if self.encryption_ready { "yes" } else { "no" };
        writeln!(f, "Encryption ready: {}", ready_str)?;
        write!(f, "Encryption state: {}", self.encryption_state)?;
        Ok(())
    }
}

/// Run the space-status command.
///
/// Uses `build_cli_runtime()` to query encryption state directly without
/// requiring the daemon to be running.
pub async fn run(json: bool, verbose: bool) -> i32 {
    let profile = if verbose {
        Some(uc_observability::LogProfile::Dev)
    } else {
        Some(uc_observability::LogProfile::Cli)
    };

    let runtime = match uc_bootstrap::build_cli_runtime(profile) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: failed to build CLI runtime: {}", e);
            return exit_codes::EXIT_ERROR;
        }
    };

    let encryption_ready = runtime.is_encryption_ready().await;
    let encryption_state = match runtime.encryption_state().await {
        Ok(state) => format!("{:?}", state),
        Err(e) => {
            eprintln!("Error: failed to query encryption state: {}", e);
            return exit_codes::EXIT_ERROR;
        }
    };

    let result = SpaceStatusOutput {
        encryption_ready,
        encryption_state,
    };

    if let Err(e) = output::print_result(&result, json) {
        eprintln!("Error: {}", e);
        return exit_codes::EXIT_ERROR;
    }

    exit_codes::EXIT_SUCCESS
}
