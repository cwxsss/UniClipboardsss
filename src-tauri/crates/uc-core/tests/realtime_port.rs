use std::any::type_name_of_val;

use uc_core::ports::realtime::{
    PairedDevicesChangedEvent, PairingUpdatedEvent, PairingVerificationRequiredEvent,
    PeerChangedEvent, RealtimeEvent, RealtimeFrontendEvent, RealtimeFrontendPayload,
    RealtimePairedDeviceSummary, RealtimePeerSummary, RealtimeTopic, FRONTEND_REALTIME_EVENT,
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

#[test]
fn realtime_frontend_event_uses_daemon_realtime_contract() {
    let event = RealtimeFrontendEvent::new(
        RealtimeTopic::Pairing,
        "pairing.verificationRequired",
        1_731_234_567,
        RealtimeFrontendPayload::PairingVerificationRequired(PairingVerificationRequiredEvent {
            session_id: "session-1".into(),
            peer_id: Some("peer-1".into()),
            device_name: Some("Desk".into()),
            code: Some("123456".into()),
            local_fingerprint: Some("local".into()),
            peer_fingerprint: Some("peer".into()),
        }),
    );

    assert_eq!(FRONTEND_REALTIME_EVENT, "daemon://realtime");

    let debug = format!("{event:?}");
    assert!(
        debug.contains("topic:"),
        "expected debug output to expose topic field, got {debug}"
    );
    assert!(
        debug.contains("type:"),
        "expected debug output to expose type field, got {debug}"
    );
    assert!(
        debug.contains("ts:"),
        "expected debug output to expose ts field, got {debug}"
    );
    assert!(
        debug.contains("payload:"),
        "expected debug output to expose payload field, got {debug}"
    );
    assert_eq!(event.event_type(), "pairing.verificationRequired");

    let peers_event = RealtimeFrontendEvent::new(
        RealtimeTopic::Peers,
        "peers.changed",
        1_731_234_568,
        RealtimeFrontendPayload::PeersChanged(PeerChangedEvent {
            peers: vec![RealtimePeerSummary {
                peer_id: "peer-2".into(),
                device_name: Some("Laptop".into()),
                connected: false,
            }],
        }),
    );
    assert_eq!(peers_event.event_type(), "peers.changed");
}

#[test]
fn realtime_port_contract_is_not_finalized() {
    realtime_event_variants_cover_pairing_peers_and_paired_devices();
    realtime_frontend_event_uses_daemon_realtime_contract();
}
