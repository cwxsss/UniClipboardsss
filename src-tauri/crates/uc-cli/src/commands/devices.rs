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
