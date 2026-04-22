//! Helpers for building the [`EndpointAddr`] handed to
//! [`iroh::Endpoint::connect`] from a stored [`PeerAddressRecord`] blob.
//!
//! ## Why strip stored IP addresses
//!
//! `peer_addr_repo` stores the peer's `EndpointAddr` as a postcard blob at
//! pairing time and never updates it. The blob's `TransportAddr::Ip(...)`
//! entries therefore freeze the peer's UDP socket address *at that
//! moment* — including the **magicsock-assigned random port**. Every
//! subsequent daemon restart on the peer binds a fresh random port, which
//! means:
//!
//! * The stored LAN IP `192.168.31.224:50754` becomes
//!   `192.168.31.224:<new random>` after peer restart → packets sent to
//!   the stored port land on a closed socket, the kernel silently drops
//!   them, and iroh's dial sits waiting for a QUIC handshake that never
//!   comes. 30-second dial timeout observed in real-device logs.
//! * The stored public IP is similarly bound to a port that's almost
//!   certainly NAT-remapped now.
//! * The stored TUN IP (e.g. `198.18.0.1:50754`) was never reachable by
//!   anyone outside the peer's machine.
//!
//! Iroh's built-in pkarr discovery service *does* resolve the peer's
//! currently-published `EndpointAddr` on every connect attempt — but
//! those results are *merged* with whatever direct addresses we pass in.
//! Keeping the stored `Ip(...)` entries means iroh still races them
//! against the fresh pkarr data, burning dial budget on dead ports.
//!
//! ## What we keep
//!
//! We keep the stored [`TransportAddr::Relay`] url as a fallback hint —
//! if the peer's current pkarr record doesn't mention a relay (e.g. the
//! peer only just booted and discovery hasn't propagated), the stored
//! relay gives iroh something to try while it waits for pkarr. Relay
//! URLs don't rotate per-process the way UDP ports do.
//!
//! ## What we drop
//!
//! All [`TransportAddr::Ip`] entries from the stored blob — but **only**
//! if the blob also carries a [`TransportAddr::Relay`]. Without a relay
//! fallback, dropping direct addrs leaves iroh with nothing to try
//! (discovery won't help if relay isn't configured either); unit tests
//! that bind endpoints with `RelayMode::Disabled` rely on this
//! no-relay-means-keep-directs branch. Production pairing-time blobs
//! always carry a relay (iroh's default `RelayMode::Default` picks one
//! during `endpoint.addr()`), so the strip triggers where it matters.
//! [`TransportAddr::Custom`] entries are never dropped — we don't use
//! them today and a future caller should decide per-variant.

use iroh::{EndpointAddr, TransportAddr};

/// Strip direct IP entries from a stored endpoint address when a relay
/// fallback is also present. Callers hand the result to
/// `Endpoint::connect`; iroh's discovery service then fills in fresh
/// direct addresses from the peer's current pkarr publish. When no
/// relay is in the blob, the input is returned unchanged so iroh still
/// has direct addrs to race against — test fixtures depend on this.
pub fn strip_stale_direct_addrs(addr: EndpointAddr) -> EndpointAddr {
    let has_relay = addr
        .addrs
        .iter()
        .any(|a| matches!(a, TransportAddr::Relay(_)));
    if !has_relay {
        return addr;
    }

    let EndpointAddr { id, addrs } = addr;
    let kept = addrs
        .into_iter()
        .filter(|addr| !matches!(addr, TransportAddr::Ip(_)));
    EndpointAddr::from_parts(id, kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

    use iroh::{EndpointId, RelayUrl, SecretKey};

    fn test_id() -> EndpointId {
        SecretKey::generate(&mut rand::rng()).public()
    }

    #[test]
    fn drops_ip_entries_keeps_relay() {
        let id = test_id();
        let relay: RelayUrl = "https://relay.example.com/".parse().unwrap();
        let lan: SocketAddr =
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 5), 50754));
        let wan: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(1, 2, 3, 4), 59875));

        let full = EndpointAddr::from_parts(
            id,
            [
                TransportAddr::Ip(lan),
                TransportAddr::Ip(wan),
                TransportAddr::Relay(relay.clone()),
            ],
        );

        let stripped = strip_stale_direct_addrs(full);
        assert_eq!(stripped.id, id);
        assert_eq!(stripped.addrs.len(), 1);
        assert!(stripped
            .addrs
            .iter()
            .all(|a| matches!(a, TransportAddr::Relay(_))));
    }

    #[test]
    fn id_only_input_is_unchanged() {
        let id = test_id();
        let addr = EndpointAddr::new(id);
        let stripped = strip_stale_direct_addrs(addr);
        assert_eq!(stripped.id, id);
        assert!(stripped.addrs.is_empty());
    }

    #[test]
    fn ip_only_input_is_kept_intact() {
        // No relay present → no discovery fallback to rely on → keep the
        // IPs as the only dialable paths. Mirrors unit-test fixtures that
        // bind endpoints with `RelayMode::Disabled`.
        let id = test_id();
        let lan: SocketAddr =
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 5), 50754));
        let addr = EndpointAddr::from_parts(id, [TransportAddr::Ip(lan)]);
        let stripped = strip_stale_direct_addrs(addr);
        assert_eq!(stripped.id, id);
        assert_eq!(stripped.addrs.len(), 1);
        assert!(stripped
            .addrs
            .iter()
            .all(|a| matches!(a, TransportAddr::Ip(_))));
    }
}
