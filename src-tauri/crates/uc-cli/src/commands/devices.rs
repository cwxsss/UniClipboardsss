//! Devices command -- lists paired devices via daemon HTTP API via `GET /paired-devices`.

use crate::daemon_client::{DaemonClientError, DaemonHttpClient};
use crate::exit_codes;
use uc_daemon::api::types::PairedDeviceDto;

/// Run the devices command.
///
pub async fn run(json: bool, verbose: bool) -> i32 {
    let _ = verbose;

    let client = match DaemonHttpClient::new() {
        Ok(client) => client,
        Err(error) => return print_client_error(error),
    };

    let devices = match client.get_paired_devices().await {
        Ok(devices) => devices,
        Err(error) => return print_client_error(error),
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

fn print_client_error(error: DaemonClientError) -> i32 {
    match error {
        DaemonClientError::Unreachable(_) => {
            eprintln!("Error: daemon unreachable (is uniclipboard-daemon running?)");
            exit_codes::EXIT_DAEMON_UNREACHABLE
        }
        DaemonClientError::Unauthorized => {
            eprintln!("Error: daemon rejected request: invalid or missing auth token");
            exit_codes::EXIT_ERROR
        }
        DaemonClientError::Initialization(_)
        | DaemonClientError::UnexpectedStatus { .. }
        | DaemonClientError::InvalidResponse(_) => {
            eprintln!("Error: {error}");
            exit_codes::EXIT_ERROR
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_devices_from_http_fixture() {
        let devices = vec![
            PairedDeviceDto {
                peer_id: "peer-a".to_string(),
                device_name: "Alice Mac".to_string(),
                pairing_state: "Paired".to_string(),
                last_seen_at_ms: None,
                connected: true,
            },
            PairedDeviceDto {
                peer_id: "peer-b".to_string(),
                device_name: "Bob PC".to_string(),
                pairing_state: "Paired".to_string(),
                last_seen_at_ms: Some(42),
                connected: false,
            },
        ];

        let rendered = render_devices_output(&devices);

        assert_eq!(
            rendered,
            [
                "Paired devices: 2",
                "  Alice Mac (id: peer-a)",
                "  Bob PC (id: peer-b)",
            ]
            .join("\n")
        );
    }

    #[test]
    fn json_output_serializes_daemon_device_dtos() {
        let devices = vec![PairedDeviceDto {
            peer_id: "peer-a".to_string(),
            device_name: "Alice Mac".to_string(),
            pairing_state: "Paired".to_string(),
            last_seen_at_ms: Some(7),
            connected: true,
        }];

        let value = serde_json::to_value(&devices).unwrap();
        assert_eq!(value[0]["peerId"], "peer-a");
        assert_eq!(value[0]["deviceName"], "Alice Mac");
        assert!(value[0].get("peer_id").is_none());
    }
}
