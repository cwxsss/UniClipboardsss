use uc_core::{DeviceId, FileTransferEvent};

use crate::{
    announce_input, build_context, progress_input, published_events, start_input, transfer_history,
};

#[tokio::test]
async fn start_transfer_persists_and_publishes_started_event() {
    let ctx = build_context();

    let announced = ctx
        .announce_transfer
        .execute(announce_input("transfer-1", "device-1"))
        .await
        .unwrap();
    let event = ctx
        .start_transfer
        .execute(start_input("transfer-1", "peer-1"))
        .await
        .unwrap();

    assert_eq!(
        event,
        FileTransferEvent::started("transfer-1", "peer-1", "report.pdf", Some(128))
    );
    assert_eq!(
        transfer_history(&ctx, "transfer-1").await,
        vec![announced.clone(), event.clone()]
    );
    assert_eq!(published_events(&ctx), vec![announced, event]);
}

#[tokio::test]
async fn start_transfer_rejects_duplicate_start_after_existing_history() {
    let ctx = build_context();

    ctx.announce_transfer
        .execute(announce_input("transfer-1", "device-1"))
        .await
        .unwrap();
    ctx.start_transfer
        .execute(start_input("transfer-1", "peer-1"))
        .await
        .unwrap();

    let error = ctx
        .start_transfer
        .execute(start_input("transfer-1", "peer-1"))
        .await
        .unwrap_err();

    assert_eq!(
        error,
        uc_application::file_transfer::FileTransferApplicationError::TransferAlreadyStarted {
            transfer_id: "transfer-1".into(),
        }
    );
    assert_eq!(transfer_history(&ctx, "transfer-1").await.len(), 2);
    assert_eq!(published_events(&ctx).len(), 2);
}

#[tokio::test]
async fn start_transfer_keeps_other_transfer_history_isolated() {
    let ctx = build_context();

    ctx.announce_transfer
        .execute(announce_input("transfer-1", "device-1"))
        .await
        .unwrap();
    ctx.announce_transfer
        .execute(announce_input("transfer-2", "device-2"))
        .await
        .unwrap();
    ctx.start_transfer
        .execute(start_input("transfer-1", "peer-1"))
        .await
        .unwrap();
    ctx.start_transfer
        .execute(start_input("transfer-2", "peer-2"))
        .await
        .unwrap();
    ctx.report_progress
        .execute(progress_input("transfer-2", "peer-2", 64))
        .await
        .unwrap();

    assert_eq!(transfer_history(&ctx, "transfer-1").await.len(), 2);
    assert_eq!(transfer_history(&ctx, "transfer-2").await.len(), 3);
    assert_eq!(published_events(&ctx).len(), 5);
}

#[tokio::test]
async fn start_transfer_allows_unknown_file_size() {
    let ctx = build_context();

    let announced = ctx
        .announce_transfer
        .execute(uc_application::file_transfer::AnnounceTransfer {
            transfer_id: "transfer-unknown".into(),
            origin_device_id: DeviceId::new("device-1"),
            filename: "report.pdf".into(),
            file_size: None,
        })
        .await
        .unwrap();
    let event = ctx
        .start_transfer
        .execute(uc_application::file_transfer::StartTransfer {
            transfer_id: "transfer-unknown".into(),
            peer_id: "peer-1".into(),
            filename: "report.pdf".into(),
            file_size: None,
        })
        .await
        .unwrap();

    assert_eq!(
        event,
        FileTransferEvent::started("transfer-unknown", "peer-1", "report.pdf", None)
    );
    assert_eq!(
        transfer_history(&ctx, "transfer-unknown").await,
        vec![announced.clone(), event.clone()]
    );
    assert_eq!(published_events(&ctx), vec![announced, event]);
}

#[tokio::test]
async fn start_transfer_requires_announcement_first() {
    let ctx = build_context();

    let error = ctx
        .start_transfer
        .execute(start_input("transfer-1", "peer-1"))
        .await
        .unwrap_err();

    assert_eq!(
        error,
        uc_application::file_transfer::FileTransferApplicationError::TransferNotAnnounced {
            transfer_id: "transfer-1".into(),
        }
    );
    assert!(transfer_history(&ctx, "transfer-1").await.is_empty());
    assert!(published_events(&ctx).is_empty());
}
