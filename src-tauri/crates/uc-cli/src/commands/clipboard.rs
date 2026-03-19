//! Clipboard commands -- list, get, and clear clipboard history via direct bootstrap.

use serde::Serialize;
use std::fmt;

use crate::exit_codes;
use crate::output;

#[derive(Serialize)]
struct ClipboardEntryRow {
    id: String,
    preview: String,
    content_type: String,
    size_bytes: i64,
    captured_at_ms: i64,
    active_time_ms: i64,
    is_favorited: bool,
}

#[derive(Serialize)]
struct ClipboardListOutput {
    entries: Vec<ClipboardEntryRow>,
    count: usize,
}

#[derive(Serialize)]
struct ClipboardEntryDetail {
    id: String,
    content: String,
    mime_type: Option<String>,
    size_bytes: i64,
    created_at_ms: i64,
    active_time_ms: i64,
}

#[derive(Serialize)]
struct ClipboardClearOutput {
    deleted_count: u64,
    failed_count: usize,
}

impl fmt::Display for ClipboardListOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.entries.is_empty() {
            return write!(f, "No clipboard entries found.");
        }
        writeln!(f, "Clipboard entries: {}", self.count)?;
        for e in &self.entries {
            writeln!(
                f,
                "  {} {} ({}, {}B, captured_at={}, active_time={})",
                e.id, e.preview, e.content_type, e.size_bytes, e.captured_at_ms, e.active_time_ms
            )?;
        }
        Ok(())
    }
}

impl fmt::Display for ClipboardEntryDetail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "id: {}", self.id)?;
        writeln!(
            f,
            "mime_type: {}",
            self.mime_type.as_deref().unwrap_or("unknown")
        )?;
        writeln!(f, "size_bytes: {}", self.size_bytes)?;
        writeln!(f, "created_at: {}", self.created_at_ms)?;
        writeln!(f, "active_time: {}", self.active_time_ms)?;
        writeln!(f)?;
        write!(f, "{}", self.content)
    }
}

impl fmt::Display for ClipboardClearOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Cleared {} clipboard entries.", self.deleted_count)?;
        if self.failed_count > 0 {
            write!(f, " {} entries failed to delete.", self.failed_count)?;
        }
        Ok(())
    }
}

/// Run `clipboard list` command.
///
/// Lists clipboard history entries with preview, type, size, and timestamps.
pub async fn run_list(json: bool, verbose: bool, limit: usize, offset: usize) -> i32 {
    let profile = if verbose {
        Some(uc_observability::LogProfile::Dev)
    } else {
        Some(uc_observability::LogProfile::Cli)
    };

    let ctx = match uc_bootstrap::build_cli_context_with_profile(profile) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: failed to initialize CLI context: {}", e);
            return exit_codes::EXIT_ERROR;
        }
    };

    let storage_paths = match uc_bootstrap::get_storage_paths(&ctx.config) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: failed to resolve storage paths: {}", e);
            return exit_codes::EXIT_ERROR;
        }
    };

    let runtime = match uc_bootstrap::build_non_gui_runtime(ctx.deps, storage_paths) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: failed to build runtime: {}", e);
            return exit_codes::EXIT_ERROR;
        }
    };

    let usecases = uc_app::usecases::CoreUseCases::new(&runtime);

    let entries = match usecases
        .list_entry_projections()
        .execute(limit, offset)
        .await
    {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Error: failed to list clipboard entries: {}", e);
            return exit_codes::EXIT_ERROR;
        }
    };

    let rows: Vec<ClipboardEntryRow> = entries
        .into_iter()
        .map(|e| ClipboardEntryRow {
            id: e.id,
            preview: e.preview,
            content_type: e.content_type,
            size_bytes: e.size_bytes,
            captured_at_ms: e.captured_at,
            active_time_ms: e.active_time,
            is_favorited: e.is_favorited,
        })
        .collect();

    let result = ClipboardListOutput {
        count: rows.len(),
        entries: rows,
    };

    if let Err(e) = output::print_result(&result, json) {
        eprintln!("Error: {}", e);
        return exit_codes::EXIT_ERROR;
    }

    exit_codes::EXIT_SUCCESS
}

/// Run `clipboard get <id>` command.
///
/// Prints full content and metadata for a single clipboard entry.
pub async fn run_get(json: bool, verbose: bool, id: String) -> i32 {
    let profile = if verbose {
        Some(uc_observability::LogProfile::Dev)
    } else {
        Some(uc_observability::LogProfile::Cli)
    };

    let ctx = match uc_bootstrap::build_cli_context_with_profile(profile) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: failed to initialize CLI context: {}", e);
            return exit_codes::EXIT_ERROR;
        }
    };

    let storage_paths = match uc_bootstrap::get_storage_paths(&ctx.config) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: failed to resolve storage paths: {}", e);
            return exit_codes::EXIT_ERROR;
        }
    };

    let runtime = match uc_bootstrap::build_non_gui_runtime(ctx.deps, storage_paths) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: failed to build runtime: {}", e);
            return exit_codes::EXIT_ERROR;
        }
    };

    let usecases = uc_app::usecases::CoreUseCases::new(&runtime);

    let entry_id = uc_core::ids::EntryId::from_str(&id);

    let detail = match usecases.get_entry_detail().execute(&entry_id).await {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error: {}", e);
            return exit_codes::EXIT_ERROR;
        }
    };

    let result = ClipboardEntryDetail {
        id: detail.id,
        content: detail.content,
        mime_type: detail.mime_type,
        size_bytes: detail.size_bytes,
        created_at_ms: detail.created_at_ms,
        active_time_ms: detail.active_time_ms,
    };

    if let Err(e) = output::print_result(&result, json) {
        eprintln!("Error: {}", e);
        return exit_codes::EXIT_ERROR;
    }

    exit_codes::EXIT_SUCCESS
}

/// Run `clipboard clear` command.
///
/// Clears all clipboard history and reports the count of deleted entries.
pub async fn run_clear(json: bool, verbose: bool) -> i32 {
    let profile = if verbose {
        Some(uc_observability::LogProfile::Dev)
    } else {
        Some(uc_observability::LogProfile::Cli)
    };

    let ctx = match uc_bootstrap::build_cli_context_with_profile(profile) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: failed to initialize CLI context: {}", e);
            return exit_codes::EXIT_ERROR;
        }
    };

    let storage_paths = match uc_bootstrap::get_storage_paths(&ctx.config) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: failed to resolve storage paths: {}", e);
            return exit_codes::EXIT_ERROR;
        }
    };

    let runtime = match uc_bootstrap::build_non_gui_runtime(ctx.deps, storage_paths) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: failed to build runtime: {}", e);
            return exit_codes::EXIT_ERROR;
        }
    };

    let usecases = uc_app::usecases::CoreUseCases::new(&runtime);

    let result = match usecases.clear_clipboard_history().execute().await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: failed to clear clipboard history: {}", e);
            return exit_codes::EXIT_ERROR;
        }
    };

    let output = ClipboardClearOutput {
        deleted_count: result.deleted_count,
        failed_count: result.failed_entries.len(),
    };

    if let Err(e) = output::print_result(&output, json) {
        eprintln!("Error: {}", e);
        return exit_codes::EXIT_ERROR;
    }

    exit_codes::EXIT_SUCCESS
}
