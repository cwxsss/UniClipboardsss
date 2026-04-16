//! Centralized field redaction for OTLP span export.
//!
//! Wraps any `SpanExporter` and scrubs sensitive field values before they
//! leave the process. This ensures privacy guarantees are enforced at the
//! pipeline layer rather than relying on individual call sites.
//!
//! ## Blocklist approach (v1)
//!
//! Uses an explicit field-name blocklist. Matching is case-insensitive to
//! catch both `snake_case` tracing fields and `camelCase` OTel conventions.

use std::fmt;

use opentelemetry::{Key, KeyValue, Value};
use opentelemetry_sdk::trace::SpanData;

/// Replacement value inserted for redacted fields.
const REDACTED: &str = "[REDACTED]";

/// Field names whose values must be redacted before OTLP export.
///
/// Organised by category. Matching is case-insensitive.
///
/// ## Extension
///
/// Add new entries here when new sensitive fields are introduced.
/// Prefer explicit names over substring matching to avoid false positives.
const REDACTED_FIELDS: &[&str] = &[
    // ── Clipboard content (never export user data) ──────────────
    "content",
    "text",
    "raw_text",
    "display_text",
    "clipboard_content",
    "clipboard_text",
    "clipboard.content",
    "clipboard.text",
    "payload",
    "body",
    // ── Binary / byte data ──────────────────────────────────────
    "raw_bytes",
    "blob_data",
    // ── Authentication & secrets ────────────────────────────────
    "token",
    "auth_token",
    "session_token",
    "api_key",
    "secret",
    "password",
    "passphrase",
    "credential",
    "private_key",
    "encryption_key",
    "master_key",
    // ── File system paths (may leak usernames / dir structure) ──
    "file_path",
    "file_content",
    "full_path",
    "home_dir",
    // ── Sensitive queries ───────────────────────────────────────
    "search_query",
];

fn canonicalize_key(input: &str) -> String {
    input
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Returns `true` if `key` matches any entry in the blocklist (case-insensitive).
fn is_sensitive(key: &Key) -> bool {
    let k = canonicalize_key(key.as_str());
    REDACTED_FIELDS
        .iter()
        .any(|blocked| k == canonicalize_key(blocked))
}

/// Redact sensitive values in a slice of [`KeyValue`] attributes in-place.
fn redact_attributes(attrs: &mut [KeyValue]) {
    for attr in attrs.iter_mut() {
        if is_sensitive(&attr.key) {
            attr.value = Value::String(REDACTED.into());
        }
    }
}

/// Redact all sensitive fields in a single [`SpanData`].
pub(crate) fn redact_span(span: &mut SpanData) {
    redact_attributes(&mut span.attributes);
    for event in &mut span.events.events {
        redact_attributes(&mut event.attributes);
    }
}

// ── RedactingExporter ───────────────────────────────────────────

/// A [`SpanExporter`] decorator that redacts sensitive field values before
/// forwarding the batch to the inner exporter.
pub struct RedactingExporter<E> {
    inner: E,
}

impl<E> RedactingExporter<E> {
    pub fn new(inner: E) -> Self {
        Self { inner }
    }
}

impl<E: fmt::Debug> fmt::Debug for RedactingExporter<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RedactingExporter")
            .field("inner", &self.inner)
            .finish()
    }
}

impl<E: opentelemetry_sdk::trace::SpanExporter + 'static> opentelemetry_sdk::trace::SpanExporter
    for RedactingExporter<E>
{
    fn export(
        &self,
        batch: Vec<SpanData>,
    ) -> impl std::future::Future<Output = opentelemetry_sdk::error::OTelSdkResult> + Send {
        let mut redacted = batch;
        for span in &mut redacted {
            redact_span(span);
        }
        self.inner.export(redacted)
    }

    fn shutdown(&mut self) -> opentelemetry_sdk::error::OTelSdkResult {
        self.inner.shutdown()
    }

    fn force_flush(&mut self) -> opentelemetry_sdk::error::OTelSdkResult {
        self.inner.force_flush()
    }

    fn set_resource(&mut self, resource: &opentelemetry_sdk::Resource) {
        self.inner.set_resource(resource);
    }
}

// ── Tests ───────────────────────────────────────────────────────
