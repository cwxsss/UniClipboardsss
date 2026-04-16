//! Search command -- exposes `search query`, `search status`, and `search rebuild`
//! subcommands that forward the full daemon filter surface through `SearchQueryRequest`.

use std::future::Future;

use clap::Subcommand;
use indicatif::ProgressBar;
use tokio::time::{sleep, Duration};

use crate::exit_codes;
use uc_daemon::api::dto::search::{SearchRebuildAcceptedResponse, SearchStatusResponse};
use uc_daemon_client::{DaemonClientContext, DaemonSearchRequestError, SearchQueryRequest};

/// Subcommands for the grouped `search` CLI command.
#[derive(Subcommand, Debug)]
pub enum SearchCommands {
    /// Query the search index
    Query {
        /// Free-text query string (inline AND/OR are forwarded verbatim to daemon)
        query: String,
        /// Boolean operator: "and" or "or"
        #[arg(long)]
        operator: Option<String>,
        /// Time preset: today, yesterday, last_7d, last_30d
        #[arg(long = "time-preset")]
        time_preset: Option<String>,
        /// Start of absolute time range (milliseconds since epoch)
        #[arg(long = "from-ms")]
        from_ms: Option<i64>,
        /// End of absolute time range (milliseconds since epoch)
        #[arg(long = "to-ms")]
        to_ms: Option<i64>,
        /// Filter by content type (text, html, link, file, image, other); repeatable
        #[arg(long = "type")]
        content_types: Vec<String>,
        /// Filter by file extension (e.g. md, txt); repeatable
        #[arg(long = "ext")]
        extensions: Vec<String>,
        /// Maximum results to return
        #[arg(long, default_value_t = 50)]
        limit: u32,
        /// Result offset (for pagination)
        #[arg(long, default_value_t = 0)]
        offset: u32,
        /// Show detailed metadata for each result
        #[arg(long)]
        detailed: bool,
    },
    /// Show search index availability status
    Status,
    /// Trigger a search index rebuild (stays attached by default)
    Rebuild {
        /// Return immediately after the rebuild is accepted instead of following progress
        #[arg(long)]
        no_wait: bool,
    },
}

/// Run the grouped search command.
pub async fn run(subcommand: SearchCommands, json: bool, verbose: bool) -> i32 {
    let _ = verbose;

    let ctx = match DaemonClientContext::from_env() {
        Ok(ctx) => ctx,
        Err(error) => {
            eprintln!("Error: failed to connect to daemon: {error}");
            return exit_codes::EXIT_DAEMON_UNREACHABLE;
        }
    };

    match subcommand {
        SearchCommands::Query {
            query,
            operator,
            time_preset,
            from_ms,
            to_ms,
            content_types,
            extensions,
            limit,
            offset,
            detailed,
        } => {
            // Local flag-shape validation: --from-ms and --to-ms must come in pairs
            match (from_ms, to_ms) {
                (Some(_), None) | (None, Some(_)) => {
                    eprintln!("Error: --from-ms and --to-ms must be provided together");
                    return exit_codes::EXIT_ERROR;
                }
                _ => {}
            }

            let request = SearchQueryRequest {
                query,
                operator,
                time_preset,
                from_ms,
                to_ms,
                content_types,
                extensions,
                limit,
                offset,
            };

            let response = match ctx.search_client().query(request).await {
                Ok(response) => response,
                Err(error) => {
                    eprintln!("Error: failed to query search index: {error}");
                    return exit_codes::EXIT_ERROR;
                }
            };

            if json {
                match serde_json::to_string_pretty(&response) {
                    Ok(value) => println!("{value}"),
                    Err(error) => {
                        eprintln!("Error: failed to serialize search query response: {error}");
                        return exit_codes::EXIT_ERROR;
                    }
                }
            } else {
                println!("{}", render_query_output(&response, detailed));
            }

            exit_codes::EXIT_SUCCESS
        }

        SearchCommands::Status => {
            let response = match ctx.search_client().status().await {
                Ok(response) => response,
                Err(error) => {
                    eprintln!("Error: failed to get search status: {error}");
                    return exit_codes::EXIT_ERROR;
                }
            };

            if json {
                match serde_json::to_string_pretty(&response) {
                    Ok(value) => println!("{value}"),
                    Err(error) => {
                        eprintln!("Error: failed to serialize search status response: {error}");
                        return exit_codes::EXIT_ERROR;
                    }
                }
            } else {
                println!("{}", render_status_output(&response));
            }

            exit_codes::EXIT_SUCCESS
        }

        SearchCommands::Rebuild { no_wait } => run_rebuild(json, no_wait, ctx).await,
    }
}

/// Run the `search rebuild` subcommand using the daemon client from context.
async fn run_rebuild(json: bool, no_wait: bool, ctx: DaemonClientContext) -> i32 {
    let search = ctx.search_client();
    let request_rebuild = move || {
        let s = search.clone();
        async move { s.rebuild().await }
    };
    let search = ctx.search_client();
    let fetch_status = move || {
        let s = search.clone();
        async move { s.status().await }
    };
    run_rebuild_with(request_rebuild, fetch_status, json, no_wait).await
}

/// Run the `search rebuild` subcommand with injected rebuild and status closures.
///
/// This testable variant accepts async closures for both the rebuild request and status
/// polling, allowing unit tests to inject mock sequences without real network calls.
pub async fn run_rebuild_with<RFn, RFut, SFn, SFut>(
    request_rebuild: RFn,
    fetch_status: SFn,
    json: bool,
    no_wait: bool,
) -> i32
where
    RFn: FnOnce() -> RFut,
    RFut: Future<Output = anyhow::Result<SearchRebuildAcceptedResponse>>,
    SFn: Fn() -> SFut + Clone,
    SFut: Future<Output = anyhow::Result<SearchStatusResponse>>,
{
    match request_rebuild().await {
        Ok(accepted) => {
            if no_wait {
                if json {
                    if let Ok(s) = serde_json::to_string(&accepted) {
                        println!("{s}");
                    }
                } else {
                    println!("Search rebuild accepted.");
                }
                return exit_codes::EXIT_SUCCESS;
            }
            // Accepted — enter polling loop.
            wait_for_search_ready(fetch_status, json).await
        }
        Err(error) => {
            // Check if this is a structured daemon error we can inspect.
            if let Some(search_err) = error.downcast_ref::<DaemonSearchRequestError>() {
                let code = search_err.code.as_deref().unwrap_or("");
                if code == "rebuild_already_running" {
                    if !json {
                        eprintln!("Search rebuild already running; following current status.");
                    }
                    return wait_for_search_ready(fetch_status, json).await;
                }
                if code == "session_locked" {
                    if !json {
                        eprintln!("{}", render_rebuild_locked_message());
                    } else {
                        if let Ok(s) = serde_json::to_string(&serde_json::json!({
                            "code": "session_locked",
                            "message": render_rebuild_locked_message()
                        })) {
                            eprintln!("{s}");
                        }
                    }
                    return exit_codes::EXIT_ERROR;
                }
            }
            eprintln!("Error: rebuild request failed: {error}");
            exit_codes::EXIT_ERROR
        }
    }
}

/// Poll `fetch_status` every 500ms until the daemon reports `ready` or `unavailable`.
///
/// Human mode uses a spinner and updates only when the `(state, reason)` pair changes.
/// JSON mode emits one compact JSON object per line for each status snapshot change.
pub async fn wait_for_search_ready<F, Fut>(fetch_status: F, json: bool) -> i32
where
    F: Fn() -> Fut + Clone,
    Fut: Future<Output = anyhow::Result<SearchStatusResponse>>,
{
    let spinner = if json {
        None
    } else {
        let pb = ProgressBar::new_spinner();
        pb.set_message("Search rebuild: starting...");
        Some(pb)
    };

    let mut last_state_reason: Option<(String, Option<String>)> = None;

    loop {
        match fetch_status().await {
            Ok(status) => {
                let state = status.data.state.clone();
                let reason = status.data.reason.clone();
                let pair = (state.clone(), reason.clone());

                if last_state_reason.as_ref() != Some(&pair) {
                    last_state_reason = Some(pair);

                    if json {
                        if let Ok(s) = serde_json::to_string(&status) {
                            println!("{s}");
                        }
                    } else if let Some(ref pb) = spinner {
                        let reason_str = reason.as_deref().unwrap_or("none");
                        pb.set_message(format!("Search rebuild: {state} ({reason_str})"));
                        pb.tick();
                    }
                }

                if state == "ready" || state == "unavailable" {
                    if let Some(pb) = spinner {
                        if state == "ready" {
                            pb.finish_with_message("Search rebuild complete.");
                        } else {
                            let reason_str = status.data.reason.as_deref().unwrap_or("none");
                            pb.finish_with_message(format!(
                                "Search rebuild failed or is still blocked: {reason_str}"
                            ));
                        }
                    }

                    return if state == "ready" {
                        exit_codes::EXIT_SUCCESS
                    } else {
                        exit_codes::EXIT_ERROR
                    };
                }
            }
            Err(error) => {
                if let Some(pb) = spinner {
                    pb.finish_with_message(format!("Error polling search status: {error}"));
                } else {
                    eprintln!("Error polling search status: {error}");
                }
                return exit_codes::EXIT_ERROR;
            }
        }

        sleep(Duration::from_millis(500)).await;
    }
}

/// Returns the actionable message to display when the rebuild is blocked by a locked session.
fn render_rebuild_locked_message() -> &'static str {
    "Search is unavailable while the encryption session is locked. Unlock first, or run `uniclipboard-cli space-status` to inspect encryption state."
}

/// Format a millisecond timestamp as a human-readable UTC string.
fn format_search_timestamp(ts_ms: i64) -> String {
    use chrono::{TimeZone, Utc};
    match Utc.timestamp_millis_opt(ts_ms) {
        chrono::LocalResult::Single(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
        _ => format!("<invalid timestamp: {ts_ms}>"),
    }
}

/// Render human-readable output for a search query response.
fn render_query_output(
    response: &uc_daemon::api::dto::search::SearchQueryResponse,
    detailed: bool,
) -> String {
    let total = response.total;
    let showing_from = response.data.len().min(1);
    let showing_to = response.data.len();
    let mut lines = vec![format!(
        "Search results: {total} total (showing {showing_from}-{showing_to})"
    )];

    if response.data.is_empty() {
        lines.push("No search results found.".to_string());
        lines.push("Try widening the time range.".to_string());
        lines.push("Try removing one or more filters.".to_string());
        lines.push("Try a fuller token; search is exact-token in V1.".to_string());
        return lines.join("\n");
    }

    for item in &response.data {
        let formatted_time = format_search_timestamp(item.active_time_ms);
        let preview = item.text_preview.as_deref().unwrap_or("<no preview>");
        let content_type = format!("{:?}", item.content_type).to_lowercase();
        lines.push(format!("- [{content_type}] {formatted_time}  {preview}"));

        if detailed {
            lines.push(format!("    entryId: {}", item.entry_id));
            lines.push(format!("    mimeType: {}", item.mime_type));
            let exts = if item.file_extensions.is_empty() {
                "<none>".to_string()
            } else {
                item.file_extensions.join(",")
            };
            lines.push(format!("    extensions: {exts}"));
        }
    }

    lines.join("\n")
}

/// Render human-readable output for a search status response.
fn render_status_output(response: &uc_daemon::api::dto::search::SearchStatusResponse) -> String {
    let data = &response.data;
    let reason = data.reason.as_deref().unwrap_or("none");
    let last_started = data
        .last_rebuild_started_at_ms
        .map(format_search_timestamp)
        .unwrap_or_else(|| "never".to_string());
    let last_completed = data
        .last_rebuild_completed_at_ms
        .map(format_search_timestamp)
        .unwrap_or_else(|| "never".to_string());

    vec![
        format!("Search state: {}", data.state),
        format!("Reason: {reason}"),
        format!("Last rebuild started: {last_started}"),
        format!("Last rebuild completed: {last_completed}"),
    ]
    .join("\n")
}
