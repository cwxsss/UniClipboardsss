//! Status command -- queries daemon runtime status over HTTP via `GET /status`.

use crate::daemon_client::{DaemonClientError, DaemonHttpClient};
use crate::exit_codes;
use uc_daemon::api::types::{StatusResponse, WorkerStatusDto};

/// Format an uptime duration in human-readable form.
///
/// Examples: "45s", "2m 15s", "2h 15m", "1d 3h".
fn format_uptime(seconds: u64) -> String {
    if seconds < 60 {
        return format!("{}s", seconds);
    }
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{}d", days));
    }
    if hours > 0 {
        parts.push(format!("{}h", hours));
    }
    if minutes > 0 {
        parts.push(format!("{}m", minutes));
    }
    if secs > 0 && days == 0 && hours == 0 {
        parts.push(format!("{}s", secs));
    }

    parts.join(" ")
}

/// Run the status command.
pub async fn run(json: bool, _verbose: bool) -> i32 {
    let client = match DaemonHttpClient::new() {
        Ok(client) => client,
        Err(error) => return print_client_error(error),
    };

    let status = match client.get_status().await {
        Ok(status) => status,
        Err(error) => return print_client_error(error),
    };

    let output = if json {
        match serde_json::to_string_pretty(&status) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("Error: failed to serialize status: {error}");
                return exit_codes::EXIT_ERROR;
            }
        }
    } else {
        render_status_output(&status)
    };

    println!("{output}");
    exit_codes::EXIT_SUCCESS
}

fn render_status_output(status: &StatusResponse) -> String {
    let healthy_count = status
        .workers
        .iter()
        .filter(|worker| worker.health == "healthy")
        .count();
    let total_count = status.workers.len();

    let mut lines = vec![
        "Status: running".to_string(),
        format!("Uptime: {}", format_uptime(status.uptime_seconds)),
        format!("Version: {}", status.package_version),
        format!("API revision: {}", status.api_revision),
        format!("Workers: {healthy_count}/{total_count} healthy"),
    ];

    lines.extend(status.workers.iter().map(render_worker_line));
    lines.push(format!("Connected peers: {}", status.connected_peers));

    lines.join("\n")
}

fn render_worker_line(worker: &WorkerStatusDto) -> String {
    format!("  {}: {}", worker.name, worker.health)
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
    fn unreachable_maps_to_daemon_unreachable_exit_code() {
        let code = print_client_error(DaemonClientError::Unreachable(anyhow::anyhow!(
            "connection refused"
        )));
        assert_eq!(code, exit_codes::EXIT_DAEMON_UNREACHABLE);
    }

    #[test]
    fn renders_human_output_from_http_fixture() {
        let status = StatusResponse {
            package_version: "0.1.0".to_string(),
            api_revision: "v1".to_string(),
            uptime_seconds: 3723,
            workers: vec![
                WorkerStatusDto {
                    name: "network".to_string(),
                    health: "healthy".to_string(),
                },
                WorkerStatusDto {
                    name: "sync".to_string(),
                    health: "degraded (retrying)".to_string(),
                },
            ],
            connected_peers: 2,
        };

        let rendered = render_status_output(&status);

        assert_eq!(
            rendered,
            [
                "Status: running",
                "Uptime: 1h 2m",
                "Version: 0.1.0",
                "API revision: v1",
                "Workers: 1/2 healthy",
                "  network: healthy",
                "  sync: degraded (retrying)",
                "Connected peers: 2",
            ]
            .join("\n")
        );
    }

    #[test]
    fn json_output_serializes_daemon_status_dto() {
        let status = StatusResponse {
            package_version: "0.1.0".to_string(),
            api_revision: "v1".to_string(),
            uptime_seconds: 10,
            workers: vec![WorkerStatusDto {
                name: "network".to_string(),
                health: "healthy".to_string(),
            }],
            connected_peers: 1,
        };

        let value = serde_json::to_value(&status).unwrap();
        assert_eq!(value["packageVersion"], "0.1.0");
        assert_eq!(value["apiRevision"], "v1");
        assert_eq!(value["uptimeSeconds"], 10);
        assert_eq!(value["workers"][0]["name"], "network");
        assert!(value.get("uptime_seconds").is_none());
    }
}
