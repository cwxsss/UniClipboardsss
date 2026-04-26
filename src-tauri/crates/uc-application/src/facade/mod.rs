//! Slice 1 application facade tree.
//!
//! Per `uc-application/AGENTS.md` §11.4 external consumers only see the
//! top-level `AppFacade` and the per-domain sub-facades it aggregates.
//! Use cases live under `crate::usecases::<domain>` and stay `pub(crate)`;
//! sub-facades expose them through domain-scoped methods.

pub mod app_facade;
pub mod blob_transfer;
pub mod clipboard;
pub mod clipboard_restore;
pub mod device;
pub mod encryption;
pub mod lifecycle;
pub mod resource;
pub mod roster;
pub mod settings;
pub mod setup_status;
pub mod space_setup;
pub mod storage;

pub use app_facade::{AppFacade, AppFacadeParts};
pub use blob_transfer::{
    BlobTransferDeps, BlobTransferError, BlobTransferFacade, FetchBlobCommand, FetchBlobResult,
    PublishBlobCommand, PublishBlobResult,
};
pub use clipboard::{
    ClipboardSyncDeps, ClipboardSyncError, ClipboardSyncFacade, DispatchEntryInput,
    DispatchEntryOutcome, DispatchEntryPerTarget, InboundAction, InboundNotice, IngestHandle,
};
pub use clipboard_restore::{
    ClipboardRestoreError, ClipboardRestoreFacade, ClipboardRestoreGateway,
};
pub use device::{DeviceFacade, DeviceFacadeError, LocalDeviceInfoView};
pub use encryption::{
    EncryptionFacade, EncryptionFacadeDeps, EncryptionFacadeError, EncryptionStateView,
};
pub use lifecycle::{
    LifecycleFacade, LifecycleFacadeDeps, LifecycleFacadeError, LifecycleStateView,
    LifecycleStatusGateway,
};
pub use resource::{BinaryResourceView, ResourceFacade, ResourceFacadeDeps, ResourceFacadeError};
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
    CancelInvitationError, CurrentInvitation, InitializeSpaceError, InitializeSpaceInput,
    InitializeSpaceResult, IssuePairingInvitationError, IssuePairingInvitationResult,
    PairingOutcome, QuerySetupStateError, RedeemPairingInvitationError,
    RedeemPairingInvitationInput, RedeemPairingInvitationResult, ResetSpaceError, SetupStateView,
    SpaceSetupDeps, SpaceSetupFacade, UnlockSpaceError, UnlockSpaceInput, UnlockSpaceResult,
};
pub use storage::{
    ClearCacheResultView, StorageFacade, StorageFacadeDeps, StorageFacadeError, StorageStatsView,
};
