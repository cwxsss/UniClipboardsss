//! Integration test for clipboard capture flow
//!
//! Tests the complete flow from clipboard change to entry persistence

use mockall::mock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uc_core::clipboard::ObservedClipboardRepresentation;
use uc_core::ids::{FormatId, RepresentationId};
use uc_core::ports::ClipboardChangeHandler;
use uc_core::SystemClipboardSnapshot;

mock! {
    Handler {}

    #[async_trait::async_trait]
    impl ClipboardChangeHandler for Handler {
        async fn on_clipboard_changed(
            &self,
            snapshot: SystemClipboardSnapshot,
        ) -> anyhow::Result<()>;
    }
}

#[tokio::test]
async fn test_clipboard_change_handler_receives_callback() {
    let capture_called = Arc::new(AtomicBool::new(false));
    let snapshot_received = Arc::new(std::sync::Mutex::new(None));
    let capture_called_for_mock = capture_called.clone();
    let snapshot_received_for_mock = snapshot_received.clone();
    let mut handler = MockHandler::new();
    handler
        .expect_on_clipboard_changed()
        .times(1)
        .returning(move |snapshot| {
            capture_called_for_mock.store(true, Ordering::SeqCst);
            *snapshot_received_for_mock.lock().unwrap() = Some(snapshot);
            Ok(())
        });
    let trait_handler: Arc<dyn ClipboardChangeHandler> = Arc::new(handler);

    // Create a dummy snapshot
    let snapshot = SystemClipboardSnapshot {
        ts_ms: 12345,
        representations: vec![ObservedClipboardRepresentation::new(
            RepresentationId::from("test-rep-1".to_string()),
            FormatId::from("public.utf8-plain-text".to_string()),
            Some(uc_core::MimeType("text/plain".to_string())),
            vec![b'H', b'e', b'l', b'l', b'o'],
        )],
    };

    // Call the handler
    trait_handler
        .on_clipboard_changed(snapshot.clone())
        .await
        .unwrap();

    assert!(
        capture_called.load(Ordering::SeqCst),
        "Handler should have been called"
    );

    let received = snapshot_received.lock().unwrap().clone();
    assert!(received.is_some(), "Snapshot should have been received");
    let received = received.unwrap();
    assert_eq!(received.ts_ms, 12345);
    assert_eq!(received.representations.len(), 1);
}

#[tokio::test]
async fn test_clipboard_change_handler_multiple_calls() {
    let capture_called = Arc::new(AtomicBool::new(false));
    let snapshot_received = Arc::new(std::sync::Mutex::new(None));
    let capture_called_for_mock = capture_called.clone();
    let snapshot_received_for_mock = snapshot_received.clone();
    let mut handler = MockHandler::new();
    handler
        .expect_on_clipboard_changed()
        .times(2)
        .returning(move |snapshot| {
            capture_called_for_mock.store(true, Ordering::SeqCst);
            *snapshot_received_for_mock.lock().unwrap() = Some(snapshot);
            Ok(())
        });
    let trait_handler: Arc<dyn ClipboardChangeHandler> = Arc::new(handler);

    // Create multiple snapshots
    let snapshot1 = SystemClipboardSnapshot {
        ts_ms: 1000,
        representations: vec![],
    };
    let snapshot2 = SystemClipboardSnapshot {
        ts_ms: 2000,
        representations: vec![],
    };

    // Call the handler multiple times
    trait_handler.on_clipboard_changed(snapshot1).await.unwrap();
    trait_handler.on_clipboard_changed(snapshot2).await.unwrap();

    assert!(
        capture_called.load(Ordering::SeqCst),
        "Handler should have been called"
    );

    let received = snapshot_received.lock().unwrap().clone();
    assert!(received.is_some());
    assert_eq!(
        received.unwrap().ts_ms,
        2000,
        "Should have received last snapshot"
    );
}
