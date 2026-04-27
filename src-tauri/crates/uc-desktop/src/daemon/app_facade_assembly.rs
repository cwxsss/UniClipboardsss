//! daemon AppFacade 装配。

use std::sync::Arc;

use uc_application::clipboard_write::ClipboardWriteCoordinator;
use uc_application::deps::AppDeps;
use uc_application::facade::{
    AppFacade, AppPaths, BlobTransferFacade, ClipboardSyncFacade, LifecycleStatusGateway,
    MemberRosterFacade, SearchCoordinator, SpaceSetupFacade,
};
use uc_bootstrap::{
    build_app_facade_from_deps, AppFacadeAssemblyOptions, ClipboardRestoreAssembly,
};
use uc_core::clipboard::ClipboardIntegrationMode;

/// daemon AppFacade 装配输入。
pub struct DaemonAppFacadeAssemblyInput<'a> {
    pub deps: &'a AppDeps,
    pub storage_paths: &'a AppPaths,
    pub lifecycle_status: Arc<dyn LifecycleStatusGateway>,
    pub space_setup: Arc<SpaceSetupFacade>,
    pub member_roster: Arc<MemberRosterFacade>,
    pub clipboard_sync: Arc<ClipboardSyncFacade>,
    pub blob_transfer: Arc<BlobTransferFacade>,
    pub clipboard_write_coordinator: Arc<ClipboardWriteCoordinator>,
    pub clipboard_integration_mode: ClipboardIntegrationMode,
    pub search_coordinator: Arc<SearchCoordinator>,
}

/// 构造 daemon 对外统一业务入口。
pub fn build_daemon_app_facade(input: DaemonAppFacadeAssemblyInput<'_>) -> Arc<AppFacade> {
    build_app_facade_from_deps(
        input.deps,
        input.storage_paths,
        input.lifecycle_status,
        AppFacadeAssemblyOptions {
            space_setup: Some(input.space_setup),
            member_roster: Some(input.member_roster),
            clipboard_sync: Some(input.clipboard_sync),
            blob_transfer: Some(input.blob_transfer),
            clipboard_restore: Some(ClipboardRestoreAssembly {
                write_coordinator: input.clipboard_write_coordinator,
                integration_mode: input.clipboard_integration_mode,
            }),
            search_coordinator: Some(input.search_coordinator),
        },
    )
}
