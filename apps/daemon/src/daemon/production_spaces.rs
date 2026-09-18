//! Production Windows multi-space composition.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, info_span, warn, Instrument};
use uc_bootstrap::{
    DesktopClipboardHub, DesktopClipboardHubChangeStream, DesktopClipboardProfileHandle,
};
use uc_daemon_contract::api::dto::v2::spaces::{
    CreateSpaceProfileRequestDto, JoinSpaceProfileRequestDto, SetActiveSendSpaceRequestDto,
    SpaceFaultDto, SpaceIncomingSyncStateDto, SpaceProfileSummaryDto, SpaceRuntimeStateDto,
};
use uc_engine::{
    CreateSpaceInput, Engine, JoinSpaceInput, JoinSpaceStatusSummary,
    JoinSpaceTerminationReasonSummary, Operation, OperationResult, SecretString,
};
use uc_platform::clipboard::SystemClipboardSnapshot;

use super::clipboard_router::{
    spawn_clipboard_router, ClipboardRouterBackend, ClipboardRouterHandle, ClipboardRouterTask,
};
use super::run_mode::DaemonRunMode;
use super::space_catalog::{SpaceCatalog, SpaceCatalogEntry};
use super::space_runtime_supervisor::{
    SpaceRuntimeLifecycle, SpaceRuntimeRoots, SpaceRuntimeStatus, SpaceRuntimeSupervisor,
};
use super::spaces_http::{SpacesBackendError, SpacesHttpBackend, SpacesHttpService};
use super::startup_recovery::spawn_startup_recovery;
use super::windows_space_authority::{
    CatalogPort, ClipboardRouterPort, RuntimePort, WindowsSpaceAuthority,
    WindowsSpaceAuthorityError,
};

const JOIN_COMPLETION_TIMEOUT: Duration = Duration::from_secs(90);
const JOIN_POLL_INTERVAL: Duration = Duration::from_millis(200);
const CLIPBOARD_FORWARDER_RECOVERY_DELAY: Duration = Duration::from_millis(250);

#[derive(Clone)]
struct CatalogRepository {
    root: PathBuf,
}

impl CatalogRepository {
    fn new(root: PathBuf) -> Self {
        Self { root }
    }

    async fn read<R, F>(&self, read: F) -> anyhow::Result<R>
    where
        R: Send + 'static,
        F: FnOnce(&SpaceCatalog) -> anyhow::Result<R> + Send + 'static,
    {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            let catalog = SpaceCatalog::load_or_migrate(root)?;
            read(&catalog)
        })
        .await
        .map_err(|error| anyhow::anyhow!("catalog task failed: {error}"))?
    }

    async fn mutate<R, F>(&self, mutate: F) -> anyhow::Result<R>
    where
        R: Send + 'static,
        F: FnOnce(&mut SpaceCatalog) -> anyhow::Result<R> + Send + 'static,
    {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            let mut catalog = SpaceCatalog::load_or_migrate(root)?;
            mutate(&mut catalog)
        })
        .await
        .map_err(|error| anyhow::anyhow!("catalog task failed: {error}"))?
    }

    async fn entries(&self) -> anyhow::Result<Vec<SpaceCatalogEntry>> {
        self.read(|catalog| Ok(catalog.entries().to_vec())).await
    }

    async fn active_profile(&self) -> anyhow::Result<String> {
        self.read(|catalog| {
            catalog
                .entries()
                .iter()
                .find(|entry| entry.active_send)
                .map(|entry| entry.profile_id.clone())
                .ok_or_else(|| anyhow::anyhow!("catalog has no active-send profile"))
        })
        .await
    }

    async fn reserve_entry(&self) -> anyhow::Result<SpaceCatalogEntry> {
        self.read(|catalog| Ok(catalog.new_profile_entry())).await
    }

    async fn publish_entry(&self, entry: SpaceCatalogEntry) -> anyhow::Result<()> {
        self.mutate(move |catalog| Ok(catalog.add_entry(entry)?))
            .await
    }

    async fn set_active(&self, profile_id: String) -> anyhow::Result<()> {
        self.mutate(move |catalog| Ok(catalog.set_active_send(&profile_id)?))
            .await
    }

    async fn remove(&self, profile_id: String) -> anyhow::Result<SpaceCatalogEntry> {
        self.mutate(move |catalog| Ok(catalog.remove_profile(&profile_id)?))
            .await
    }
}

struct ProductionCatalogPort {
    catalog: CatalogRepository,
}

#[async_trait]
impl CatalogPort for ProductionCatalogPort {
    async fn profile_dir(&self, profile_id: &str) -> anyhow::Result<Option<String>> {
        let profile_id = profile_id.to_owned();
        self.catalog
            .read(move |catalog| {
                Ok(catalog
                    .entries()
                    .iter()
                    .find(|entry| entry.profile_id == profile_id)
                    .map(|entry| entry.profile_dir.clone()))
            })
            .await
    }

    async fn remove(&self, profile_id: &str) -> anyhow::Result<()> {
        self.catalog.remove(profile_id.to_owned()).await.map(drop)
    }
}

struct ProductionRuntimePort {
    supervisor: Arc<SpaceRuntimeSupervisor>,
}

#[async_trait]
impl RuntimePort for ProductionRuntimePort {
    async fn ensure_available(&self, profile_id: &str) -> anyhow::Result<()> {
        let status = self
            .supervisor
            .status(profile_id)
            .ok_or_else(|| anyhow::anyhow!("profile runtime is not registered"))?;
        if status.lifecycle != SpaceRuntimeLifecycle::Running {
            anyhow::bail!("profile runtime is not running");
        }
        Ok(())
    }

    async fn stop(&self, profile_id: &str) -> anyhow::Result<()> {
        let status = self
            .supervisor
            .stop_profile(profile_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("profile runtime is not registered"))?;
        if status.lifecycle != SpaceRuntimeLifecycle::Stopped {
            anyhow::bail!("profile runtime did not stop cleanly");
        }
        Ok(())
    }
}

struct ProductionClipboardRouterBackend {
    catalog: CatalogRepository,
    supervisor: Arc<SpaceRuntimeSupervisor>,
}

#[async_trait]
impl ClipboardRouterBackend<SystemClipboardSnapshot> for ProductionClipboardRouterBackend {
    async fn load_active_profile(&self, cancel: CancellationToken) -> anyhow::Result<String> {
        tokio::select! {
            _ = cancel.cancelled() => anyhow::bail!("active-profile load was cancelled"),
            result = self.catalog.active_profile() => result,
        }
    }

    async fn dispatch_snapshot(
        &self,
        profile_id: &str,
        snapshot: SystemClipboardSnapshot,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        self.supervisor
            .dispatch_snapshot(profile_id, snapshot, cancel)
            .await
            .map_err(anyhow::Error::new)
    }

    async fn persist_active_profile(
        &self,
        profile_id: &str,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        tokio::select! {
            _ = cancel.cancelled() => anyhow::bail!("active-profile persist was cancelled"),
            result = self.catalog.set_active(profile_id.to_owned()) => result,
        }
    }
}

struct ProductionSpacesBackend {
    catalog: CatalogRepository,
    supervisor: Arc<SpaceRuntimeSupervisor>,
    authority: Arc<WindowsSpaceAuthority>,
}

impl ProductionSpacesBackend {
    async fn engine_for(&self, profile_id: &str) -> Option<Arc<Engine>> {
        self.supervisor.engine(profile_id)
    }

    async fn summary(
        &self,
        entry: &SpaceCatalogEntry,
    ) -> Result<SpaceProfileSummaryDto, SpacesBackendError> {
        let status = self.supervisor.status(&entry.profile_id);
        let runtime_state = runtime_state(entry, status.as_ref());
        let last_fault = status
            .as_ref()
            .and_then(|status| status.last_failure.as_ref())
            .map(|failure| SpaceFaultDto {
                category: format!("{:?}", failure.category).to_ascii_lowercase(),
                message_code: None,
            });
        let Some(engine) = self.engine_for(&entry.profile_id).await else {
            return Ok(SpaceProfileSummaryDto {
                profile_id: entry.profile_id.clone(),
                space_id: None,
                display_name: None,
                device_name: None,
                runtime_state,
                incoming_sync_state: SpaceIncomingSyncStateDto::Disabled,
                last_fault,
                is_active_send: entry.active_send,
            });
        };

        let (space_id, device_name, configured) =
            match engine.execute(Operation::QuerySetupState).await {
                Ok(OperationResult::SetupState(setup)) => {
                    (setup.space_id, setup.device_name, setup.has_completed)
                }
                Ok(_) | Err(_) => (None, None, false),
            };
        let incoming_sync_state = match engine.execute(Operation::QueryReceiveReadiness).await {
            Ok(OperationResult::ReceiveReadiness(readiness)) if readiness.degraded => {
                SpaceIncomingSyncStateDto::Degraded
            }
            Ok(OperationResult::ReceiveReadiness(readiness)) if readiness.ready => {
                SpaceIncomingSyncStateDto::Enabled
            }
            _ => SpaceIncomingSyncStateDto::Disabled,
        };
        Ok(SpaceProfileSummaryDto {
            profile_id: entry.profile_id.clone(),
            space_id,
            display_name: None,
            device_name,
            runtime_state: if configured {
                runtime_state
            } else if runtime_state == SpaceRuntimeStateDto::Running {
                SpaceRuntimeStateDto::Locked
            } else {
                runtime_state
            },
            incoming_sync_state,
            last_fault,
            is_active_send: entry.active_send,
        })
    }

    async fn start_reserved_runtime(
        &self,
        entry: &SpaceCatalogEntry,
    ) -> Result<Arc<Engine>, SpacesBackendError> {
        if let Err(error) = self.supervisor.start_entry(entry.clone()).await {
            warn!(
                profile_id = %entry.profile_id,
                error = %error.failure,
                "reserved space runtime failed to start"
            );
            let _ = self.supervisor.stop_profile(&entry.profile_id).await;
            return Err(SpacesBackendError::runtime_unavailable(
                "runtime_start_failed",
                "space runtime could not start",
            ));
        }
        self.supervisor.engine(&entry.profile_id).ok_or_else(|| {
            SpacesBackendError::runtime_unavailable(
                "runtime_unavailable",
                "space runtime did not expose an Engine",
            )
        })
    }

    async fn rollback_unpublished_runtime(&self, profile_id: &str) {
        let _ = self.supervisor.stop_profile(profile_id).await;
    }

    async fn publish_and_summarize(
        &self,
        entry: SpaceCatalogEntry,
    ) -> Result<SpaceProfileSummaryDto, SpacesBackendError> {
        if let Err(error) = self.catalog.publish_entry(entry.clone()).await {
            self.rollback_unpublished_runtime(&entry.profile_id).await;
            return Err(SpacesBackendError::internal(format!(
                "failed to publish completed profile: {error}"
            )));
        }
        self.summary(&entry).await
    }
}

#[async_trait]
impl SpacesHttpBackend for ProductionSpacesBackend {
    async fn list_spaces(&self) -> Result<Vec<SpaceProfileSummaryDto>, SpacesBackendError> {
        let entries = self
            .catalog
            .entries()
            .await
            .map_err(|error| SpacesBackendError::internal(error.to_string()))?;
        let mut summaries = Vec::with_capacity(entries.len());
        for entry in entries {
            summaries.push(self.summary(&entry).await?);
        }
        Ok(summaries)
    }

    async fn create_space(
        &self,
        request: CreateSpaceProfileRequestDto,
    ) -> Result<SpaceProfileSummaryDto, SpacesBackendError> {
        if request.passphrase != request.passphrase_confirm {
            return Err(SpacesBackendError::bad_request(
                "passphrase_mismatch",
                "passphrase confirmation does not match",
            ));
        }
        let _mutation = self
            .authority
            .acquire_mutation()
            .await
            .map_err(map_authority)?;
        let entry = self
            .catalog
            .reserve_entry()
            .await
            .map_err(|error| SpacesBackendError::internal(error.to_string()))?;
        let engine = self.start_reserved_runtime(&entry).await?;
        let result = engine
            .execute(Operation::CreateSpace(CreateSpaceInput {
                passphrase: SecretString::new(request.passphrase),
                passphrase_confirmation: SecretString::new(request.passphrase_confirm),
                device_name: request.device_name,
            }))
            .await;
        if !matches!(result, Ok(OperationResult::SpaceCreated { .. })) {
            self.rollback_unpublished_runtime(&entry.profile_id).await;
            return Err(SpacesBackendError::runtime_unavailable(
                "create_failed",
                "space creation did not complete",
            ));
        }
        self.publish_and_summarize(entry).await
    }

    async fn join_space(
        &self,
        request: JoinSpaceProfileRequestDto,
    ) -> Result<SpaceProfileSummaryDto, SpacesBackendError> {
        if request.code.trim().is_empty() {
            return Err(SpacesBackendError::bad_request(
                "invitation_required",
                "invitation code is required",
            ));
        }
        let _mutation = self
            .authority
            .acquire_mutation()
            .await
            .map_err(map_authority)?;
        let entry = self
            .catalog
            .reserve_entry()
            .await
            .map_err(|error| SpacesBackendError::internal(error.to_string()))?;
        let engine = self.start_reserved_runtime(&entry).await?;
        let result = engine
            .execute(Operation::JoinSpace(JoinSpaceInput {
                invitation_code: request.code,
                device_name: request.device_name,
                passphrase: SecretString::new(request.passphrase),
                preserve_unreadable_history: false,
            }))
            .await;
        let status = match result {
            Ok(OperationResult::JoinSpace(status)) => status,
            _ => {
                self.rollback_unpublished_runtime(&entry.profile_id).await;
                return Err(SpacesBackendError::runtime_unavailable(
                    "join_failed",
                    "space join could not be started",
                ));
            }
        };
        if let Err(error) = wait_for_join_completion(&engine, status).await {
            self.rollback_unpublished_runtime(&entry.profile_id).await;
            return Err(error);
        }
        self.publish_and_summarize(entry).await
    }

    async fn set_active_send(
        &self,
        request: SetActiveSendSpaceRequestDto,
    ) -> Result<SpaceProfileSummaryDto, SpacesBackendError> {
        self.authority
            .set_active(&request.profile_id)
            .await
            .map_err(map_authority)?;
        let profile_id = request.profile_id;
        let entry = self
            .catalog
            .read(move |catalog| {
                catalog
                    .entries()
                    .iter()
                    .find(|entry| entry.profile_id == profile_id)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("active profile disappeared from catalog"))
            })
            .await
            .map_err(|error| SpacesBackendError::internal(error.to_string()))?;
        self.summary(&entry).await
    }

    async fn remove_space(
        &self,
        profile_id: String,
    ) -> Result<SpaceProfileSummaryDto, SpacesBackendError> {
        let before = self
            .catalog
            .read({
                let profile_id = profile_id.clone();
                move |catalog| {
                    catalog
                        .entries()
                        .iter()
                        .find(|entry| entry.profile_id == profile_id)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("profile not found"))
                }
            })
            .await
            .map_err(|_| {
                SpacesBackendError::not_found("profile_not_found", "space was not found")
            })?;
        self.authority
            .remove(&profile_id)
            .await
            .map_err(map_authority)?;
        let mut summary = self.summary(&before).await?;
        summary.runtime_state = SpaceRuntimeStateDto::Stopped;
        summary.incoming_sync_state = SpaceIncomingSyncStateDto::Disabled;
        summary.is_active_send = false;
        Ok(summary)
    }
}

async fn wait_for_join_completion(
    engine: &Arc<Engine>,
    mut status: JoinSpaceStatusSummary,
) -> Result<(), SpacesBackendError> {
    let deadline = tokio::time::Instant::now() + JOIN_COMPLETION_TIMEOUT;
    loop {
        match status {
            JoinSpaceStatusSummary::Active { .. } => return Ok(()),
            JoinSpaceStatusSummary::Rejected { .. } => {
                return Err(SpacesBackendError::conflict(
                    "join_rejected",
                    "space join was rejected",
                ))
            }
            // rc.17 起 Engine 会把「已终结」的加入尝试单独上报：取消 / 过期 / 被新的
            // 邀请取代。它是终态而非瞬时状态，继续轮询只会耗到超时，因此立刻失败并
            // 把具体原因透出给调用方。
            JoinSpaceStatusSummary::Terminated { reason, .. } => {
                let reason_code = match reason {
                    JoinSpaceTerminationReasonSummary::Cancelled => "cancelled",
                    JoinSpaceTerminationReasonSummary::Expired => "expired",
                    JoinSpaceTerminationReasonSummary::Superseded => "superseded",
                };
                return Err(SpacesBackendError::conflict(
                    "join_terminated",
                    format!("space join was terminated: {reason_code}"),
                ));
            }
            JoinSpaceStatusSummary::Pending { ref join_id, .. } => {
                let expected_join_id = join_id.clone();
                if tokio::time::Instant::now() >= deadline {
                    return Err(SpacesBackendError::runtime_unavailable(
                        "join_timeout",
                        "space join is still pending",
                    ));
                }
                tokio::time::sleep(JOIN_POLL_INTERVAL).await;
                status = match engine.execute(Operation::QueryDeviceGroupChoices).await {
                    Ok(OperationResult::DeviceGroupChoices(choices)) => {
                        match choices.device_trust.current_join {
                            Some(candidate) if join_id_of(&candidate) == expected_join_id => {
                                candidate
                            }
                            _ => continue,
                        }
                    }
                    _ => continue,
                };
            }
        }
    }
}

fn join_id_of(status: &JoinSpaceStatusSummary) -> &str {
    match status {
        JoinSpaceStatusSummary::Active { join_id, .. }
        | JoinSpaceStatusSummary::Pending { join_id, .. }
        | JoinSpaceStatusSummary::Rejected { join_id, .. }
        | JoinSpaceStatusSummary::Terminated { join_id, .. } => join_id,
    }
}

fn runtime_state(
    _entry: &SpaceCatalogEntry,
    status: Option<&SpaceRuntimeStatus>,
) -> SpaceRuntimeStateDto {
    match status.map(|status| status.lifecycle) {
        Some(SpaceRuntimeLifecycle::Running) => SpaceRuntimeStateDto::Running,
        Some(SpaceRuntimeLifecycle::Starting) => SpaceRuntimeStateDto::Starting,
        Some(SpaceRuntimeLifecycle::Failed) => SpaceRuntimeStateDto::Failed,
        Some(SpaceRuntimeLifecycle::Stopping | SpaceRuntimeLifecycle::Stopped) | None => {
            SpaceRuntimeStateDto::Stopped
        }
    }
}

fn map_authority(error: WindowsSpaceAuthorityError) -> SpacesBackendError {
    match error {
        WindowsSpaceAuthorityError::ProfileNotFound(_) => {
            SpacesBackendError::not_found("profile_not_found", "space was not found")
        }
        WindowsSpaceAuthorityError::LegacyProfileCannotBeRemoved => SpacesBackendError::conflict(
            "legacy_profile",
            "the compatibility space cannot be removed",
        ),
        WindowsSpaceAuthorityError::ActiveProfileCannotBeRemoved => SpacesBackendError::conflict(
            "active_profile",
            "the active-send space cannot be removed",
        ),
        WindowsSpaceAuthorityError::Quiescing => {
            SpacesBackendError::runtime_unavailable("daemon_stopping", "the daemon is stopping")
        }
        WindowsSpaceAuthorityError::Runtime(message) => {
            SpacesBackendError::runtime_unavailable("runtime_unavailable", message)
        }
        WindowsSpaceAuthorityError::Catalog(message)
        | WindowsSpaceAuthorityError::Router(message) => SpacesBackendError::internal(message),
    }
}

#[async_trait]
trait ClipboardForwarderStream: Send {
    async fn next_snapshot(&mut self) -> anyhow::Result<Option<SystemClipboardSnapshot>>;

    async fn shutdown(&mut self) -> anyhow::Result<()>;
}

#[async_trait]
impl ClipboardForwarderStream for DesktopClipboardHubChangeStream {
    async fn next_snapshot(&mut self) -> anyhow::Result<Option<SystemClipboardSnapshot>> {
        DesktopClipboardHubChangeStream::next(self)
            .await
            .map_err(anyhow::Error::new)
    }

    async fn shutdown(&mut self) -> anyhow::Result<()> {
        DesktopClipboardHubChangeStream::shutdown(self)
            .await
            .map_err(anyhow::Error::new)
    }
}

trait ClipboardForwarderStreamSource: Send + Sync {
    fn take_stream(&self) -> anyhow::Result<Option<Box<dyn ClipboardForwarderStream>>>;
}

struct HubClipboardForwarderStreamSource {
    hub: DesktopClipboardHub,
}

impl ClipboardForwarderStreamSource for HubClipboardForwarderStreamSource {
    fn take_stream(&self) -> anyhow::Result<Option<Box<dyn ClipboardForwarderStream>>> {
        self.hub
            .take_change_stream()
            .map_err(anyhow::Error::new)
            .map(|stream| {
                stream.map(|stream| Box::new(stream) as Box<dyn ClipboardForwarderStream>)
            })
    }
}

enum ClipboardForwarderStreamExit {
    Cancelled,
    Closed,
    Failed(anyhow::Error),
}

struct ClipboardForwarder {
    cancel: CancellationToken,
    join: Option<tokio::task::JoinHandle<anyhow::Result<()>>>,
}

impl ClipboardForwarder {
    fn spawn(
        initial_stream: Option<DesktopClipboardHubChangeStream>,
        hub: DesktopClipboardHub,
        router: ClipboardRouterHandle<SystemClipboardSnapshot>,
    ) -> Self {
        Self::spawn_with_source(
            initial_stream.map(|stream| Box::new(stream) as Box<dyn ClipboardForwarderStream>),
            Arc::new(HubClipboardForwarderStreamSource { hub }),
            router,
            CLIPBOARD_FORWARDER_RECOVERY_DELAY,
        )
    }

    fn spawn_with_source(
        initial_stream: Option<Box<dyn ClipboardForwarderStream>>,
        source: Arc<dyn ClipboardForwarderStreamSource>,
        router: ClipboardRouterHandle<SystemClipboardSnapshot>,
        recovery_delay: Duration,
    ) -> Self {
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let join = initial_stream.map(|stream| {
            let span = info_span!("daemon.clipboard_forwarder");
            tokio::spawn(
                async move {
                    run_clipboard_forwarder(stream, source, router, task_cancel, recovery_delay)
                        .await
                }
                .instrument(span),
            )
        });
        Self { cancel, join }
    }

    async fn shutdown(&mut self) -> anyhow::Result<()> {
        self.cancel.cancel();
        if let Some(join) = self.join.take() {
            join.await
                .map_err(|error| anyhow::anyhow!("clipboard watcher task failed: {error}"))??;
        }
        Ok(())
    }
}

async fn run_clipboard_forwarder(
    mut stream: Box<dyn ClipboardForwarderStream>,
    source: Arc<dyn ClipboardForwarderStreamSource>,
    router: ClipboardRouterHandle<SystemClipboardSnapshot>,
    cancel: CancellationToken,
    recovery_delay: Duration,
) -> anyhow::Result<()> {
    let mut recovery_attempt = 0_u32;
    info!("desktop clipboard forwarder started");

    loop {
        let exit = forward_clipboard_stream(stream.as_mut(), &router, &cancel).await;
        let shutdown_result = stream.shutdown().await;

        match exit {
            ClipboardForwarderStreamExit::Cancelled => {
                if let Err(error) = shutdown_result {
                    warn!(error = %error, "desktop clipboard watcher shutdown failed");
                    return Err(error);
                }
                return Ok(());
            }
            ClipboardForwarderStreamExit::Closed => {
                warn!("desktop clipboard watcher closed unexpectedly; scheduling recovery");
            }
            ClipboardForwarderStreamExit::Failed(error) => {
                warn!(error = %error, "desktop clipboard watcher failed; scheduling recovery");
            }
        }
        if let Err(error) = shutdown_result {
            warn!(error = %error, "desktop clipboard watcher cleanup failed during recovery");
        }
        if cancel.is_cancelled() {
            return Ok(());
        }

        recovery_attempt = recovery_attempt.saturating_add(1);
        loop {
            match source.take_stream() {
                Ok(Some(next_stream)) => {
                    info!(recovery_attempt, "desktop clipboard watcher recovered");
                    stream = next_stream;
                    break;
                }
                Ok(None) => {
                    debug!(
                        recovery_attempt,
                        "desktop clipboard watcher lease is not yet available"
                    );
                }
                Err(error) => {
                    warn!(
                        recovery_attempt,
                        error = %error,
                        "desktop clipboard watcher reacquire failed"
                    );
                }
            }
            if wait_for_clipboard_forwarder_retry(&cancel, recovery_delay).await {
                return Ok(());
            }
        }
    }
}

async fn forward_clipboard_stream(
    stream: &mut dyn ClipboardForwarderStream,
    router: &ClipboardRouterHandle<SystemClipboardSnapshot>,
    cancel: &CancellationToken,
) -> ClipboardForwarderStreamExit {
    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => return ClipboardForwarderStreamExit::Cancelled,
            next = stream.next_snapshot() => match next {
                Ok(Some(snapshot)) => {
                    if let Err(error) = router.clipboard_changed(snapshot).await {
                        warn!(
                            error = %error,
                            "desktop clipboard snapshot dispatch failed; keeping watcher active"
                        );
                    }
                }
                Ok(None) => return ClipboardForwarderStreamExit::Closed,
                Err(error) => return ClipboardForwarderStreamExit::Failed(error),
            },
        }
    }
}

async fn wait_for_clipboard_forwarder_retry(cancel: &CancellationToken, delay: Duration) -> bool {
    tokio::select! {
        _ = cancel.cancelled() => true,
        _ = tokio::time::sleep(delay) => false,
    }
}

pub(crate) struct WindowsMultiSpace {
    pub(crate) service: SpacesHttpService,
    authority: Arc<WindowsSpaceAuthority>,
    forwarder: ClipboardForwarder,
    router_task: Option<ClipboardRouterTask<SystemClipboardSnapshot>>,
    supervisor: Arc<SpaceRuntimeSupervisor>,
}

impl WindowsMultiSpace {
    pub(crate) async fn start(
        catalog_root: PathBuf,
        roots: SpaceRuntimeRoots,
        initial_engine: Arc<Engine>,
        hub: DesktopClipboardHub,
        initial_clipboard: DesktopClipboardProfileHandle,
        run_mode: DaemonRunMode,
    ) -> anyhow::Result<Self> {
        let catalog = CatalogRepository::new(catalog_root);
        let entries = catalog.entries().await?;
        let default_entry = entries
            .iter()
            .find(|entry| entry.profile_dir == ".")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("space catalog has no default profile"))?;
        let supervisor = SpaceRuntimeSupervisor::production(roots, hub.clone());
        supervisor
            .adopt_running_engine(
                default_entry,
                initial_engine,
                hub.clone(),
                initial_clipboard,
            )
            .map_err(|error| anyhow::anyhow!("failed to adopt default space runtime: {error}"))?;
        for entry in entries.iter().filter(|entry| entry.enabled).cloned() {
            match supervisor.start_entry(entry.clone()).await {
                Ok(_) => {
                    if let Some(engine) = supervisor.engine(&entry.profile_id) {
                        spawn_startup_recovery(run_mode, engine);
                    }
                }
                Err(error) => warn!(
                    profile_id = %entry.profile_id,
                    error = %error.failure,
                    "secondary space runtime recovery failed"
                ),
            }
        }

        let backend = Arc::new(ProductionClipboardRouterBackend {
            catalog: catalog.clone(),
            supervisor: Arc::clone(&supervisor),
        });
        let (router, router_task) = spawn_clipboard_router(backend);
        let authority = Arc::new(WindowsSpaceAuthority::new(
            Arc::new(ProductionCatalogPort {
                catalog: catalog.clone(),
            }),
            Arc::new(ProductionRuntimePort {
                supervisor: Arc::clone(&supervisor),
            }),
            Arc::new(ClipboardRouterPort::new(router.clone())),
        ));
        let service = SpacesHttpService::new(Arc::new(ProductionSpacesBackend {
            catalog,
            supervisor: Arc::clone(&supervisor),
            authority: Arc::clone(&authority),
        }));
        let stream = hub.take_change_stream()?;
        let forwarder = ClipboardForwarder::spawn(stream, hub, router);
        info!("Windows multi-space runtime started");
        Ok(Self {
            service,
            authority,
            forwarder,
            router_task: Some(router_task),
            supervisor,
        })
    }

    pub(crate) async fn quiesce(&self) -> anyhow::Result<()> {
        self.authority.quiesce().await.map_err(anyhow::Error::new)
    }

    pub(crate) async fn shutdown_clipboard(&mut self) -> anyhow::Result<()> {
        self.forwarder.shutdown().await?;
        if let Some(router_task) = self.router_task.take() {
            router_task.shutdown().await?;
        }
        Ok(())
    }

    pub(crate) async fn shutdown_runtimes(&self) -> anyhow::Result<()> {
        let statuses = self.supervisor.shutdown_all().await;
        let failed: Vec<_> = statuses
            .into_iter()
            .filter(|status| status.lifecycle != SpaceRuntimeLifecycle::Stopped)
            .map(|status| status.profile_id)
            .collect();
        if !failed.is_empty() {
            anyhow::bail!("space runtimes did not stop cleanly: {failed:?}");
        }
        Ok(())
    }
}

pub(crate) fn catalog_root(process_data_root: &Path) -> PathBuf {
    process_data_root.to_path_buf()
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Mutex;

    use async_trait::async_trait;
    use tokio::sync::{Mutex as AsyncMutex, Notify};

    use super::*;

    struct ScriptedClipboardForwarderStream {
        events: VecDeque<ScriptedClipboardForwarderEvent>,
        shutdowns: Arc<AtomicUsize>,
    }

    enum ScriptedClipboardForwarderEvent {
        Snapshot(i64),
        Fail(&'static str),
    }

    #[async_trait]
    impl ClipboardForwarderStream for ScriptedClipboardForwarderStream {
        async fn next_snapshot(&mut self) -> anyhow::Result<Option<SystemClipboardSnapshot>> {
            match self.events.pop_front() {
                Some(ScriptedClipboardForwarderEvent::Snapshot(ts_ms)) => {
                    Ok(Some(test_snapshot(ts_ms)))
                }
                Some(ScriptedClipboardForwarderEvent::Fail(message)) => anyhow::bail!(message),
                None => {
                    std::future::pending::<anyhow::Result<Option<SystemClipboardSnapshot>>>().await
                }
            }
        }

        async fn shutdown(&mut self) -> anyhow::Result<()> {
            self.shutdowns.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    struct ScriptedClipboardForwarderSource {
        streams: Mutex<VecDeque<Box<dyn ClipboardForwarderStream>>>,
        take_count: AtomicUsize,
    }

    impl ScriptedClipboardForwarderSource {
        fn new(streams: Vec<Box<dyn ClipboardForwarderStream>>) -> Self {
            Self {
                streams: Mutex::new(streams.into()),
                take_count: AtomicUsize::new(0),
            }
        }
    }

    impl ClipboardForwarderStreamSource for ScriptedClipboardForwarderSource {
        fn take_stream(&self) -> anyhow::Result<Option<Box<dyn ClipboardForwarderStream>>> {
            self.take_count.fetch_add(1, Ordering::SeqCst);
            Ok(self
                .streams
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .pop_front())
        }
    }

    struct RecordingClipboardForwarderBackend {
        fail_first_dispatch: AtomicBool,
        dispatch_attempts: AtomicUsize,
        successful_dispatches: AsyncMutex<Vec<i64>>,
        successful_dispatch_notify: Notify,
    }

    impl RecordingClipboardForwarderBackend {
        fn new(fail_first_dispatch: bool) -> Self {
            Self {
                fail_first_dispatch: AtomicBool::new(fail_first_dispatch),
                dispatch_attempts: AtomicUsize::new(0),
                successful_dispatches: AsyncMutex::new(Vec::new()),
                successful_dispatch_notify: Notify::new(),
            }
        }

        async fn wait_for_successes(&self, expected: usize) {
            tokio::time::timeout(Duration::from_secs(1), async {
                loop {
                    let notified = self.successful_dispatch_notify.notified();
                    if self.successful_dispatches.lock().await.len() >= expected {
                        return;
                    }
                    notified.await;
                }
            })
            .await
            .expect("clipboard forwarder did not dispatch the expected snapshot");
        }
    }

    #[async_trait]
    impl ClipboardRouterBackend<SystemClipboardSnapshot> for RecordingClipboardForwarderBackend {
        async fn load_active_profile(&self, _cancel: CancellationToken) -> anyhow::Result<String> {
            Ok("profile-a".into())
        }

        async fn dispatch_snapshot(
            &self,
            _profile_id: &str,
            snapshot: SystemClipboardSnapshot,
            _cancel: CancellationToken,
        ) -> anyhow::Result<()> {
            self.dispatch_attempts.fetch_add(1, Ordering::SeqCst);
            if self.fail_first_dispatch.swap(false, Ordering::SeqCst) {
                anyhow::bail!("injected clipboard dispatch failure");
            }
            self.successful_dispatches.lock().await.push(snapshot.ts_ms);
            self.successful_dispatch_notify.notify_waiters();
            Ok(())
        }

        async fn persist_active_profile(
            &self,
            _profile_id: &str,
            _cancel: CancellationToken,
        ) -> anyhow::Result<()> {
            Ok(())
        }
    }

    fn test_snapshot(ts_ms: i64) -> SystemClipboardSnapshot {
        SystemClipboardSnapshot {
            ts_ms,
            representations: Vec::new(),
            file_content_digests: Vec::new(),
            file_set_v1_component: None,
        }
    }

    fn scripted_stream(
        events: Vec<ScriptedClipboardForwarderEvent>,
        shutdowns: Arc<AtomicUsize>,
    ) -> Box<dyn ClipboardForwarderStream> {
        Box::new(ScriptedClipboardForwarderStream {
            events: events.into(),
            shutdowns,
        })
    }

    #[tokio::test]
    async fn reserved_profile_is_invisible_until_successful_publication() {
        let root = tempfile::tempdir().unwrap();
        let catalog = CatalogRepository::new(root.path().to_path_buf());
        let before = catalog.entries().await.unwrap();

        let reserved = catalog.reserve_entry().await.unwrap();
        assert_eq!(catalog.entries().await.unwrap(), before);

        catalog.publish_entry(reserved.clone()).await.unwrap();
        let after = catalog.entries().await.unwrap();
        assert_eq!(after.len(), before.len() + 1);
        assert!(after.iter().any(|entry| entry == &reserved));
    }

    #[test]
    fn runtime_projection_keeps_stopping_non_runnable() {
        let entry = SpaceCatalogEntry {
            profile_id: uuid::Uuid::new_v4().to_string(),
            profile_dir: "profile-test".into(),
            enabled: true,
            active_send: false,
        };
        let status = SpaceRuntimeStatus {
            profile_id: entry.profile_id.clone(),
            generation: 2,
            lifecycle: SpaceRuntimeLifecycle::Stopping,
            last_failure: None,
        };
        assert_eq!(
            runtime_state(&entry, Some(&status)),
            SpaceRuntimeStateDto::Stopped
        );
    }

    #[tokio::test]
    async fn clipboard_forwarder_keeps_watching_after_one_dispatch_failure() {
        let backend = Arc::new(RecordingClipboardForwarderBackend::new(true));
        let (router, router_task) = spawn_clipboard_router(backend.clone());
        let shutdowns = Arc::new(AtomicUsize::new(0));
        let source = Arc::new(ScriptedClipboardForwarderSource::new(Vec::new()));
        let initial_stream = scripted_stream(
            vec![
                ScriptedClipboardForwarderEvent::Snapshot(1),
                ScriptedClipboardForwarderEvent::Snapshot(2),
            ],
            Arc::clone(&shutdowns),
        );
        let mut forwarder = ClipboardForwarder::spawn_with_source(
            Some(initial_stream),
            source,
            router,
            Duration::from_millis(1),
        );

        backend.wait_for_successes(1).await;
        assert_eq!(
            backend.dispatch_attempts.load(Ordering::SeqCst),
            2,
            "the second snapshot must still reach the router after the first dispatch fails"
        );
        assert_eq!(*backend.successful_dispatches.lock().await, vec![2]);

        forwarder.shutdown().await.unwrap();
        router_task.shutdown().await.unwrap();
        assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn clipboard_forwarder_reacquires_watcher_after_stream_failure() {
        let backend = Arc::new(RecordingClipboardForwarderBackend::new(false));
        let (router, router_task) = spawn_clipboard_router(backend.clone());
        let shutdowns = Arc::new(AtomicUsize::new(0));
        let source = Arc::new(ScriptedClipboardForwarderSource::new(vec![
            scripted_stream(
                vec![ScriptedClipboardForwarderEvent::Snapshot(2)],
                Arc::clone(&shutdowns),
            ),
        ]));
        let initial_stream = scripted_stream(
            vec![ScriptedClipboardForwarderEvent::Fail(
                "injected watcher stream failure",
            )],
            Arc::clone(&shutdowns),
        );
        let mut forwarder = ClipboardForwarder::spawn_with_source(
            Some(initial_stream),
            source.clone(),
            router,
            Duration::from_millis(1),
        );

        backend.wait_for_successes(1).await;
        assert_eq!(*backend.successful_dispatches.lock().await, vec![2]);
        assert!(
            source.take_count.load(Ordering::SeqCst) >= 1,
            "the forwarder must ask the source for a replacement watcher"
        );

        forwarder.shutdown().await.unwrap();
        router_task.shutdown().await.unwrap();
        assert_eq!(shutdowns.load(Ordering::SeqCst), 2);
    }
}
