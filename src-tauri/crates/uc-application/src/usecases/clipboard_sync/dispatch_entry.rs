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
//
// **Mocking convention** — locked in by Slice 2 Phase 1 T6 (`ensure_reachable_all`)
// and reaffirmed by Phase 2 plan §10 risk row 4:
//
// * Use `mockall::mock!` for ports whose tests assert call-count + return-
//   value behavior. Every Phase 1 use case test does this; we follow.
// * Use a hand-written fake **only** when ergonomics demand it:
//     - `subscribe()` returning a non-Clone `broadcast::Receiver` plus an
//       `emit(...)` helper to drive the test (see `roster/facade.rs` ::
//       `FakePresence` for the canonical example), or
//     - wall-time concurrency assertions where mockall's internal
//       `Mutex<FnMut>` would serialise concurrent `.returning()` closures
//       (Phase 1 T6's `SleepyPresence`).
//
// For this file: the dispatch use case calls 3 async ports + 4 read-only
// ports; no broadcast emit, no wall-time concurrency assertion. All 7
// ports are mocked with `mockall::mock!`. `PresencePort` is fully mocked
// even though `current_state` is per-device — `.expect_current_state()`
// with `.with(...)` predicates handles per-device routing cleanly.

#[cfg(test)]
mod tests {
    use super::*;

    use async_trait::async_trait;
    use chrono::Utc;
    use mockall::predicate::*;
    use tokio::sync::broadcast;

    use uc_core::ports::security::{TransferCipherError, TransferCipherPort};
    use uc_core::ports::{
        ClockPort, DeviceIdentityPort, LocalIdentityError, LocalIdentityPort, PeerAddressError,
        PeerAddressRecord, PeerAddressRepositoryPort, PresenceError, PresenceEvent, PresencePort,
        ReachabilityState, SettingsPort,
    };
    use uc_core::security::IdentityFingerprint;
    use uc_core::settings::model::Settings;

    // ── mockall: PeerAddressRepositoryPort ──────────────────────────────

    mockall::mock! {
        pub PeerAddrRepo {}

        #[async_trait]
        impl PeerAddressRepositoryPort for PeerAddrRepo {
            async fn get(
                &self,
                device: &DeviceId,
            ) -> Result<Option<PeerAddressRecord>, PeerAddressError>;
            async fn upsert(&self, record: &PeerAddressRecord) -> Result<(), PeerAddressError>;
            async fn list(&self) -> Result<Vec<PeerAddressRecord>, PeerAddressError>;
            async fn remove(&self, device: &DeviceId) -> Result<(), PeerAddressError>;
        }
    }

    // ── mockall: PresencePort ───────────────────────────────────────────
    //
    // Per-device state routing is handled with `.with(eq(DeviceId::new(...)))`
    // predicates per expectation — clearer than a hand-written `HashMap`
    // because each test reads as "for peer X, return Online" instead of
    // "set up a map ahead of time."

    mockall::mock! {
        pub Presence {}

        #[async_trait]
        impl PresencePort for Presence {
            async fn ensure_reachable(
                &self,
                device: &DeviceId,
            ) -> Result<ReachabilityState, PresenceError>;
            async fn current_state(&self, device: &DeviceId) -> ReachabilityState;
            fn subscribe(&self) -> broadcast::Receiver<PresenceEvent>;
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

    // ── mockall: ClipboardDispatchPort ──────────────────────────────────
    //
    // The use case fan-outs via JoinSet, which spawns one task per target.
    // mockall's internal expectation `Mutex<FnMut>` would serialise
    // concurrent `.returning()` closures — but only when those closures
    // perform an `.await` that yields. Our `.returning(|_, _, _| ...)`
    // bodies return immediately, so there's nothing to serialise. The
    // Phase 1 T6 lesson (SleepyPresence) only applies when asserting
    // wall-time concurrency; per-target outcome assertions don't need it.

    mockall::mock! {
        pub Dispatch {}

        #[async_trait]
        impl ClipboardDispatchPort for Dispatch {
            async fn dispatch(
                &self,
                target: &DeviceId,
                header: &ClipboardHeader,
                payload: SyncPayload,
            ) -> Result<DispatchAck, ClipboardDispatchError>;
        }
    }

    // ── mockall: DeviceIdentityPort / LocalIdentityPort / SettingsPort ──

    mockall::mock! {
        pub DeviceId_ {}

        impl DeviceIdentityPort for DeviceId_ {
            fn current_device_id(&self) -> DeviceId;
        }
    }

    mockall::mock! {
        pub LocalIdentity {}

        #[async_trait]
        impl LocalIdentityPort for LocalIdentity {
            async fn create(&self) -> Result<IdentityFingerprint, LocalIdentityError>;
            async fn ensure(&self) -> Result<IdentityFingerprint, LocalIdentityError>;
            async fn get_current_fingerprint(
                &self,
            ) -> Result<Option<IdentityFingerprint>, LocalIdentityError>;
        }
    }

    mockall::mock! {
        pub Settings_ {}

        #[async_trait]
        impl SettingsPort for Settings_ {
            async fn load(&self) -> anyhow::Result<Settings>;
            async fn save(&self, s: &Settings) -> anyhow::Result<()>;
        }
    }

    // ── hand-written: ClockPort ─────────────────────────────────────────
    //
    // `ClockPort::now_ms` is sync + 4 lines; mockall's adapter would be
    // strictly more code than the hand-written `FixedClock`. Phase 1's
    // ensure_reachable_all uses the same pattern (`FixedDevice`).

    struct FixedClock(i64);
    impl ClockPort for FixedClock {
        fn now_ms(&self) -> i64 {
            self.0
        }
    }

    // ── helpers ─────────────────────────────────────────────────────────

    fn fp(seed: u8) -> IdentityFingerprint {
        IdentityFingerprint::from_raw_string(
            (0..16)
                .map(|i| char::from(b'A' + ((seed as usize + i) % 26) as u8))
                .collect::<String>(),
        )
        .expect("valid fingerprint")
    }

    fn record(device: &str) -> PeerAddressRecord {
        PeerAddressRecord {
            device_id: DeviceId::new(device),
            addr_blob: vec![0xAA; 32],
            observed_at: Utc::now(),
        }
    }

    /// Build a `Settings` whose `general.device_name` round-trips to a
    /// stable header value.
    fn settings_with_device_name(name: &str) -> Settings {
        let mut s = Settings::default();
        s.general.device_name = Some(name.to_string());
        s
    }

    /// Wire the use case from a set of mock ports. The clock is always
    /// the same fixed value so header `captured_at_ms` assertions are
    /// deterministic.
    fn build_uc(
        peer_addr_repo: MockPeerAddrRepo,
        presence: MockPresence,
        cipher: MockCipher,
        dispatch: MockDispatch,
        device_identity: MockDeviceId_,
        local_identity: MockLocalIdentity,
        settings: MockSettings_,
    ) -> DispatchClipboardEntryUseCase {
        DispatchClipboardEntryUseCase::new(
            Arc::new(peer_addr_repo),
            Arc::new(presence),
            Arc::new(cipher),
            Arc::new(dispatch),
            Arc::new(device_identity),
            Arc::new(local_identity),
            Arc::new(settings),
            Arc::new(FixedClock(1_700_000_000_000)),
        )
    }

    /// Build a `DeviceIdentity` mock that returns the same `device_id`
    /// every call. Always-present helper because every test sets self.
    fn make_device_identity(local: &str) -> MockDeviceId_ {
        let local = DeviceId::new(local);
        let mut m = MockDeviceId_::new();
        m.expect_current_device_id()
            .returning(move || local.clone());
        m
    }

    /// Default settings + identity stubs that every test wires identically.
    fn make_local_identity_stub() -> MockLocalIdentity {
        let mut m = MockLocalIdentity::new();
        m.expect_get_current_fingerprint()
            .returning(|| Ok(Some(fp(7))));
        m
    }

    fn make_settings_stub() -> MockSettings_ {
        let mut m = MockSettings_::new();
        m.expect_load()
            .returning(|| Ok(settings_with_device_name("Test Device")));
        m
    }

    fn input() -> DispatchClipboardEntryInput {
        DispatchClipboardEntryInput {
            plaintext: Bytes::from_static(b"hello world"),
            content_hash: "9".repeat(64),
            payload_version: 3,
        }
    }

    // ── verdicts ────────────────────────────────────────────────────────

    /// 1. Happy path — two online peers, both accept. mockall asserts
    /// dispatch is called exactly twice (once per peer) and the encrypt
    /// path runs exactly once.
    #[tokio::test]
    async fn fan_outs_to_all_online_peers_and_counts_accepted() {
        let mut repo = MockPeerAddrRepo::new();
        repo.expect_list()
            .times(1)
            .returning(|| Ok(vec![record("peer-a"), record("peer-b")]));

        let mut presence = MockPresence::new();
        presence
            .expect_current_state()
            .with(eq(DeviceId::new("peer-a")))
            .times(1)
            .returning(|_| ReachabilityState::Online);
        presence
            .expect_current_state()
            .with(eq(DeviceId::new("peer-b")))
            .times(1)
            .returning(|_| ReachabilityState::Online);

        let mut cipher = MockCipher::new();
        cipher
            .expect_encrypt()
            .times(1)
            .returning(|p| Ok(p.to_vec()));

        let mut dispatch = MockDispatch::new();
        dispatch
            .expect_dispatch()
            .with(eq(DeviceId::new("peer-a")), always(), always())
            .times(1)
            .returning(|_, _, _| Ok(DispatchAck::Accepted));
        dispatch
            .expect_dispatch()
            .with(eq(DeviceId::new("peer-b")), always(), always())
            .times(1)
            .returning(|_, _, _| Ok(DispatchAck::Accepted));

        let uc = build_uc(
            repo,
            presence,
            cipher,
            dispatch,
            make_device_identity("self-device"),
            make_local_identity_stub(),
            make_settings_stub(),
        );

        let outcome = uc.execute(input()).await.expect("dispatch ok");
        assert_eq!(outcome.total_accepted, 2);
        assert_eq!(outcome.total_offline, 0);
        assert_eq!(outcome.total_errored, 0);
        assert_eq!(outcome.per_target.len(), 2);
    }

    /// 2. Offline filter — one peer is Offline, dispatch port is **never**
    /// called for it (mockall enforces via `.times(0)` implicit on the
    /// missing expectation; an extra call would panic on drop).
    #[tokio::test]
    async fn skips_offline_peers_without_invoking_dispatch_port() {
        let mut repo = MockPeerAddrRepo::new();
        repo.expect_list()
            .times(1)
            .returning(|| Ok(vec![record("peer-on"), record("peer-off")]));

        let mut presence = MockPresence::new();
        presence
            .expect_current_state()
            .with(eq(DeviceId::new("peer-on")))
            .times(1)
            .returning(|_| ReachabilityState::Online);
        presence
            .expect_current_state()
            .with(eq(DeviceId::new("peer-off")))
            .times(1)
            .returning(|_| ReachabilityState::Offline);

        let mut cipher = MockCipher::new();
        cipher
            .expect_encrypt()
            .times(1)
            .returning(|p| Ok(p.to_vec()));

        let mut dispatch = MockDispatch::new();
        // Only one expectation registered; if the use case calls dispatch
        // for `peer-off`, mockall will panic ("no matching expectation").
        dispatch
            .expect_dispatch()
            .with(eq(DeviceId::new("peer-on")), always(), always())
            .times(1)
            .returning(|_, _, _| Ok(DispatchAck::Accepted));

        let uc = build_uc(
            repo,
            presence,
            cipher,
            dispatch,
            make_device_identity("self-device"),
            make_local_identity_stub(),
            make_settings_stub(),
        );

        let outcome = uc.execute(input()).await.expect("dispatch ok");
        assert_eq!(outcome.total_accepted, 1);
        assert_eq!(outcome.per_target.len(), 1);
        assert_eq!(outcome.per_target[0].device_id.as_str(), "peer-on");
    }

    /// 3. Self-filter — `peer_addr_repo` inadvertently contains the local
    /// device. mockall enforces self-skip: no dispatch expectation is
    /// registered for `self-device`, so a self-dial would panic.
    #[tokio::test]
    async fn skips_self_even_if_peer_addr_repo_contains_local_device() {
        let mut repo = MockPeerAddrRepo::new();
        repo.expect_list()
            .times(1)
            .returning(|| Ok(vec![record("self-device"), record("peer-a")]));

        let mut presence = MockPresence::new();
        // The use case filters self before consulting presence, so
        // `self-device` must not be probed either. Single expectation
        // covers `peer-a` only.
        presence
            .expect_current_state()
            .with(eq(DeviceId::new("peer-a")))
            .times(1)
            .returning(|_| ReachabilityState::Online);

        let mut cipher = MockCipher::new();
        cipher
            .expect_encrypt()
            .times(1)
            .returning(|p| Ok(p.to_vec()));

        let mut dispatch = MockDispatch::new();
        dispatch
            .expect_dispatch()
            .with(eq(DeviceId::new("peer-a")), always(), always())
            .times(1)
            .returning(|_, _, _| Ok(DispatchAck::Accepted));

        let uc = build_uc(
            repo,
            presence,
            cipher,
            dispatch,
            make_device_identity("self-device"),
            make_local_identity_stub(),
            make_settings_stub(),
        );

        let outcome = uc.execute(input()).await.expect("dispatch ok");
        assert_eq!(outcome.per_target.len(), 1);
        assert_eq!(outcome.per_target[0].device_id.as_str(), "peer-a");
    }

    /// 4. Locked space — `transfer_cipher.encrypt` returns `NotUnlocked`.
    /// Use case short-circuits with `LockedSpace`. mockall enforces "no
    /// dispatch ever called" by registering zero dispatch expectations.
    #[tokio::test]
    async fn locked_space_short_circuits_before_dispatch() {
        // peer_addr_repo and presence aren't reached — register zero
        // expectations so an accidental call would panic.
        let repo = MockPeerAddrRepo::new();
        let presence = MockPresence::new();

        let mut cipher = MockCipher::new();
        cipher
            .expect_encrypt()
            .times(1)
            .returning(|_| Err(TransferCipherError::NotUnlocked));

        let dispatch = MockDispatch::new();

        let uc = build_uc(
            repo,
            presence,
            cipher,
            dispatch,
            make_device_identity("self-device"),
            make_local_identity_stub(),
            make_settings_stub(),
        );

        let err = uc
            .execute(input())
            .await
            .expect_err("locked space must short-circuit");
        assert!(matches!(err, DispatchSyncError::LockedSpace));
    }

    /// 5. Mixed outcomes — one accept, one offline, one rejected. Each
    /// target's expectation is registered independently with `.with(...)`
    /// matching the device id; mockall picks the right one per call,
    /// ignoring task ordering (JoinSet).
    #[tokio::test]
    async fn mixed_per_target_outcomes_are_reported_independently() {
        let mut repo = MockPeerAddrRepo::new();
        repo.expect_list().times(1).returning(|| {
            Ok(vec![
                record("peer-ok"),
                record("peer-off"),
                record("peer-rej"),
            ])
        });

        let mut presence = MockPresence::new();
        for d in ["peer-ok", "peer-off", "peer-rej"] {
            presence
                .expect_current_state()
                .with(eq(DeviceId::new(d)))
                .times(1)
                .returning(|_| ReachabilityState::Online);
        }

        let mut cipher = MockCipher::new();
        cipher
            .expect_encrypt()
            .times(1)
            .returning(|p| Ok(p.to_vec()));

        let mut dispatch = MockDispatch::new();
        dispatch
            .expect_dispatch()
            .with(eq(DeviceId::new("peer-ok")), always(), always())
            .times(1)
            .returning(|_, _, _| Ok(DispatchAck::Accepted));
        dispatch
            .expect_dispatch()
            .with(eq(DeviceId::new("peer-off")), always(), always())
            .times(1)
            .returning(|_, _, _| Err(ClipboardDispatchError::Offline));
        dispatch
            .expect_dispatch()
            .with(eq(DeviceId::new("peer-rej")), always(), always())
            .times(1)
            .returning(|_, _, _| Err(ClipboardDispatchError::PeerRejected("too big".to_string())));

        let uc = build_uc(
            repo,
            presence,
            cipher,
            dispatch,
            make_device_identity("self-device"),
            make_local_identity_stub(),
            make_settings_stub(),
        );

        let outcome = uc.execute(input()).await.expect("dispatch ok");
        assert_eq!(outcome.total_accepted, 1);
        assert_eq!(outcome.total_offline, 1);
        assert_eq!(outcome.total_errored, 1);
        assert_eq!(outcome.per_target.len(), 3);

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
}
