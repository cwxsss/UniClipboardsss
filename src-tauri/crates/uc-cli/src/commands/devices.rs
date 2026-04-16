//! Devices command -- lists paired devices via daemon HTTP API via `GET /paired-devices`.

use crate::exit_codes;
use uc_daemon::api::types::PairedDeviceDto;
use uc_daemon_client::DaemonClientContext;

/// Run the devices command.
///
pub async fn run(json: bool, verbose: bool) -> i32 {
    let _ = verbose;

    let ctx = match DaemonClientContext::from_env() {
        Ok(ctx) => ctx,
        Err(error) => {
            eprintln!("Error: failed to connect to daemon: {error}");
            return exit_codes::EXIT_DAEMON_UNREACHABLE;
        }
    };

    let devices = match ctx.query_client().get_paired_devices().await {
        Ok(devices) => devices,
        Err(error) => {
            eprintln!("Error: failed to get paired devices: {error}");
            return exit_codes::EXIT_ERROR;
        }
    };

    if json {
        match serde_json::to_string_pretty(&devices) {
            Ok(value) => println!("{value}"),
            Err(error) => {
                eprintln!("Error: failed to serialize paired devices: {error}");
                return exit_codes::EXIT_ERROR;
            }
        }
    } else {
        println!("{}", render_devices_output(&devices));
    }

    exit_codes::EXIT_SUCCESS
}

fn render_devices_output(devices: &[PairedDeviceDto]) -> String {
    let mut lines = vec![format!("Paired devices: {}", devices.len())];
    lines.extend(
        devices
            .iter()
            .map(|device| format!("  {} (id: {})", device.device_name, device.peer_id)),
    );
    lines.join("\n")
}
