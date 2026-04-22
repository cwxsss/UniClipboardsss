//! Slice 2 Phase 2 · T8 — `IngestInboundClipboardUseCase`.
//!
//! Subscribes to [`ClipboardReceiverPort`] once, then drives a background
//! loop that decrypts each inbound frame via [`TransferCipherPort`] and
//! re-emits an application-level [`InboundClipboardNotice`] on its own
//! broadcast channel. The notice carries the **decrypted plaintext** so
//! downstream consumers (CLI `watch`, Phase 3 daemon) can persist to
//! `ClipboardEventWriterPort` / write the system clipboard without
//! re-deriving the cipher key.
//!
//! Phase 2 is intentionally thin:
//! * No local persistence — the receiver broadcasts plaintext + metadata;
//!   the CLI `watch` command prints it (§5.3 of the plan intentionally
//!   decoupled system-clipboard write from ingest to avoid daemon
//!   collision).
//! * No dedup — duplicate content at the source is already short-circuited
//!   by the receiver adapter's ack boundary (Accepted vs DuplicateIgnored);
//!   the application layer merely reports what the wire said.
//!
//! Failure handling:
//! * Decrypt error → log + skip. The connection is already closed by the
//!   receiver adapter; retrying has no effect.
//! * Receiver lagged → log; next frame catches up.
//! * Receiver closed (all senders dropped, i.e. adapter shutdown) → loop
//!   exits cleanly; the [`IngestSpawnHandle`] resolves its join handle.

use std::sync::Arc;

use bytes::Bytes;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::{debug, info, instrument, warn};

use uc_core::ids::DeviceId;
use uc_core::ports::security::TransferCipherPort;
use uc_core::ports::{ClipboardReceiverPort, ClockPort};

/// Application-layer view of one decrypted inbound clipboard payload.
#[derive(Debug, Clone)]
pub(crate) struct InboundClipboardNotice {
    pub from_device: DeviceId,
    pub content_hash: String,
    pub plaintext: Bytes,
    pub action: InboundAction,
    pub at_ms: i64,
}

/// What the ingest path did with the inbound frame. Phase 2 only emits
/// [`InboundAction::NewEntry`]; [`InboundAction::DuplicateIgnored`] is
/// reserved for Phase 3 when local persistence dedup lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InboundAction {
    NewEntry,
    DuplicateIgnored,
}

const NOTICE_CHANNEL_CAPACITY: usize = 64;

pub(crate) struct IngestInboundClipboardUseCase {
    receiver: Arc<dyn ClipboardReceiverPort>,
    transfer_cipher: Arc<dyn TransferCipherPort>,
    notices_tx: broadcast::Sender<InboundClipboardNotice>,
    clock: Arc<dyn ClockPort>,
}

/// Handle returned by [`IngestInboundClipboardUseCase::spawn_run`]. Drop
/// or `abort()` to stop the loop; the cleanup is also automatic when the
/// receiver adapter shuts down (its broadcast senders drop).
pub(crate) struct IngestSpawnHandle {
    join: JoinHandle<()>,
}

impl IngestSpawnHandle {
    pub fn abort(&self) {
        self.join.abort();
    }
}

impl Drop for IngestSpawnHandle {
    fn drop(&mut self) {
        self.join.abort();
    }
}

impl IngestInboundClipboardUseCase {
    pub(crate) fn new(
        receiver: Arc<dyn ClipboardReceiverPort>,
        transfer_cipher: Arc<dyn TransferCipherPort>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        let (notices_tx, _) = broadcast::channel(NOTICE_CHANNEL_CAPACITY);
        Self {
            receiver,
            transfer_cipher,
            notices_tx,
            clock,
        }
    }

    /// Subscribe to the application-level notice stream. Multiple callers
    /// may subscribe; lagging receivers drop frames per broadcast semantics.
    pub(crate) fn subscribe_notices(&self) -> broadcast::Receiver<InboundClipboardNotice> {
        self.notices_tx.subscribe()
    }

    /// Spawn the ingest loop. Takes `Arc<Self>` so the spawned task can
    /// hold the use case's dependencies without moving them out of the
    /// owning facade.
    pub(crate) fn spawn_run(self: Arc<Self>) -> IngestSpawnHandle {
        let uc = Arc::clone(&self);
        let join = tokio::spawn(async move { uc.run().await });
        IngestSpawnHandle { join }
    }

    #[instrument(skip_all)]
    async fn run(self: Arc<Self>) {
        let mut rx = self.receiver.subscribe();
        loop {
            match rx.recv().await {
                Ok(inbound) => {
                    self.handle_one(inbound).await;
                }
                Err(broadcast::error::RecvError::Lagged(missed)) => {
                    warn!(missed, "ingest receiver lagged; dropped frames");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    info!("ingest receiver closed; exiting loop");
                    break;
                }
            }
        }
    }

    async fn handle_one(&self, inbound: uc_core::ports::InboundClipboard) {
        let plaintext = match self.transfer_cipher.decrypt(&inbound.ciphertext).await {
            Ok(bytes) => Bytes::from(bytes),
            Err(err) => {
                warn!(
                    peer = %inbound.peer_device_id.as_str(),
                    content_hash = %inbound.header.content_hash,
                    error = %err,
                    "ingest: decrypt failed; dropping frame"
                );
                return;
            }
        };
        let notice = InboundClipboardNotice {
            from_device: inbound.peer_device_id.clone(),
            content_hash: inbound.header.content_hash.clone(),
            plaintext,
            action: InboundAction::NewEntry,
            at_ms: self.clock.now_ms(),
        };
        if self.notices_tx.send(notice).is_err() {
            debug!(
                peer = %inbound.peer_device_id.as_str(),
                "ingest: no notice subscribers; frame dropped"
            );
        }
    }
}

// ============================================================================
// Tests
// ============================================================================
//
// **Mocking convention** (see also `dispatch_entry.rs::tests`):
//
// * `TransferCipherPort` is mocked via `mockall::mock!` — all four tests
//   need different `decrypt` recipes (echo / fail / decryption error),
//   and mockall lets each test register exactly the calls it expects.
// * `ClipboardReceiverPort` stays a hand-written `FakeReceiver` because
//   the tests need to drive `subscribe()` + emit on demand. The Phase 1
//   roster facade hand-wrote `FakePresence` for the same reason
//   (broadcast emit ergonomics — see `roster/facade.rs::tests`).
// * `ClockPort` stays the trivial `FixedClock` (4 lines) — same call as
//   Phase 1 ensure_reachable_all.

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Duration;

    use async_trait::async_trait;

    use uc_core::ports::security::{TransferCipherError, TransferCipherPort};
    use uc_core::ports::{ClipboardHeader, ClockPort, InboundClipboard};

    // ── hand-written fake: ClipboardReceiverPort ────────────────────────
    //
    // mockall would require a `subscribe()` returning a one-shot
    // `broadcast::Receiver` — possible but awkward (the receiver isn't
    // Clone, so a reusable mock would need `Mutex<Option<Receiver>>`).
    // The hand-written fake is shorter and exposes a `publish(...)`
    // helper the tests use to drive the loop. Same trade-off as Phase
    // 1's `FakePresence`.

    struct FakeReceiver {
        tx: broadcast::Sender<InboundClipboard>,
    }

    impl FakeReceiver {
        fn new() -> Self {
            let (tx, _) = broadcast::channel(32);
            Self { tx }
        }
        fn publish(&self, inbound: InboundClipboard) {
            let _ = self.tx.send(inbound);
        }
    }

    #[async_trait]
    impl ClipboardReceiverPort for FakeReceiver {
        fn subscribe(&self) -> broadcast::Receiver<InboundClipboard> {
            self.tx.subscribe()
        }
    }

    // ── mockall: TransferCipherPort ─────────────────────────────────────

    mockall::mock! {
        pub Cipher {}

        #[async_trait]
        impl TransferCipherPort for Cipher {
            async fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, TransferCipherError>;
            async fn decrypt(&self, encrypted: &[u8]) -> Result<Vec<u8>, TransferCipherError>;
        }
    }

    // ── trivial: ClockPort ──────────────────────────────────────────────

    struct FixedClock(i64);
    impl ClockPort for FixedClock {
        fn now_ms(&self) -> i64 {
            self.0
        }
    }

    fn inbound_fixture(peer: &str, content_hash: &str, ciphertext: Bytes) -> InboundClipboard {
        InboundClipboard {
            peer_device_id: DeviceId::new(peer),
            header: ClipboardHeader {
                version: ClipboardHeader::CURRENT_VERSION,
                content_hash: content_hash.to_string(),
                captured_at_ms: 1_700_000_000_000,
                origin_device_id: peer.to_string(),
                origin_device_name: format!("Device {peer}"),
                payload_version: 3,
            },
            ciphertext,
        }
    }

    /// 1. Happy path — one inbound, decrypt succeeds, notice arrives on
    /// the broadcast with `NewEntry` action and the decrypted plaintext.
    /// mockall asserts decrypt is called exactly once with the expected
    /// ciphertext bytes.
    #[tokio::test]
    async fn forwards_decrypted_inbound_as_notice() {
        let receiver = Arc::new(FakeReceiver::new());
        let mut cipher = MockCipher::new();
        // Decrypt is called once with the inbound's ciphertext; return a
        // recognisable plaintext so the assertion checks both the call
        // and the round-trip.
        cipher.expect_decrypt().times(1).returning(|ct| {
            Ok(if ct.starts_with(b"CIPH") {
                ct[4..].to_vec()
            } else {
                ct.to_vec()
            })
        });

        let uc = Arc::new(IngestInboundClipboardUseCase::new(
            receiver.clone(),
            Arc::new(cipher),
            Arc::new(FixedClock(42)),
        ));
        let mut notice_rx = uc.subscribe_notices();
        let _handle = Arc::clone(&uc).spawn_run();

        // Give the spawned task a tick to subscribe before publishing.
        tokio::time::sleep(Duration::from_millis(20)).await;

        receiver.publish(inbound_fixture(
            "peer-1",
            "0".repeat(64).as_str(),
            Bytes::from(b"CIPHhello".to_vec()),
        ));

        let notice = tokio::time::timeout(Duration::from_secs(2), notice_rx.recv())
            .await
            .expect("notice arrives")
            .expect("sender alive");
        assert_eq!(notice.from_device.as_str(), "peer-1");
        assert_eq!(notice.content_hash, "0".repeat(64));
        assert_eq!(notice.plaintext, Bytes::from_static(b"hello"));
        assert_eq!(notice.action, InboundAction::NewEntry);
        assert_eq!(notice.at_ms, 42);
    }

    /// 2. Decrypt failure — no notice is emitted; the ingest loop keeps
    /// running. mockall asserts the decrypt failure is the only invocation.
    #[tokio::test]
    async fn skips_frame_when_decrypt_fails_but_keeps_loop_running() {
        let receiver = Arc::new(FakeReceiver::new());
        let mut cipher = MockCipher::new();
        cipher
            .expect_decrypt()
            .times(1)
            .returning(|_| Err(TransferCipherError::DecryptionFailed));

        let uc = Arc::new(IngestInboundClipboardUseCase::new(
            receiver.clone(),
            Arc::new(cipher),
            Arc::new(FixedClock(0)),
        ));
        let mut notice_rx = uc.subscribe_notices();
        let _handle = Arc::clone(&uc).spawn_run();

        tokio::time::sleep(Duration::from_millis(20)).await;

        receiver.publish(inbound_fixture(
            "peer-broken",
            "f".repeat(64).as_str(),
            Bytes::from_static(b"broken"),
        ));

        let fast_poll = tokio::time::timeout(Duration::from_millis(200), notice_rx.recv()).await;
        assert!(fast_poll.is_err(), "decrypt failure must not emit a notice");
    }

    /// 3. Three inbounds — publish three frames in quick succession; the
    /// loop processes each one. mockall asserts decrypt is called
    /// exactly three times.
    #[tokio::test]
    async fn forwards_multiple_inbounds_in_order() {
        let receiver = Arc::new(FakeReceiver::new());
        let mut cipher = MockCipher::new();
        cipher
            .expect_decrypt()
            .times(3)
            .returning(|ct| Ok(ct.to_vec()));

        let uc = Arc::new(IngestInboundClipboardUseCase::new(
            receiver.clone(),
            Arc::new(cipher),
            Arc::new(FixedClock(100)),
        ));
        let mut notice_rx = uc.subscribe_notices();
        let _handle = Arc::clone(&uc).spawn_run();

        tokio::time::sleep(Duration::from_millis(20)).await;

        for i in 0..3 {
            receiver.publish(inbound_fixture(
                &format!("peer-{i}"),
                &format!("{i}").repeat(64),
                Bytes::from(format!("m-{i}").into_bytes()),
            ));
        }

        let mut seen = Vec::new();
        for _ in 0..3 {
            let notice = tokio::time::timeout(Duration::from_secs(2), notice_rx.recv())
                .await
                .expect("notice arrives")
                .expect("sender alive");
            seen.push(notice.from_device.as_str().to_string());
        }
        seen.sort();
        assert_eq!(seen, vec!["peer-0", "peer-1", "peer-2"]);
    }

    /// 4. Handle abort cleanly stops the loop. The drop impl aborts the
    /// task; the post-abort publish does not reach decrypt (mockall would
    /// panic at drop if a second decrypt slipped through).
    #[tokio::test]
    async fn abort_stops_loop_without_emitting_further_notices() {
        let receiver = Arc::new(FakeReceiver::new());
        let mut cipher = MockCipher::new();
        // Exactly one decrypt call expected — for the pre-abort publish.
        // The post-abort publish must NOT reach the cipher; mockall's
        // `.times(1)` enforces this on Drop.
        cipher.expect_decrypt().times(1).returning(|ct| {
            Ok(if ct.starts_with(b"CIPH") {
                ct[4..].to_vec()
            } else {
                ct.to_vec()
            })
        });

        let uc = Arc::new(IngestInboundClipboardUseCase::new(
            receiver.clone(),
            Arc::new(cipher),
            Arc::new(FixedClock(0)),
        ));
        let mut notice_rx = uc.subscribe_notices();
        let handle = Arc::clone(&uc).spawn_run();

        tokio::time::sleep(Duration::from_millis(20)).await;

        // Publish one, observe, then abort.
        receiver.publish(inbound_fixture(
            "peer-a",
            "a".repeat(64).as_str(),
            Bytes::from_static(b"CIPHfirst"),
        ));
        let _first = tokio::time::timeout(Duration::from_secs(2), notice_rx.recv())
            .await
            .expect("first notice arrives")
            .expect("sender alive");

        handle.abort();
        // Allow abort to settle.
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Publish another — the loop is gone, decrypt must not be called.
        receiver.publish(inbound_fixture(
            "peer-b",
            "b".repeat(64).as_str(),
            Bytes::from_static(b"CIPHsecond"),
        ));
        let fast_poll = tokio::time::timeout(Duration::from_millis(200), notice_rx.recv()).await;
        assert!(
            fast_poll.is_err(),
            "loop must be stopped after handle.abort()"
        );
    }
}
