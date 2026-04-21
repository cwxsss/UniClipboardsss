//! Slice 2 Phase 1 T3a — probe iroh 0.95 `conn_type` semantics.
//!
//! Goal: confirm the `ReachabilityState` mapping rule for `IrohPresenceAdapter`
//! before writing it. Specifically we want to answer:
//!
//! 1. What does `Endpoint::conn_type(peer_id)` return for a peer we have
//!    never dialed and whose address we have never seen?
//! 2. After a successful `connect(addr, alpn)`, what `ConnectionType`
//!    variant does `Watcher::get()` report?
//! 3. When the peer endpoint is closed, how long until `Watcher::stream()`
//!    yields a transition, and what is the transitioned value?
//! 4. Does `Connection::closed().await` fire reliably when the peer
//!    shuts down (the fallback offline-detection pathway)?
//!
//! **Key finding (2026-04-20)**: `Endpoint::conn_type` **does NOT transition
//! off `Direct(...)` when the peer closes its endpoint**. It is a cache, not a
//! liveness probe. Offline detection must rely on `Connection::closed()` on
//! the held connection handle (scenario 4 below).
//!
//! This probe stays as an integration test so it doubles as regression
//! coverage. Once `IrohPresenceAdapter` lands in T3b with its own tests,
//! most scenarios here will be redundant, but the negative case
//! ("never dialed → Option::None") and the liveness-via-Connection::closed
//! assertion are cheap enough to keep.
//!
//! Runs loopback-only (relays disabled) so it has no external dependency.

use std::time::Duration;

use iroh::{Endpoint, RelayMode, Watcher};

const PROBE_ALPN: &[u8] = b"uniclipboard/presence-probe/0";

/// Bind a single endpoint with relays disabled and `PROBE_ALPN` registered.
async fn bind_endpoint() -> Endpoint {
    Endpoint::builder()
        .alpns(vec![PROBE_ALPN.to_vec()])
        .relay_mode(RelayMode::Disabled)
        .bind()
        .await
        .expect("bind endpoint")
}

/// Wait until `endpoint.addr().addrs` has published at least one direct addr.
/// Without relays the magicsock takes a handful of ms to surface loopback
/// addresses; short retry loop matches the Slice 1 pairing test fixture.
async fn wait_for_direct_addrs(endpoint: &Endpoint) {
    for _ in 0..100 {
        if !endpoint.addr().addrs.is_empty() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("endpoint never published direct addresses");
}

/// Spawn an accept loop that holds every incoming connection open until the
/// peer closes it. Returns a cancel token via `JoinHandle::abort()` drop.
fn spawn_hold_open_acceptor(endpoint: Endpoint) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(incoming) = endpoint.accept().await {
            let Ok(conn) = incoming.await else { continue };
            tokio::spawn(async move {
                // Hold until peer closes.
                let _ = conn.closed().await;
            });
        }
    })
}

#[tokio::test]
async fn conn_type_returns_none_before_any_connect() {
    let prober = bind_endpoint().await;
    let peer = bind_endpoint().await;
    let peer_id = peer.id();

    let mut watcher_opt = prober.conn_type(peer_id);

    // With no address information and no prior connection, iroh should
    // report "unknown peer". The exact shape matters for the adapter's
    // `ReachabilityState::Unknown` mapping.
    let watcher_present = watcher_opt.is_some();
    println!("pre-connect conn_type watcher present: {watcher_present}");
    if let Some(w) = watcher_opt.as_mut() {
        println!("pre-connect ConnectionType = {:?}", w.get());
    }
    // Expectation: Option::None. Fail loudly if iroh returns Some for an
    // unknown peer — the adapter's mapping rule depends on this behavior.
    assert!(
        !watcher_present,
        "expected Option::None for peer we have never contacted, got Some — revisit adapter mapping",
    );

    prober.close().await;
    peer.close().await;
}

#[tokio::test]
async fn conn_type_is_some_after_connect_and_reports_direct() {
    let peer = bind_endpoint().await;
    wait_for_direct_addrs(&peer).await;
    let peer_addr = peer.addr();
    let peer_id = peer.id();
    let acceptor = spawn_hold_open_acceptor(peer.clone());

    let prober = bind_endpoint().await;
    wait_for_direct_addrs(&prober).await;

    // Dial peer.
    let connection = prober
        .connect(peer_addr, PROBE_ALPN)
        .await
        .expect("dial peer");

    // Give magicsock a beat to finalize the direct path.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut watcher = prober
        .conn_type(peer_id)
        .expect("conn_type watcher present after connect");
    let state = watcher.get();
    println!("post-connect ConnectionType = {:?}", state);
    assert!(
        matches!(
            state,
            iroh::endpoint::ConnectionType::Direct(_)
                | iroh::endpoint::ConnectionType::Relay(_)
                | iroh::endpoint::ConnectionType::Mixed(_, _)
        ),
        "expected Direct/Relay/Mixed after successful dial, got {:?}",
        state
    );

    drop(connection);
    acceptor.abort();
    prober.close().await;
    peer.close().await;
}

/// Documents the finding that `Endpoint::conn_type` is a stale cache — it
/// keeps returning `Direct(...)` even after the peer endpoint is torn down.
/// This is what forces the adapter to track `Connection::closed()` instead.
#[tokio::test]
async fn conn_type_watcher_is_stale_after_peer_shutdown() {
    let peer = bind_endpoint().await;
    wait_for_direct_addrs(&peer).await;
    let peer_addr = peer.addr();
    let peer_id = peer.id();
    let acceptor = spawn_hold_open_acceptor(peer.clone());

    let prober = bind_endpoint().await;
    wait_for_direct_addrs(&prober).await;
    let _connection = prober
        .connect(peer_addr, PROBE_ALPN)
        .await
        .expect("dial peer");
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut watcher = prober
        .conn_type(peer_id)
        .expect("conn_type watcher present after connect");
    let before = watcher.get();

    // Shut down the peer entirely.
    acceptor.abort();
    peer.close().await;
    // Give any transition 3 seconds to land (far less than the 10s Phase 1
    // budget — if conn_type was reliable, it would transition in well under
    // 3s on loopback).
    tokio::time::sleep(Duration::from_secs(3)).await;

    let after = watcher.get();
    println!(
        "conn_type staleness: before={:?} after(3s post peer-close)={:?}",
        before, after
    );
    assert_eq!(
        before, after,
        "conn_type unexpectedly transitioned — adapter design assumption broken",
    );

    prober.close().await;
}

/// The actual fallback: `Connection::closed().await` fires when the peer
/// drops. Adapter `IrohPresenceAdapter` holds the `Connection` and awaits
/// this future in a watchdog task per tracked peer.
#[tokio::test]
async fn connection_closed_fires_when_peer_shuts_down() {
    let peer = bind_endpoint().await;
    wait_for_direct_addrs(&peer).await;
    let peer_addr = peer.addr();
    let acceptor = spawn_hold_open_acceptor(peer.clone());

    let prober = bind_endpoint().await;
    wait_for_direct_addrs(&prober).await;
    let connection = prober
        .connect(peer_addr, PROBE_ALPN)
        .await
        .expect("dial peer");
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert!(
        connection.close_reason().is_none(),
        "connection should be alive immediately after dial",
    );

    let closed_fut = connection.closed();
    let started = std::time::Instant::now();

    // Shut down the peer entirely.
    acceptor.abort();
    peer.close().await;

    // Connection::closed() must resolve within the Phase 1 budget (≤ 10s).
    match tokio::time::timeout(Duration::from_secs(10), closed_fut).await {
        Ok(reason) => {
            println!(
                "Connection::closed() fired after {:?} with reason = {:?}",
                started.elapsed(),
                reason
            );
        }
        Err(_) => panic!(
            "Connection::closed() did not fire within 10s — offline detection \
             would miss Phase 1 budget",
        ),
    }

    prober.close().await;
}
