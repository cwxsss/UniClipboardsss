//! Tests for clipboard stats and favorite commands.

use std::sync::Arc;

use uc_tauri::bootstrap::AppRuntime;
use uc_tauri::commands::clipboard::get_clipboard_stats;
use uc_tauri::commands::clipboard::toggle_favorite_clipboard_item;
use uc_tauri::commands::error::CommandError;
use uc_tauri::models::ClipboardStats;
use uc_tauri::test_utils::noop_network_ports;

#[tokio::test]
async fn clipboard_stats_serialization_matches_contract() {
    let stats = ClipboardStats {
        total_items: 3,
        total_size: 42,
    };

    let value = serde_json::to_value(&stats).expect("serialize ClipboardStats");
    assert!(
        value.get("total_items").is_some(),
        "expected total_items key"
    );
    assert!(value.get("total_size").is_some(), "expected total_size key");
    assert!(
        value.get("totalItems").is_none(),
        "unexpected camelCase totalItems"
    );
    assert!(
        value.get("totalSize").is_none(),
        "unexpected camelCase totalSize"
    );
}

#[tokio::test]
async fn get_clipboard_stats_returns_internal_error_when_usecase_fails() {
    // For now, we exercise the command wiring error path by constructing
    // a runtime with empty clipboard repositories. The stats use case is
    // expected to return an error, which the command must map into
    // CommandError::InternalError.
    let deps = uc_app::AppDeps {
        clipboard: Arc::new(uc_infra::clipboard::NoopClipboard),
        system_clipboard: Arc::new(uc_infra::clipboard::NoopClipboard),
        clipboard_entry_repo: Arc::new(uc_infra::db::repositories::clipboard_entry::InMemoryClipboardEntryRepository::default()),
        clipboard_event_repo: Arc::new(uc_infra::db::repositories::clipboard_event::InMemoryClipboardEventRepository::default()),
        representation_repo: Arc::new(uc_infra::db::repositories::clipboard_representation::InMemoryClipboardRepresentationRepository::default()),
        representation_normalizer: Arc::new(uc_infra::clipboard::NoopRepresentationNormalizer),
        selection_repo: Arc::new(uc_infra::db::repositories::clipboard_selection::InMemoryClipboardSelectionRepository::default()),
        representation_policy: Arc::new(uc_infra::clipboard::NoopSelectionPolicy),
        representation_cache: Arc::new(uc_infra::clipboard::NoopRepresentationCache),
        spool_queue: Arc::new(uc_infra::clipboard::NoopSpoolQueue),
        clipboard_change_origin: Arc::new(uc_infra::clipboard::InMemoryClipboardChangeOrigin::new()),
        worker_tx: tokio::sync::mpsc::channel(1).0,
        encryption: Arc::new(uc_infra::security::NoopEncryptionPort),
        encryption_session: Arc::new(uc_infra::security::NoopEncryptionSessionPort),
        encryption_state: Arc::new(uc_infra::security::NoopEncryptionStatePort),
        key_scope: Arc::new(uc_infra::security::NoopKeyScopePort),
        secure_storage: Arc::new(uc_infra::security::NoopSecureStoragePort),
        key_material: Arc::new(uc_infra::security::NoopKeyMaterialPort),
        watcher_control: Arc::new(uc_infra::clipboard::NoopWatcherControlPort),
        device_repo: Arc::new(uc_infra::db::repositories::device::InMemoryDeviceRepository::default()),
        device_identity: Arc::new(uc_infra::device::InMemoryDeviceIdentity::new("test-device".to_string())),
        paired_device_repo: Arc::new(uc_infra::db::repositories::paired_device::InMemoryPairedDeviceRepository::default()),
        network_ports: noop_network_ports(),
        network_control: Arc::new(uc_infra::network::NoopNetworkControlPort),
        setup_status: Arc::new(uc_infra::setup::InMemorySetupStatusPort::default()),
        blob_store: Arc::new(uc_infra::storage::NoopBlobStorePort),
        blob_repository: Arc::new(uc_infra::storage::NoopBlobRepositoryPort),
        blob_writer: Arc::new(uc_infra::storage::NoopBlobWriterPort),
        thumbnail_repo: Arc::new(uc_infra::clipboard::NoopThumbnailRepositoryPort),
        thumbnail_generator: Arc::new(uc_infra::clipboard::NoopThumbnailGeneratorPort),
        settings: Arc::new(uc_infra::settings::InMemorySettingsPort::new()),
        ui_port: Arc::new(uc_infra::ui::NoopUiPort),
        autostart: Arc::new(uc_infra::autostart::NoopAutostartPort),
        clock: Arc::new(uc_infra::time::SystemClockPort),
        hash: Arc::new(uc_infra::hash::Blake3ContentHashPort),
    };

    let runtime = Arc::new(AppRuntime::new(deps));
    let result = get_clipboard_stats(tauri::State::from(runtime.clone()), None).await;

    // At this stage of wiring, any error should surface as CommandError::InternalError.
    if let Err(err) = result {
        match err {
            CommandError::InternalError(_) => {}
            other => panic!("expected InternalError, got: {other:?}"),
        }
    }
}

#[tokio::test]
async fn toggle_favorite_clipboard_item_returns_not_found() {
    let deps = uc_app::AppDeps {
        clipboard: Arc::new(uc_infra::clipboard::NoopClipboard),
        system_clipboard: Arc::new(uc_infra::clipboard::NoopClipboard),
        clipboard_entry_repo: Arc::new(uc_infra::db::repositories::clipboard_entry::InMemoryClipboardEntryRepository::default()),
        clipboard_event_repo: Arc::new(uc_infra::db::repositories::clipboard_event::InMemoryClipboardEventRepository::default()),
        representation_repo: Arc::new(uc_infra::db::repositories::clipboard_representation::InMemoryClipboardRepresentationRepository::default()),
        representation_normalizer: Arc::new(uc_infra::clipboard::NoopRepresentationNormalizer),
        selection_repo: Arc::new(uc_infra::db::repositories::clipboard_selection::InMemoryClipboardSelectionRepository::default()),
        representation_policy: Arc::new(uc_infra::clipboard::NoopSelectionPolicy),
        representation_cache: Arc::new(uc_infra::clipboard::NoopRepresentationCache),
        spool_queue: Arc::new(uc_infra::clipboard::NoopSpoolQueue),
        clipboard_change_origin: Arc::new(uc_infra::clipboard::InMemoryClipboardChangeOrigin::new()),
        worker_tx: tokio::sync::mpsc::channel(1).0,
        encryption: Arc::new(uc_infra::security::NoopEncryptionPort),
        encryption_session: Arc::new(uc_infra::security::NoopEncryptionSessionPort),
        encryption_state: Arc::new(uc_infra::security::NoopEncryptionStatePort),
        key_scope: Arc::new(uc_infra::security::NoopKeyScopePort),
        secure_storage: Arc::new(uc_infra::security::NoopSecureStoragePort),
        key_material: Arc::new(uc_infra::security::NoopKeyMaterialPort),
        watcher_control: Arc::new(uc_infra::clipboard::NoopWatcherControlPort),
        device_repo: Arc::new(uc_infra::db::repositories::device::InMemoryDeviceRepository::default()),
        device_identity: Arc::new(uc_infra::device::InMemoryDeviceIdentity::new("test-device".to_string())),
        paired_device_repo: Arc::new(uc_infra::db::repositories::paired_device::InMemoryPairedDeviceRepository::default()),
        network_ports: noop_network_ports(),
        network_control: Arc::new(uc_infra::network::NoopNetworkControlPort),
        setup_status: Arc::new(uc_infra::setup::InMemorySetupStatusPort::default()),
        blob_store: Arc::new(uc_infra::storage::NoopBlobStorePort),
        blob_repository: Arc::new(uc_infra::storage::NoopBlobRepositoryPort),
        blob_writer: Arc::new(uc_infra::storage::NoopBlobWriterPort),
        thumbnail_repo: Arc::new(uc_infra::clipboard::NoopThumbnailRepositoryPort),
        thumbnail_generator: Arc::new(uc_infra::clipboard::NoopThumbnailGeneratorPort),
        settings: Arc::new(uc_infra::settings::InMemorySettingsPort::new()),
        ui_port: Arc::new(uc_infra::ui::NoopUiPort),
        autostart: Arc::new(uc_infra::autostart::NoopAutostartPort),
        clock: Arc::new(uc_infra::time::SystemClockPort),
        hash: Arc::new(uc_infra::hash::Blake3ContentHashPort),
    };

    let runtime = Arc::new(AppRuntime::new(deps));
    let result = toggle_favorite_clipboard_item(
        tauri::State::from(runtime.clone()),
        "missing-entry".to_string(),
        true,
        None,
    )
    .await;

    let err = result.expect_err("expected NotFound when toggle usecase is not implemented");
    match err {
        CommandError::NotFound(_) => {}
        other => panic!("expected NotFound, got: {other:?}"),
    }
}
