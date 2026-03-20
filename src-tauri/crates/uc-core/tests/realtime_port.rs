use std::any::type_name_of_val;

use uc_core::ports::realtime::{
    PlaceholderPayload, RealtimeEvent, RealtimeFrontendEvent, RealtimeFrontendPayload,
    RealtimeTopic, FRONTEND_REALTIME_EVENT,
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
        RealtimeEvent::PairingUpdated(PlaceholderPayload),
        RealtimeEvent::PairingVerificationRequired(PlaceholderPayload),
        RealtimeEvent::PairingFailed(PlaceholderPayload),
        RealtimeEvent::PairingComplete(PlaceholderPayload),
        RealtimeEvent::PeersChanged(PlaceholderPayload),
        RealtimeEvent::PeersNameUpdated(PlaceholderPayload),
        RealtimeEvent::PeersConnectionChanged(PlaceholderPayload),
        RealtimeEvent::PairedDevicesChanged(PlaceholderPayload),
    ];

    let payload_types = events
        .iter()
        .map(|event| match event {
            RealtimeEvent::PairingUpdated(payload)
            | RealtimeEvent::PairingVerificationRequired(payload)
            | RealtimeEvent::PairingFailed(payload)
            | RealtimeEvent::PairingComplete(payload)
            | RealtimeEvent::PeersChanged(payload)
            | RealtimeEvent::PeersNameUpdated(payload)
            | RealtimeEvent::PeersConnectionChanged(payload)
            | RealtimeEvent::PairedDevicesChanged(payload) => type_name_of_val(payload),
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
        "pairing.verification_required",
        1_731_234_567,
        RealtimeFrontendPayload::Placeholder(PlaceholderPayload),
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
    assert_ne!(event.event_type(), "peers.changed");
}

#[test]
fn realtime_port_contract_is_not_finalized() {
    realtime_event_variants_cover_pairing_peers_and_paired_devices();
    realtime_frontend_event_uses_daemon_realtime_contract();
}
