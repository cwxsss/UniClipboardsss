use uc_core::{DeviceId, FileTransferEvent};

use crate::{announce_input, build_context, published_events, transfer_history};

#[tokio::test]
async fn announce_transfer_persists_and_publishes_announced_event() {
    let ctx = build_context();

    let event = ctx
        .announce_transfer
        .execute(announce_input("transfer-1", "device-1"))
        .await
        .unwrap();

    assert_eq!(
        event,
        FileTransferEvent::announced(
            "transfer-1",
            DeviceId::new("device-1"),
            "report.pdf",
            Some(128),
        )
    );
    assert_eq!(
        transfer_history(&ctx, "transfer-1").await,
        vec![event.clone()]
    );
    assert_eq!(published_events(&ctx), vec![event]);
}

#[tokio::test]
async fn announce_transfer_allows_unknown_file_size() {
    let ctx = build_context();

    let event = ctx
        .announce_transfer
        .execute(uc_application::file_transfer::AnnounceTransfer {
            transfer_id: "transfer-unknown".into(),
            origin_device_id: DeviceId::new("device-1"),
            filename: "report.pdf".into(),
            file_size: None,
        })
        .await
        .unwrap();

    assert_eq!(
        event,
        FileTransferEvent::announced(
            "transfer-unknown",
            DeviceId::new("device-1"),
            "report.pdf",
            None,
        )
    );
    assert_eq!(
        transfer_history(&ctx, "transfer-unknown").await,
        vec![event.clone()]
    );
    assert_eq!(published_events(&ctx), vec![event]);
}
