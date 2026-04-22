//! Slice 2 Phase 2 · T7 — `DispatchClipboardEntryUseCase`.
//!
//! Encrypts one clipboard plaintext payload via [`TransferCipherPort`] and
//! fans it out to every **online** member (excluding self) on the
//! clipboard ALPN. Failure per target is isolated in the per-target report
//! so a single unreachable peer never blocks the rest of the roster.
//!
//! ## Inputs, not side-effects
//!
//! This use case takes a [`DispatchClipboardEntryInput`] — plaintext bytes
//! + `content_hash` + `payload_version`. Reading the system clipboard +
//! building the `ClipboardBinaryPayload` is the caller's responsibility
//! (CLI `send` / `watch` in T11, daemon in Phase 3). Keeping the
//! plaintext-production step outside keeps the use case testable with
//! deterministic bytes.
//!
//! ## Iteration source
//!
//! Follows the `EnsureReachableAllUseCase` pattern (T6 / Phase 1):
//! `peer_addr_repo.list()` is the authoritative roster of "members we
//! have an address blob for" and avoids iterating ghost entries in
//! `member_repo` that never completed pairing. Online filter is
//! `PresencePort::current_state == Online` — `ensure_reachable_all` is
//! already queued at F1 `auto_start_network` so the cache is warm.
//!
//! ## Concurrency
//!
//! `tokio::task::JoinSet` per target. Phase 1's mockall-Mutex lesson
//! applies (see plan §10) — the tests here use a hand-written fake
//! dispatch port to avoid serialising concurrent probe calls through
//! `mockall::Mutex<FnMut>`.

use std::sync::Arc;

use bytes::Bytes;
use tokio::task::JoinSet;
use tracing::{debug, info, instrument, warn};

use uc_core::ids::DeviceId;
use uc_core::ports::security::TransferCipherPort;
use uc_core::ports::{
    ClipboardDispatchError, ClipboardDispatchPort, ClipboardHeader, ClockPort, DeviceIdentityPort,
    DispatchAck, LocalIdentityPort, PeerAddressRepositoryPort, PresencePort, ReachabilityState,
    SettingsPort, SyncPayload,
};

/// Input to one dispatch pass. The caller owns the plaintext →
/// `ClipboardBinaryPayload` → bytes pipeline.
#[derive(Debug, Clone)]
pub(crate) struct DispatchClipboardEntryInput {
    /// Unencrypted payload bytes. Typically the postcard-encoded
    /// `ClipboardBinaryPayload` (V3) the caller built from the system
    /// clipboard snapshot.
    pub plaintext: Bytes,
    /// SHA256 hex of the plaintext above. Receiver uses this for dedup.
    pub content_hash: String,
    /// Payload codec tag, e.g. `3` for the V3 `ClipboardBinaryPayload`.
    pub payload_version: u8,
}

/// One target's dispatch result. `Ok` + `DispatchAck` when the peer
/// accepted or duplicate-ignored the frame; `Err` when the wire boundary
/// rejected or the peer is offline.
#[derive(Debug, Clone)]
pub(crate) struct DispatchPerTarget {
    pub device_id: DeviceId,
    pub outcome: Result<DispatchAck, String>,
}

/// Aggregated per-pass outcome. `total_accepted` counts peers that
/// returned `Accepted` (the ones whose repos now carry the new entry);
/// `total_offline` counts peers the dispatch port reported as unreachable.
#[derive(Debug, Clone)]
pub(crate) struct DispatchOutcome {
    pub content_hash: String,
    pub per_target: Vec<DispatchPerTarget>,
    pub total_accepted: usize,
    pub total_duplicate: usize,
    pub total_offline: usize,
    pub total_errored: usize,
    pub at_ms: i64,
}

/// Fatal errors that abort the whole pass. Per-peer failures land in
/// `per_target`; they are not errors in this sense.
#[derive(Debug, thiserror::Error)]
pub(crate) enum DispatchSyncError {
    /// Encryption failed — typically because the space session is locked.
    #[error("encryption session not unlocked")]
    LockedSpace,
    /// Encryption failed for any other reason.
    #[error("transfer cipher failure: {0}")]
    CipherFailure(String),
    /// Listing the peer address repository failed.
    #[error("peer_addr_repo.list: {0}")]
    Repository(String),
    /// Local identity lookup failed (rare — the identity should be
    /// available by the time the CLI reaches `send`).
    #[error("local identity lookup: {0}")]
    LocalIdentity(String),
}

pub(crate) struct DispatchClipboardEntryUseCase {
    peer_addr_repo: Arc<dyn PeerAddressRepositoryPort>,
    presence: Arc<dyn PresencePort>,
    transfer_cipher: Arc<dyn TransferCipherPort>,
    clipboard_dispatch: Arc<dyn ClipboardDispatchPort>,
    device_identity: Arc<dyn DeviceIdentityPort>,
    local_identity: Arc<dyn LocalIdentityPort>,
    settings: Arc<dyn SettingsPort>,
    clock: Arc<dyn ClockPort>,
}

impl DispatchClipboardEntryUseCase {
    pub(crate) fn new(
        peer_addr_repo: Arc<dyn PeerAddressRepositoryPort>,
        presence: Arc<dyn PresencePort>,
        transfer_cipher: Arc<dyn TransferCipherPort>,
        clipboard_dispatch: Arc<dyn ClipboardDispatchPort>,
        device_identity: Arc<dyn DeviceIdentityPort>,
        local_identity: Arc<dyn LocalIdentityPort>,
        settings: Arc<dyn SettingsPort>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        Self {
            peer_addr_repo,
            presence,
            transfer_cipher,
            clipboard_dispatch,
            device_identity,
            local_identity,
            settings,
            clock,
        }
    }

    #[instrument(skip_all, fields(content_hash = %input.content_hash))]
    pub(crate) async fn execute(
        &self,
        input: DispatchClipboardEntryInput,
    ) -> Result<DispatchOutcome, DispatchSyncError> {
        // 1. Encrypt. A locked session surfaces here — let it short-circuit
        //    so we don't spam the dispatch wire with encrypt-retries.
        let ciphertext = match self.transfer_cipher.encrypt(&input.plaintext).await {
            Ok(bytes) => Bytes::from(bytes),
            Err(err) => {
                return Err(match err {
                    uc_core::ports::security::TransferCipherError::NotUnlocked => {
                        DispatchSyncError::LockedSpace
                    }
                    other => DispatchSyncError::CipherFailure(other.to_string()),
                });
            }
        };

        // 2. Enumerate targets. `peer_addr_repo.list()` is the iteration
        //    source (see module doc); filter self + Online-only.
        let records =
            self.peer_addr_repo.list().await.map_err(|err| {
                DispatchSyncError::Repository(format!("peer_addr_repo.list: {err}"))
            })?;

        let local_device = self.device_identity.current_device_id();
        let mut candidates: Vec<DeviceId> = Vec::with_capacity(records.len());
        for record in records {
            if record.device_id == local_device {
                continue;
            }
            if self.presence.current_state(&record.device_id).await == ReachabilityState::Online {
                candidates.push(record.device_id);
            }
        }

        // 3. Build the header once and clone per target.
        let origin_device_name = self.load_origin_device_name().await;
        let header = ClipboardHeader {
            version: ClipboardHeader::CURRENT_VERSION,
            content_hash: input.content_hash.clone(),
            captured_at_ms: self.clock.now_ms(),
            origin_device_id: local_device.as_str().to_string(),
            origin_device_name,
            payload_version: input.payload_version,
        };

        if candidates.is_empty() {
            info!("dispatch: no online peers; skipping fan-out");
            return Ok(DispatchOutcome {
                content_hash: input.content_hash,
                per_target: Vec::new(),
                total_accepted: 0,
                total_duplicate: 0,
                total_offline: 0,
                total_errored: 0,
                at_ms: self.clock.now_ms(),
            });
        }

        // 4. Fan-out. One JoinSet task per target; results merged at the end.
        let mut set: JoinSet<(DeviceId, Result<DispatchAck, ClipboardDispatchError>)> =
            JoinSet::new();
        for device_id in &candidates {
            let dispatch = Arc::clone(&self.clipboard_dispatch);
            let header = header.clone();
            let device_id = device_id.clone();
            let payload = SyncPayload {
                ciphertext: ciphertext.clone(),
            };
            set.spawn(async move {
                let result = dispatch.dispatch(&device_id, &header, payload).await;
                (device_id, result)
            });
        }

        let mut per_target = Vec::with_capacity(candidates.len());
        let mut total_accepted = 0;
        let mut total_duplicate = 0;
        let mut total_offline = 0;
        let mut total_errored = 0;

        while let Some(joined) = set.join_next().await {
            match joined {
                Ok((device_id, Ok(DispatchAck::Accepted))) => {
                    total_accepted += 1;
                    debug!(device_id = %device_id.as_str(), "dispatch → Accepted");
                    per_target.push(DispatchPerTarget {
                        device_id,
                        outcome: Ok(DispatchAck::Accepted),
                    });
                }
                Ok((device_id, Ok(DispatchAck::DuplicateIgnored))) => {
                    total_duplicate += 1;
                    debug!(device_id = %device_id.as_str(), "dispatch → DuplicateIgnored");
                    per_target.push(DispatchPerTarget {
                        device_id,
                        outcome: Ok(DispatchAck::DuplicateIgnored),
                    });
                }
                Ok((device_id, Err(ClipboardDispatchError::Offline))) => {
                    total_offline += 1;
                    warn!(device_id = %device_id.as_str(), "dispatch → Offline");
                    per_target.push(DispatchPerTarget {
                        device_id,
                        outcome: Err("offline".to_string()),
                    });
                }
                Ok((device_id, Err(err))) => {
                    total_errored += 1;
                    warn!(device_id = %device_id.as_str(), error = %err, "dispatch failed");
                    per_target.push(DispatchPerTarget {
                        device_id,
                        outcome: Err(err.to_string()),
                    });
                }
                Err(err) => {
                    total_errored += 1;
                    warn!(error = %err, "dispatch task panicked or cancelled");
                }
            }
        }

        Ok(DispatchOutcome {
            content_hash: input.content_hash,
            per_target,
            total_accepted,
            total_duplicate,
            total_offline,
            total_errored,
            at_ms: self.clock.now_ms(),
        })
    }

    /// Load the device's own display name to embed in the outbound header
    /// so the peer can show "from <Alice's Laptop>". Falls back to the
    /// fingerprint if settings are unreadable or empty.
    async fn load_origin_device_name(&self) -> String {
        match self.settings.load().await {
            Ok(settings) => {
                if let Some(name) = settings.general.device_name {
                    if !name.is_empty() {
                        return name;
                    }
                }
            }
            Err(err) => {
                warn!(error = %err, "dispatch: settings load failed; using fingerprint fallback");
            }
        }
        match self.local_identity.get_current_fingerprint().await {
            Ok(Some(fp)) => fp.as_display().to_string(),
            _ => "unknown-device".to_string(),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use async_trait::async_trait;
    use chrono::Utc;
    use tokio::sync::{broadcast, Mutex};

    use uc_core::ports::security::{TransferCipherError, TransferCipherPort};
    use uc_core::ports::{
        ClockPort, LocalIdentityError, LocalIdentityPort, PeerAddressError, PeerAddressRecord,
        PeerAddressRepositoryPort, PresenceError, PresenceEvent, PresencePort, ReachabilityState,
        SettingsPort,
    };
    use uc_core::security::IdentityFingerprint;
    use uc_core::settings::model::Settings;

    // ----- port doubles ------------------------------------------------------

    #[derive(Default)]
    struct MemPeerAddrRepo {
        inner: Mutex<HashMap<String, PeerAddressRecord>>,
    }
    #[async_trait]
    impl PeerAddressRepositoryPort for MemPeerAddrRepo {
        async fn get(
            &self,
            device: &DeviceId,
        ) -> Result<Option<PeerAddressRecord>, PeerAddressError> {
            Ok(self.inner.lock().await.get(device.as_str()).cloned())
        }
        async fn upsert(&self, record: &PeerAddressRecord) -> Result<(), PeerAddressError> {
            self.inner
                .lock()
                .await
                .insert(record.device_id.as_str().to_string(), record.clone());
            Ok(())
        }
        async fn list(&self) -> Result<Vec<PeerAddressRecord>, PeerAddressError> {
            Ok(self.inner.lock().await.values().cloned().collect())
        }
        async fn remove(&self, device: &DeviceId) -> Result<(), PeerAddressError> {
            self.inner.lock().await.remove(device.as_str());
            Ok(())
        }
    }

    /// Fingerprint lookup via explicit state map. Keyed by device_id string
    /// so the test can flip peers between Online / Offline without
    /// touching iroh.
    struct StaticPresence {
        states: HashMap<String, ReachabilityState>,
    }

    #[async_trait]
    impl PresencePort for StaticPresence {
        async fn ensure_reachable(
            &self,
            _device: &DeviceId,
        ) -> Result<ReachabilityState, PresenceError> {
            unreachable!("dispatch use case does not call ensure_reachable")
        }

        async fn current_state(&self, device: &DeviceId) -> ReachabilityState {
            self.states
                .get(device.as_str())
                .copied()
                .unwrap_or(ReachabilityState::Unknown)
        }

        fn subscribe(&self) -> broadcast::Receiver<PresenceEvent> {
            let (tx, rx) = broadcast::channel(1);
            std::mem::forget(tx);
            rx
        }
    }

    struct FixedDeviceIdentity(DeviceId);
    impl uc_core::ports::DeviceIdentityPort for FixedDeviceIdentity {
        fn current_device_id(&self) -> DeviceId {
            self.0.clone()
        }
    }

    struct StubLocalIdentity(Option<IdentityFingerprint>);
    #[async_trait]
    impl LocalIdentityPort for StubLocalIdentity {
        async fn create(&self) -> Result<IdentityFingerprint, LocalIdentityError> {
            Ok(self.0.clone().expect("identity present"))
        }
        async fn ensure(&self) -> Result<IdentityFingerprint, LocalIdentityError> {
            Ok(self.0.clone().expect("identity present"))
        }
        async fn get_current_fingerprint(
            &self,
        ) -> Result<Option<IdentityFingerprint>, LocalIdentityError> {
            Ok(self.0.clone())
        }
    }

    struct StubSettings {
        settings: Settings,
    }
    #[async_trait]
    impl SettingsPort for StubSettings {
        async fn load(&self) -> anyhow::Result<Settings> {
            Ok(self.settings.clone())
        }
        async fn save(&self, _s: &Settings) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct FixedClock(i64);
    impl ClockPort for FixedClock {
        fn now_ms(&self) -> i64 {
            self.0
        }
    }

    /// Cipher double. Emits a sentinel-prefixed ciphertext so the test can
    /// assert on ordering, or errors out on demand.
    struct OkCipher;
    #[async_trait]
    impl TransferCipherPort for OkCipher {
        async fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, TransferCipherError> {
            let mut out = b"CIPH".to_vec();
            out.extend_from_slice(plaintext);
            Ok(out)
        }
        async fn decrypt(&self, _: &[u8]) -> Result<Vec<u8>, TransferCipherError> {
            Err(TransferCipherError::InvalidFormat)
        }
    }

    struct LockedCipher;
    #[async_trait]
    impl TransferCipherPort for LockedCipher {
        async fn encrypt(&self, _: &[u8]) -> Result<Vec<u8>, TransferCipherError> {
            Err(TransferCipherError::NotUnlocked)
        }
        async fn decrypt(&self, _: &[u8]) -> Result<Vec<u8>, TransferCipherError> {
            Err(TransferCipherError::NotUnlocked)
        }
    }

    /// Dispatch double. Each device_id has a recipe specifying the
    /// simulated outcome. Tracks call count per target for assertions.
    #[derive(Clone)]
    enum DispatchRecipe {
        Ok(DispatchAck),
        Offline,
        PeerRejected(String),
    }

    #[derive(Default)]
    struct RecipeDispatch {
        recipes: Mutex<HashMap<String, DispatchRecipe>>,
        call_counter: AtomicUsize,
    }

    impl RecipeDispatch {
        async fn set(&self, device_id: &str, recipe: DispatchRecipe) {
            self.recipes
                .lock()
                .await
                .insert(device_id.to_string(), recipe);
        }
    }

    #[async_trait]
    impl ClipboardDispatchPort for RecipeDispatch {
        async fn dispatch(
            &self,
            target: &DeviceId,
            _header: &ClipboardHeader,
            _payload: SyncPayload,
        ) -> Result<DispatchAck, ClipboardDispatchError> {
            self.call_counter.fetch_add(1, Ordering::SeqCst);
            let recipe = self
                .recipes
                .lock()
                .await
                .get(target.as_str())
                .cloned()
                .unwrap_or(DispatchRecipe::Offline);
            match recipe {
                DispatchRecipe::Ok(ack) => Ok(ack),
                DispatchRecipe::Offline => Err(ClipboardDispatchError::Offline),
                DispatchRecipe::PeerRejected(msg) => Err(ClipboardDispatchError::PeerRejected(msg)),
            }
        }
    }

    // ----- fixture builders --------------------------------------------------

    fn fp(seed: u8) -> IdentityFingerprint {
        IdentityFingerprint::from_raw_string(
            (0..16)
                .map(|i| char::from(b'A' + ((seed as usize + i) % 26) as u8))
                .collect::<String>(),
        )
        .expect("valid fingerprint")
    }

    async fn make_repo_with_peers(peers: &[&str]) -> Arc<MemPeerAddrRepo> {
        let repo = Arc::new(MemPeerAddrRepo::default());
        for p in peers {
            repo.upsert(&PeerAddressRecord {
                device_id: DeviceId::new(*p),
                addr_blob: vec![0xAA; 32],
                observed_at: Utc::now(),
            })
            .await
            .unwrap();
        }
        repo
    }

    fn make_uc(
        peer_addr_repo: Arc<dyn PeerAddressRepositoryPort>,
        presence: Arc<dyn PresencePort>,
        cipher: Arc<dyn TransferCipherPort>,
        dispatch: Arc<dyn ClipboardDispatchPort>,
        local_device: &str,
    ) -> DispatchClipboardEntryUseCase {
        let mut settings = Settings::default();
        settings.general.device_name = Some("Test Device".to_string());

        DispatchClipboardEntryUseCase::new(
            peer_addr_repo,
            presence,
            cipher,
            dispatch,
            Arc::new(FixedDeviceIdentity(DeviceId::new(local_device))),
            Arc::new(StubLocalIdentity(Some(fp(7)))),
            Arc::new(StubSettings { settings }),
            Arc::new(FixedClock(1_700_000_000_000)),
        )
    }

    fn input() -> DispatchClipboardEntryInput {
        DispatchClipboardEntryInput {
            plaintext: Bytes::from_static(b"hello world"),
            content_hash: "9".repeat(64),
            payload_version: 3,
        }
    }

    // ----- verdicts ----------------------------------------------------------

    /// 1. Happy path — two online peers, both accept. Report mirrors the
    /// dispatch adapter's `Accepted` ack.
    #[tokio::test]
    async fn fan_outs_to_all_online_peers_and_counts_accepted() {
        let peer_addr_repo = make_repo_with_peers(&["peer-a", "peer-b"]).await;
        let presence = Arc::new(StaticPresence {
            states: [
                ("peer-a".to_string(), ReachabilityState::Online),
                ("peer-b".to_string(), ReachabilityState::Online),
            ]
            .into_iter()
            .collect(),
        });
        let dispatch = Arc::new(RecipeDispatch::default());
        dispatch
            .set("peer-a", DispatchRecipe::Ok(DispatchAck::Accepted))
            .await;
        dispatch
            .set("peer-b", DispatchRecipe::Ok(DispatchAck::Accepted))
            .await;

        let uc = make_uc(
            peer_addr_repo,
            presence,
            Arc::new(OkCipher),
            dispatch.clone(),
            "self-device",
        );

        let outcome = uc.execute(input()).await.expect("dispatch ok");
        assert_eq!(outcome.total_accepted, 2);
        assert_eq!(outcome.total_offline, 0);
        assert_eq!(outcome.total_errored, 0);
        assert_eq!(outcome.per_target.len(), 2);
        assert_eq!(dispatch.call_counter.load(Ordering::SeqCst), 2);
    }

    /// 2. Offline filter — one peer is Offline, dispatch port is not even
    /// called for it. The remaining online peer is reported normally.
    #[tokio::test]
    async fn skips_offline_peers_without_invoking_dispatch_port() {
        let peer_addr_repo = make_repo_with_peers(&["peer-on", "peer-off"]).await;
        let presence = Arc::new(StaticPresence {
            states: [
                ("peer-on".to_string(), ReachabilityState::Online),
                ("peer-off".to_string(), ReachabilityState::Offline),
            ]
            .into_iter()
            .collect(),
        });
        let dispatch = Arc::new(RecipeDispatch::default());
        dispatch
            .set("peer-on", DispatchRecipe::Ok(DispatchAck::Accepted))
            .await;

        let uc = make_uc(
            peer_addr_repo,
            presence,
            Arc::new(OkCipher),
            dispatch.clone(),
            "self-device",
        );

        let outcome = uc.execute(input()).await.expect("dispatch ok");
        assert_eq!(outcome.total_accepted, 1);
        assert_eq!(outcome.per_target.len(), 1);
        assert_eq!(outcome.per_target[0].device_id.as_str(), "peer-on");
        // dispatch port only called for the online peer
        assert_eq!(dispatch.call_counter.load(Ordering::SeqCst), 1);
    }

    /// 3. Self-filter — `peer_addr_repo` inadvertently contains the local
    /// device. The use case must skip it (defensive, mirrors the Phase 1
    /// self-filter in `EnsureReachableAllUseCase`).
    #[tokio::test]
    async fn skips_self_even_if_peer_addr_repo_contains_local_device() {
        let peer_addr_repo = make_repo_with_peers(&["self-device", "peer-a"]).await;
        let presence = Arc::new(StaticPresence {
            states: [
                ("self-device".to_string(), ReachabilityState::Online),
                ("peer-a".to_string(), ReachabilityState::Online),
            ]
            .into_iter()
            .collect(),
        });
        let dispatch = Arc::new(RecipeDispatch::default());
        dispatch
            .set("peer-a", DispatchRecipe::Ok(DispatchAck::Accepted))
            .await;
        // If self were not filtered, dispatch to "self-device" would fall
        // through to the default Offline recipe → outcome would show
        // total_offline: 1. The assertion below proves it's not the case.

        let uc = make_uc(
            peer_addr_repo,
            presence,
            Arc::new(OkCipher),
            dispatch.clone(),
            "self-device",
        );

        let outcome = uc.execute(input()).await.expect("dispatch ok");
        assert_eq!(outcome.per_target.len(), 1);
        assert_eq!(outcome.per_target[0].device_id.as_str(), "peer-a");
    }

    /// 4. Locked space — `transfer_cipher.encrypt` returns `NotUnlocked`.
    /// The use case short-circuits with `LockedSpace` before touching the
    /// dispatch port (asserted by verifying call counter is zero).
    #[tokio::test]
    async fn locked_space_short_circuits_before_dispatch() {
        let peer_addr_repo = make_repo_with_peers(&["peer-a"]).await;
        let presence = Arc::new(StaticPresence {
            states: [("peer-a".to_string(), ReachabilityState::Online)]
                .into_iter()
                .collect(),
        });
        let dispatch = Arc::new(RecipeDispatch::default());

        let uc = make_uc(
            peer_addr_repo,
            presence,
            Arc::new(LockedCipher),
            dispatch.clone(),
            "self-device",
        );

        let err = uc
            .execute(input())
            .await
            .expect_err("locked space must short-circuit");
        assert!(matches!(err, DispatchSyncError::LockedSpace));
        assert_eq!(dispatch.call_counter.load(Ordering::SeqCst), 0);
    }

    /// 5. Mixed outcomes — one accept, one offline, one rejected. Report
    /// counts all three buckets correctly and preserves per-target
    /// granularity.
    #[tokio::test]
    async fn mixed_per_target_outcomes_are_reported_independently() {
        let peer_addr_repo = make_repo_with_peers(&["peer-ok", "peer-off", "peer-rej"]).await;
        let presence = Arc::new(StaticPresence {
            states: [
                ("peer-ok".to_string(), ReachabilityState::Online),
                ("peer-off".to_string(), ReachabilityState::Online),
                ("peer-rej".to_string(), ReachabilityState::Online),
            ]
            .into_iter()
            .collect(),
        });
        let dispatch = Arc::new(RecipeDispatch::default());
        dispatch
            .set("peer-ok", DispatchRecipe::Ok(DispatchAck::Accepted))
            .await;
        dispatch.set("peer-off", DispatchRecipe::Offline).await;
        dispatch
            .set(
                "peer-rej",
                DispatchRecipe::PeerRejected("too big".to_string()),
            )
            .await;

        let uc = make_uc(
            peer_addr_repo,
            presence,
            Arc::new(OkCipher),
            dispatch.clone(),
            "self-device",
        );

        let outcome = uc.execute(input()).await.expect("dispatch ok");
        assert_eq!(outcome.total_accepted, 1);
        assert_eq!(outcome.total_offline, 1);
        assert_eq!(outcome.total_errored, 1);
        assert_eq!(outcome.per_target.len(), 3);

        // Confirm each target's outcome kind.
        use std::collections::HashSet;
        let seen: HashSet<(String, String)> = outcome
            .per_target
            .iter()
            .map(|t| {
                let key = match &t.outcome {
                    Ok(DispatchAck::Accepted) => "accepted",
                    Ok(DispatchAck::DuplicateIgnored) => "duplicate",
                    Err(msg) if msg == "offline" => "offline",
                    Err(_) => "rejected",
                };
                (t.device_id.as_str().to_string(), key.to_string())
            })
            .collect();
        assert!(seen.contains(&("peer-ok".to_string(), "accepted".to_string())));
        assert!(seen.contains(&("peer-off".to_string(), "offline".to_string())));
        assert!(seen.contains(&("peer-rej".to_string(), "rejected".to_string())));
    }

    // Guard: the tests touch only in-memory doubles; no runtime pauses
    // longer than a tick, so the default `#[tokio::test]` runtime is fine.
    // Keep this marker so future agents notice the assumption.
    const _: Duration = Duration::from_millis(0);
}
