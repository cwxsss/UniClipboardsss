use std::any::type_name_of_val;

use uc_core::ports::realtime::{
    PairedDevicesChangedEvent, PairingUpdatedEvent, PairingVerificationRequiredEvent,
    PeerChangedEvent, RealtimeEvent, RealtimePairedDeviceSummary, RealtimePeerSummary,
    RealtimeTopic,
};

#[test]
fn realtime_port_topic_set_is_stable() {
    let topics = [
        RealtimeTopic::Pairing,
        RealtimeTopic::Peers,
        RealtimeTopic::PairedDevices,
    ];

    assert_eq!(topics.len(), 3);
}

#[test]
fn realtime_event_variants_cover_pairing_peers_and_paired_devices() {
    let events = [
        RealtimeEvent::PairingUpdated(PairingUpdatedEvent {
            session_id: "session-1".into(),
            status: "awaitingVerification".into(),
            peer_id: Some("peer-1".into()),
            device_name: Some("Desk".into()),
        }),
        RealtimeEvent::PairingVerificationRequired(PairingVerificationRequiredEvent {
            session_id: "session-1".into(),
            peer_id: Some("peer-1".into()),
            device_name: Some("Desk".into()),
            code: Some("123456".into()),
            local_fingerprint: Some("local".into()),
            peer_fingerprint: Some("peer".into()),
        }),
        RealtimeEvent::PairingFailed(uc_core::ports::realtime::PairingFailedEvent {
            session_id: "session-1".into(),
            reason: "cancelled".into(),
        }),
        RealtimeEvent::PairingComplete(uc_core::ports::realtime::PairingCompleteEvent {
            session_id: "session-1".into(),
            peer_id: Some("peer-1".into()),
            device_name: Some("Desk".into()),
        }),
        RealtimeEvent::PeersChanged(PeerChangedEvent {
            peers: vec![RealtimePeerSummary {
                peer_id: "peer-1".into(),
                device_name: Some("Desk".into()),
                connected: true,
            }],
        }),
        RealtimeEvent::PeersNameUpdated(uc_core::ports::realtime::PeerNameUpdatedEvent {
            peer_id: "peer-1".into(),
            device_name: "Desk".into(),
        }),
        RealtimeEvent::PeersConnectionChanged(
            uc_core::ports::realtime::PeerConnectionChangedEvent {
                peer_id: "peer-1".into(),
                connected: true,
                device_name: Some("Desk".into()),
            },
        ),
        RealtimeEvent::PairedDevicesChanged(PairedDevicesChangedEvent {
            devices: vec![RealtimePairedDeviceSummary {
                device_id: "device-1".into(),
                device_name: "Desk".into(),
                last_seen_ts: Some(1_731_234_567),
            }],
        }),
    ];

    let payload_types = events
        .iter()
        .map(|event| match event {
            RealtimeEvent::PairingUpdated(payload) => type_name_of_val(payload),
            RealtimeEvent::PairingVerificationRequired(payload) => type_name_of_val(payload),
            RealtimeEvent::PairingFailed(payload) => type_name_of_val(payload),
            RealtimeEvent::PairingComplete(payload) => type_name_of_val(payload),
            RealtimeEvent::PeersChanged(payload) => type_name_of_val(payload),
            RealtimeEvent::PeersNameUpdated(payload) => type_name_of_val(payload),
            RealtimeEvent::PeersConnectionChanged(payload) => type_name_of_val(payload),
            RealtimeEvent::PairedDevicesChanged(payload) => type_name_of_val(payload),
            other => panic!("unexpected RealtimeEvent variant: {other:?}"),
        })
        .collect::<Vec<_>>();

    assert!(
        payload_types
            .iter()
            .any(|name| name.ends_with("PairingUpdatedEvent")),
        "expected a typed pairing.updated payload, got {payload_types:?}"
    );
    assert!(
        payload_types
            .iter()
            .any(|name| name.ends_with("PeerChangedEvent")),
        "expected a typed peers.changed payload, got {payload_types:?}"
    );
    assert!(
        payload_types
            .iter()
            .any(|name| name.ends_with("PairedDevicesChangedEvent")),
        "expected a typed paired-devices.changed payload, got {payload_types:?}"
    );
}
