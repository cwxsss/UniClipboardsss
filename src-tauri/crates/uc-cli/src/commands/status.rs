//! Status command -- queries daemon runtime status over HTTP via `GET /status`.

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
    let ctx = match uc_daemon_client::DaemonClientContext::from_env() {
        Ok(ctx) => ctx,
        Err(error) => {
            eprintln!("Error: failed to connect to daemon: {error}");
            return exit_codes::EXIT_DAEMON_UNREACHABLE;
        }
    };

    let status = match ctx.query_client().get_status().await {
        Ok(status) => status,
        Err(error) => {
            eprintln!("Error: failed to get daemon status: {error}");
            return exit_codes::EXIT_ERROR;
        }
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
