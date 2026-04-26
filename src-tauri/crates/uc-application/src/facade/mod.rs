//! Slice 1 application facade tree.
//!
//! Per `uc-application/AGENTS.md` §11.4 external consumers only see the
//! top-level `AppFacade` and the per-domain sub-facades it aggregates.
//! Use cases live under `crate::usecases::<domain>` and stay `pub(crate)`;
//! sub-facades expose them through domain-scoped methods.

pub mod app_facade;
pub mod blob_transfer;
pub mod clipboard;
pub mod device;
pub mod lifecycle;
pub mod roster;
pub mod settings;
pub mod setup_status;
pub mod space_setup;
pub mod storage;

pub use app_facade::AppFacade;
pub use blob_transfer::{
    BlobTransferDeps, BlobTransferError, BlobTransferFacade, FetchBlobCommand, FetchBlobResult,
    PublishBlobCommand, PublishBlobResult,
};
pub use clipboard::{
    ClipboardSyncDeps, ClipboardSyncError, ClipboardSyncFacade, DispatchEntryInput,
    DispatchEntryOutcome, DispatchEntryPerTarget, InboundAction, InboundNotice, IngestHandle,
};
pub use device::{DeviceFacade, DeviceFacadeError, LocalDeviceInfoView};
pub use lifecycle::{
    LifecycleFacade, LifecycleFacadeDeps, LifecycleFacadeError, LifecycleStateView,
    LifecycleStatusGateway,
};
pub use roster::{
    ContentTypesPatch, ContentTypesView, MemberRosterDeps, MemberRosterFacade, MemberSummary,
    MemberSyncPreferencesPatch, MemberSyncPreferencesView, PresenceEvent, RosterEntry, RosterError,
};
pub use settings::{
    ContentTypesPatch as SettingsContentTypesPatch, ContentTypesView as SettingsContentTypesView,
    FileSyncSettingsPatch, FileSyncSettingsView, GeneralSettingsPatch, GeneralSettingsView,
    PairingSettingsPatch, PairingSettingsView, RetentionPolicyPatch, RetentionPolicyView,
    RetentionRulePatchValue, RetentionRuleView, RuleEvaluationView, SecuritySettingsPatch,
    SecuritySettingsView, SettingsFacade, SettingsFacadeError, SettingsPatch, SettingsView,
    ShortcutKeyView, SyncFrequencyView, SyncSettingsPatch, SyncSettingsView, ThemeView,
    UpdateChannelView,
};
pub use setup_status::SetupStatusFacade;
pub use space_setup::{
    CancelInvitationError, CurrentInvitation, InitializeSpaceCommand, InitializeSpaceError,
    InitializeSpaceResult, IssuePairingInvitationError, IssuePairingInvitationResult,
    PairingOutcome, QuerySetupStateError, RedeemPairingInvitationCommand,
    RedeemPairingInvitationError, RedeemPairingInvitationResult, ResetSpaceError, SetupStateView,
    SpaceSetupDeps, SpaceSetupFacade, UnlockSpaceCommand, UnlockSpaceError, UnlockSpaceResult,
};
pub use storage::{
    ClearCacheResultView, StorageFacade, StorageFacadeDeps, StorageFacadeError, StorageStatsView,
};
