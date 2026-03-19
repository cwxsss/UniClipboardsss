//! Devices command -- lists paired devices via direct bootstrap (no daemon required).

use serde::Serialize;
use std::fmt;

use crate::exit_codes;
use crate::output;

#[derive(Serialize)]
struct DeviceInfo {
    peer_id: String,
    name: String,
    pairing_state: String,
    identity_fingerprint: String,
}

#[derive(Serialize)]
struct DeviceListOutput {
    devices: Vec<DeviceInfo>,
    count: usize,
}

impl fmt::Display for DeviceListOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.devices.is_empty() {
            return write!(f, "No paired devices found.");
        }
        writeln!(f, "Paired devices: {}", self.count)?;
        for d in &self.devices {
            writeln!(f, "  {} (id: {})", d.name, d.peer_id)?;
        }
        Ok(())
    }
}

/// Run the devices command.
///
/// Uses `build_cli_runtime()` to query the device list directly from the
/// database without requiring the daemon to be running.
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

    let usecases = uc_app::usecases::CoreUseCases::new(&runtime);
    let snapshot = match usecases.get_p2p_peers_snapshot().execute().await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: failed to get p2p peers snapshot: {}", e);
            return exit_codes::EXIT_ERROR;
        }
    };

    // Filter to paired devices only, preserve original CLI output behavior
    let device_infos: Vec<DeviceInfo> = snapshot
        .into_iter()
        .filter(|p| p.is_paired)
        .map(|p| DeviceInfo {
            peer_id: p.peer_id,
            name: p.device_name.unwrap_or_else(|| "Unknown".to_string()),
            pairing_state: p.pairing_state,
            identity_fingerprint: p.identity_fingerprint,
        })
        .collect();

    let result = DeviceListOutput {
        count: device_infos.len(),
        devices: device_infos,
    };

    if let Err(e) = output::print_result(&result, json) {
        eprintln!("Error: {}", e);
        return exit_codes::EXIT_ERROR;
    }

    exit_codes::EXIT_SUCCESS
}
