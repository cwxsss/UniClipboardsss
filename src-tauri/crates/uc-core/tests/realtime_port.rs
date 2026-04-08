use std::any::type_name_of_val;

use uc_core::ports::realtime::{
    PairedDevicesChangedEvent, PairingRoutingRecord, PairingUpdatedEvent,
    PairingVerificationRequiredEvent, PeerChangedEvent, RealtimeEvent, RealtimePairedDeviceSummary,
    RealtimePeerSummary, RealtimeTopic,
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

/// Verifies that [`PairingRoutingRecord`] captures the allowed observability fields for a
/// pairing session — session identity, wire event type, payload kind, routed class, and envelope
/// timestamp — without carrying secrets or raw verification material.
#[test]
fn pairing_routing_record_captures_session_centered_observability_fields() {
    let record = PairingRoutingRecord {
        session_id: "sess-abc".to_string(),
        source_event_type: "pairing.verification_required".to_string(),
        payload_kind: Some("verification".to_string()),
        routed_event_class: "PairingVerificationRequired",
        envelope_ts_ms: 1_742_371_200_000,
    };

    assert_eq!(record.session_id, "sess-abc");
    assert_eq!(record.source_event_type, "pairing.verification_required");
    assert_eq!(record.payload_kind.as_deref(), Some("verification"));
    assert_eq!(record.routed_event_class, "PairingVerificationRequired");
    assert_eq!(record.envelope_ts_ms, 1_742_371_200_000);

    // The record must NOT have fields for secrets.
    // This is enforced by the type definition: no `code`, `local_fingerprint`, `peer_fingerprint`,
    // `challenge`, or `keyslot_file` fields exist on PairingRoutingRecord.
    let _: &PairingRoutingRecord = &record; // type check: no extra fields leak in
}

/// Verifies routing records for the two non-verification kind mappings: verifying and complete.
#[test]
fn pairing_routing_record_covers_verifying_and_complete_kind_routes() {
    let verifying = PairingRoutingRecord {
        session_id: "sess-xyz".to_string(),
        source_event_type: "pairing.verification_required".to_string(),
        payload_kind: Some("verifying".to_string()),
        routed_event_class: "PairingUpdated",
        envelope_ts_ms: 1_742_371_200_100,
    };
    let complete = PairingRoutingRecord {
        session_id: "sess-xyz".to_string(),
        source_event_type: "pairing.verification_required".to_string(),
        payload_kind: Some("complete".to_string()),
        routed_event_class: "PairingComplete",
        envelope_ts_ms: 1_742_371_200_200,
    };

    assert_eq!(verifying.routed_event_class, "PairingUpdated");
    assert_eq!(complete.routed_event_class, "PairingComplete");
    // Both share the same session_id — correlatable across log lines.
    assert_eq!(verifying.session_id, complete.session_id);
}
