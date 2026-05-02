use opentelemetry::global;
use opentelemetry_otlp::{Protocol, SpanExporter, WithExportConfig};
use opentelemetry_sdk::{
    logs::{BatchConfigBuilder as LogBatchConfigBuilder, BatchLogProcessor, SdkLoggerProvider},
    propagation::TraceContextPropagator,
    trace::{BatchConfigBuilder as SpanBatchConfigBuilder, BatchSpanProcessor, SdkTracerProvider},
};

use crate::profile::LogProfile;

use super::{config, redact, resource};

/// Queue-depth tuning for the OTLP batch processors.
///
/// 背景：默认 `max_queue_size = 2048` 在密集 tracing 场景下（例如文件同步 + iroh
/// 网络发现同时活跃）容易触发 `BatchSpanProcessor dropped a Span due to queue full`，
/// 导致关键业务 span / log 被丢弃，诊断时看起来像是代码静默失败。将队列扩到 16k
/// 并把 export batch 提到 2k，保证突发流量下有足够缓冲而不至于丢日志。
const OTLP_MAX_QUEUE_SIZE: usize = 16_384;
const OTLP_MAX_EXPORT_BATCH_SIZE: usize = 2_048;

/// Guard that keeps the OTLP tracer and logger providers alive.
/// On drop, flushes pending data and shuts down the providers.
pub struct OtlpGuard {
    tracer_provider: Option<SdkTracerProvider>,
    logger_provider: Option<SdkLoggerProvider>,
}

impl Drop for OtlpGuard {
    fn drop(&mut self) {
        if let Some(provider) = self.logger_provider.take() {
            match provider.shutdown() {
                Ok(()) => {}
                Err(e) => {
                    tracing::warn!(error = %e, "OTLP logger provider shutdown failed");
                }
            }
        }
        if let Some(provider) = self.tracer_provider.take() {
            match provider.shutdown() {
                Ok(()) => {}
                Err(e) => {
                    tracing::warn!(error = %e, "OTLP tracer provider shutdown failed");
                }
            }
        }
    }
}

fn build_span_exporter_from_env() -> anyhow::Result<SpanExporter> {
    SpanExporter::builder()
        .with_http()
        .with_protocol(Protocol::HttpBinary)
        .build()
        .map_err(|e| anyhow::anyhow!("build OTLP span exporter: {e}"))
}

/// Check whether the OTLP pipeline should be activated for the given profile
/// and user telemetry preference.
///
/// Activation rules:
/// - Dev / DebugClipboard / Cli: always allowed (developer-controlled)
/// - Prod: only when `telemetry_enabled` is `true`
fn otlp_is_enabled(profile: &LogProfile, telemetry_enabled: bool) -> bool {
    match profile {
        LogProfile::Prod => telemetry_enabled,
        _ => true,
    }
}

fn build_log_exporter_from_env() -> anyhow::Result<opentelemetry_otlp::LogExporter> {
    opentelemetry_otlp::LogExporter::builder()
        .with_http()
        .with_protocol(Protocol::HttpBinary)
        .build()
        .map_err(|e| anyhow::anyhow!("build OTLP log exporter: {e}"))
}

fn build_otlp_guard(
    tracer_provider: &SdkTracerProvider,
    logger_provider: &SdkLoggerProvider,
) -> OtlpGuard {
    OtlpGuard {
        tracer_provider: Some(tracer_provider.clone()),
        logger_provider: Some(logger_provider.clone()),
    }
}

/// Initialize the OTLP provider with dual-layer gating:
/// 1. Endpoint must be configured (env var or baked-in)
/// 2. Profile + `telemetry_enabled` must allow it
pub(super) fn init_provider_and_guard(
    profile: &LogProfile,
    device_id: Option<&str>,
    telemetry_enabled: bool,
) -> anyhow::Result<Option<(SdkTracerProvider, SdkLoggerProvider, OtlpGuard)>> {
    // Always install the W3C propagator.
    global::set_text_map_propagator(TraceContextPropagator::new());

    if !otlp_is_enabled(profile, telemetry_enabled) || !config::otlp_endpoint_is_configured() {
        return Ok(None);
    }

    config::prime_runtime_otlp_env_from_baked();
    let resource = resource::build_resource(device_id);

    // Trace provider — explicit BatchSpanProcessor with widened queue.
    let raw_span_exporter = build_span_exporter_from_env()?;
    let span_exporter = redact::RedactingExporter::new(raw_span_exporter);
    let span_batch_config = SpanBatchConfigBuilder::default()
        .with_max_queue_size(OTLP_MAX_QUEUE_SIZE)
        .with_max_export_batch_size(OTLP_MAX_EXPORT_BATCH_SIZE)
        .build();
    let span_processor = BatchSpanProcessor::builder(span_exporter)
        .with_batch_config(span_batch_config)
        .build();
    let tracer_provider = SdkTracerProvider::builder()
        .with_span_processor(span_processor)
        .with_resource(resource.clone())
        .build();

    // Logs provider — matching widened queue.
    let log_exporter = build_log_exporter_from_env()?;
    let log_batch_config = LogBatchConfigBuilder::default()
        .with_max_queue_size(OTLP_MAX_QUEUE_SIZE)
        .with_max_export_batch_size(OTLP_MAX_EXPORT_BATCH_SIZE)
        .build();
    let log_processor = BatchLogProcessor::builder(log_exporter)
        .with_batch_config(log_batch_config)
        .build();
    let logger_provider = SdkLoggerProvider::builder()
        .with_log_processor(log_processor)
        .with_resource(resource)
        .build();

    let guard = build_otlp_guard(&tracer_provider, &logger_provider);

    Ok(Some((tracer_provider, logger_provider, guard)))
}
