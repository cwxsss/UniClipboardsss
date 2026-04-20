//! B2 · `RedeemPairingInvitationUseCase` (joiner side).
//!
//! Drives the full joiner-side pairing handshake end-to-end as a single
//! blocking use case. Slice 1 UX: the user types both the invitation code
//! and the sponsor's space passphrase up front, so the flow is purely
//! linear once `execute` is called:
//!
//! ```text
//!   dial_by_invitation
//!   → send JoinerRequest
//!   → recv KeyslotOffer | Reject            (TTL-guarded)
//!   → derive_master_key_for_proof           (persists local keyslot as
//!                                            a side effect)
//!   → build_proof
//!   → send ChallengeResponse
//!   → recv Confirm | Reject                 (TTL-guarded)
//!   → admit_member → trust_peer
//!   → mark setup complete
//!   → close session
//! ```
//!
//! ## Ordering: persist before declaring success
//!
//! Mirrors sponsor-side P7f cleanup: admit / trust / setup-status mark
//! run **before** `execute` returns `Ok`. Any persistence failure
//! surfaces as [`RedeemPairingInvitationError::Internal`] — the caller
//! never gets a success result that isn't backed by fully committed
//! local state.
//!
//! ## Why no state machine
//!
//! Same reasoning as F-052 on sponsor side: the flow is linear once the
//! passphrase is collected up front, and `SpaceAccessStateMachine`'s
//! default action order (`SendResult` before `PersistJoinerAccess`) is
//! inverted from the persist-before-success ordering this use case
//! wants. Running a linear path through an FSM adds enum ceremony
//! without buying branch-safety — documented in F-053.
//!
//! ## TTL
//!
//! Both `recv` calls are wrapped in `tokio::time::timeout(handshake_ttl,
//! …)`. This is orthogonal to P7g's sponsor-side TTL watchdog:
//! joiner's TTL protects against a silent sponsor, sponsor's TTL
//! protects against a silent joiner. If **both** fire the sponsor's
//! `Reject(Timeout)` races our own `Elapsed`; we treat whichever wins
//! as the authoritative failure (sponsor's Reject wins if it arrives
//! before our `timeout` expires). Both paths surface as user-facing
//! "timed out".

use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::time::{timeout, Duration};
use tracing::{debug, info, instrument, warn};

use uc_core::ids::SessionId;
use uc_core::pairing::session_message::{
    JoinerChallengeResponse, JoinerRequest, PairingRejectReason, PairingSessionMessage,
};
use uc_core::ports::pairing::{DialError, PairingSessionId, PairingSessionPort, SessionError};
use uc_core::ports::space::{ProofPort, SpaceAccessError, SpaceAccessPort};
use uc_core::ports::{
    ClockPort, DeviceIdentityPort, LocalIdentityPort, SettingsPort, SetupStatusPort,
};
use uc_core::setup::SetupStatus;
use uc_core::space_access::JoinOffer;
use uc_core::{MemberRepositoryPort, MemberSyncPreferences, TrustedPeerRepositoryPort};

use crate::facade::space_setup::{
    RedeemPairingInvitationCommand, RedeemPairingInvitationError, RedeemPairingInvitationResult,
};
use crate::membership::errors::MembershipApplicationError;
use crate::membership::usecases::{AdmitMember, AdmitMemberUseCase};
use crate::trusted_peer::errors::TrustedPeerApplicationError;
use crate::trusted_peer::usecases::{TrustPeer, TrustPeerUseCase};

/// Type aliases mirroring `pairing_inbound::orchestrator` so the facade
/// can inject pre-constructed use cases without re-stating the
/// `dyn …RepositoryPort` bound at every call site.
pub(crate) type AdmitMemberUc = AdmitMemberUseCase<dyn MemberRepositoryPort>;
pub(crate) type TrustPeerUc = TrustPeerUseCase<dyn TrustedPeerRepositoryPort>;

pub(crate) struct RedeemPairingInvitationUseCase {
    pairing_session: Arc<dyn PairingSessionPort>,
    space_access: Arc<dyn SpaceAccessPort>,
    proof_port: Arc<dyn ProofPort>,
    local_identity: Arc<dyn LocalIdentityPort>,
    device_identity: Arc<dyn DeviceIdentityPort>,
    settings: Arc<dyn SettingsPort>,
    setup_status: Arc<dyn SetupStatusPort>,
    admit_member: Arc<AdmitMemberUc>,
    trust_peer: Arc<TrustPeerUc>,
    clock: Arc<dyn ClockPort>,
    /// 等 sponsor 回消息的 per-recv TTL。这不是端到端握手总时长（dial +
    /// derive + build + 两轮 recv 之和），而是单次"等 sponsor 回话"
    /// 的上限。
    handshake_ttl: Duration,
}

impl RedeemPairingInvitationUseCase {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        pairing_session: Arc<dyn PairingSessionPort>,
        space_access: Arc<dyn SpaceAccessPort>,
        proof_port: Arc<dyn ProofPort>,
        local_identity: Arc<dyn LocalIdentityPort>,
        device_identity: Arc<dyn DeviceIdentityPort>,
        settings: Arc<dyn SettingsPort>,
        setup_status: Arc<dyn SetupStatusPort>,
        admit_member: Arc<AdmitMemberUc>,
        trust_peer: Arc<TrustPeerUc>,
        clock: Arc<dyn ClockPort>,
        handshake_ttl: Duration,
    ) -> Self {
        Self {
            pairing_session,
            space_access,
            proof_port,
            local_identity,
            device_identity,
            settings,
            setup_status,
            admit_member,
            trust_peer,
            clock,
            handshake_ttl,
        }
    }

    #[instrument(skip_all, fields(code = %cmd.code.as_str()))]
    pub(crate) async fn execute(
        &self,
        cmd: RedeemPairingInvitationCommand,
    ) -> Result<RedeemPairingInvitationResult, RedeemPairingInvitationError> {
        // Dial first so a NotFound / Expired / Unreachable surfaces
        // without spending any crypto work. A successful dial creates a
        // session in the adapter that we must close on every exit path
        // below (including the error paths).
        let session = self
            .pairing_session
            .dial_by_invitation(&cmd.code)
            .await
            .map_err(map_dial_err)?;
        info!(session = %session, "pairing session dialled");

        match self.drive(&session, cmd).await {
            Ok(result) => {
                self.pairing_session
                    .close(&session, Some("handshake completed".into()))
                    .await;
                Ok(result)
            }
            Err(err) => {
                self.pairing_session
                    .close(&session, Some(format!("handshake aborted: {err}")))
                    .await;
                Err(err)
            }
        }
    }

    async fn drive(
        &self,
        session: &PairingSessionId,
        cmd: RedeemPairingInvitationCommand,
    ) -> Result<RedeemPairingInvitationResult, RedeemPairingInvitationError> {
        // ── 1. Collect local facts ───────────────────────────────────────
        let local_fp = self.local_identity.ensure().await.map_err(|e| {
            RedeemPairingInvitationError::Internal(format!("local_identity.ensure: {e}"))
        })?;
        let local_device_id = self.device_identity.current_device_id();
        let local_device_name = self
            .settings
            .load()
            .await
            .map_err(|e| RedeemPairingInvitationError::Internal(format!("settings.load: {e}")))?
            .general
            .device_name
            .filter(|n| !n.trim().is_empty())
            .ok_or(RedeemPairingInvitationError::DeviceNameRequired)?;

        // ── 2. Send JoinerRequest ────────────────────────────────────────
        let request = JoinerRequest {
            invitation_code: cmd.code.clone(),
            device_id: local_device_id.clone(),
            device_name: local_device_name,
            identity_fingerprint: local_fp.clone(),
            // 保留字段：Slice 1 sponsor 不消费 transcript nonce，留空占位即
            // 可；加 rand crate 不值当。未来 Slice 若把 transcript binding
            // 纳入 HMAC，再在这里填。
            nonce: Vec::new(),
        };
        self.pairing_session
            .send(session, PairingSessionMessage::Request(request))
            .await
            .map_err(map_session_err)?;
        debug!(session = %session, "JoinerRequest sent; awaiting KeyslotOffer");

        // ── 3. Await KeyslotOffer | Reject ───────────────────────────────
        let offer = match self.recv_with_ttl(session).await? {
            PairingSessionMessage::KeyslotOffer(o) => o,
            PairingSessionMessage::Reject(r) => return Err(map_sponsor_reject(r.reason)),
            other => {
                return Err(RedeemPairingInvitationError::Internal(format!(
                    "expected KeyslotOffer, got {}",
                    variant_name(&other),
                )));
            }
        };
        debug!(
            session = %session,
            space_id = %offer.space_id,
            "KeyslotOffer received"
        );

        // ── 4. Derive proof key (side effect: persists local keyslot) ───
        let challenge_nonce = challenge_to_array(&offer.challenge)?;
        let join_offer = JoinOffer {
            space_id: offer.space_id.clone(),
            keyslot_blob: offer.keyslot_blob.clone(),
            challenge_nonce,
        };
        let derived_key = self
            .space_access
            .derive_master_key_for_proof(&join_offer, &cmd.passphrase)
            .await
            .map_err(map_space_access_err)?;
        debug!(session = %session, "master key derived from sponsor offer");

        // ── 5. Build HMAC proof ──────────────────────────────────────────
        let core_session = SessionId::new(offer.pairing_session_id.as_str().to_string());
        let proof = self
            .proof_port
            .build_proof(
                &core_session,
                &join_offer.space_id,
                challenge_nonce,
                &derived_key,
            )
            .await
            .map_err(|e| RedeemPairingInvitationError::Internal(format!("build_proof: {e}")))?;

        // ── 6. Send ChallengeResponse ────────────────────────────────────
        self.pairing_session
            .send(
                session,
                PairingSessionMessage::ChallengeResponse(JoinerChallengeResponse {
                    encrypted_challenge: proof.proof_bytes,
                }),
            )
            .await
            .map_err(map_session_err)?;
        debug!(session = %session, "ChallengeResponse sent; awaiting Confirm/Reject");

        // ── 7. Await Confirm | Reject ────────────────────────────────────
        let confirm = match self.recv_with_ttl(session).await? {
            PairingSessionMessage::Confirm(c) => c,
            PairingSessionMessage::Reject(r) => return Err(map_sponsor_reject(r.reason)),
            other => {
                return Err(RedeemPairingInvitationError::Internal(format!(
                    "expected Confirm, got {}",
                    variant_name(&other),
                )));
            }
        };
        info!(
            session = %session,
            sponsor_device_id = %confirm.sender_device_id.as_str(),
            "Confirm received from sponsor"
        );

        // ── 8. Persist: admit → trust → mark setup complete ──────────────
        //
        // Ordering: admit before trust mirrors sponsor side (P7f cleanup).
        // Setup marked complete *last*, and only after both repos landed:
        // `has_completed=true` is the marker A2 `UnlockSpaceUseCase` keys
        // off, so we must not flip it before the admit/trust rows exist
        // (otherwise policy checks would see "setup done" but no trusted
        // peers and block every inbound session).
        let now = self.now_utc()?;
        let admit_input = AdmitMember {
            device_id: confirm.sender_device_id.clone(),
            device_name: confirm.sender_device_name.clone(),
            identity_fingerprint: confirm.sender_identity_fingerprint.clone(),
            joined_at: now,
            sync_preferences: MemberSyncPreferences::default(),
        };
        self.admit_member
            .execute(admit_input)
            .await
            .map_err(map_admit_err)?;

        let trust_input = TrustPeer {
            local_device_id: local_device_id.clone(),
            peer_device_id: confirm.sender_device_id.clone(),
            peer_fingerprint: confirm.sender_identity_fingerprint.clone(),
            trusted_at: now,
        };
        self.trust_peer
            .execute(trust_input)
            .await
            .map_err(map_trust_err)?;

        self.setup_status
            .set_status(&SetupStatus {
                has_completed: true,
            })
            .await
            .map_err(|e| {
                RedeemPairingInvitationError::Internal(format!("setup_status.set_status: {e}"))
            })?;

        info!(
            session = %session,
            sponsor_device_id = %confirm.sender_device_id.as_str(),
            space_id = %confirm.space_id,
            "joiner handshake completed; local space ready"
        );

        Ok(RedeemPairingInvitationResult {
            sponsor_device_id: confirm.sender_device_id,
            sponsor_identity_fingerprint: confirm.sender_identity_fingerprint,
            space_id: confirm.space_id,
            self_device_id: local_device_id,
            self_identity_fingerprint: local_fp,
        })
    }

    async fn recv_with_ttl(
        &self,
        session: &PairingSessionId,
    ) -> Result<PairingSessionMessage, RedeemPairingInvitationError> {
        match timeout(self.handshake_ttl, self.pairing_session.recv_next(session)).await {
            Err(_elapsed) => {
                warn!(
                    session = %session,
                    ttl_ms = %self.handshake_ttl.as_millis(),
                    "recv_with_ttl exceeded; aborting handshake"
                );
                Err(RedeemPairingInvitationError::Timeout)
            }
            Ok(Ok(Some(msg))) => Ok(msg),
            Ok(Ok(None)) => Err(RedeemPairingInvitationError::ConnectionLost),
            Ok(Err(err)) => Err(map_session_err(err)),
        }
    }

    fn now_utc(&self) -> Result<DateTime<Utc>, RedeemPairingInvitationError> {
        DateTime::<Utc>::from_timestamp_millis(self.clock.now_ms()).ok_or_else(|| {
            RedeemPairingInvitationError::Internal("clock returned invalid timestamp".into())
        })
    }
}

fn challenge_to_array(bytes: &[u8]) -> Result<[u8; 32], RedeemPairingInvitationError> {
    bytes.try_into().map_err(|_| {
        RedeemPairingInvitationError::Internal(format!(
            "challenge nonce wire length invalid: expected 32 bytes, got {}",
            bytes.len()
        ))
    })
}

fn map_dial_err(err: DialError) -> RedeemPairingInvitationError {
    match err {
        DialError::InvitationNotFound => RedeemPairingInvitationError::InvitationNotFound,
        DialError::InvitationExpired => RedeemPairingInvitationError::InvitationExpired,
        DialError::SponsorUnreachable => RedeemPairingInvitationError::SponsorUnreachable,
        DialError::ServiceUnavailable => RedeemPairingInvitationError::ServiceUnavailable,
        DialError::Internal(m) => RedeemPairingInvitationError::Internal(m),
    }
}

fn map_session_err(err: SessionError) -> RedeemPairingInvitationError {
    match err {
        SessionError::NotFound(_) | SessionError::Closed => {
            RedeemPairingInvitationError::ConnectionLost
        }
        SessionError::Internal(m) => RedeemPairingInvitationError::Internal(m),
    }
}

fn map_space_access_err(err: SpaceAccessError) -> RedeemPairingInvitationError {
    match err {
        SpaceAccessError::WrongPassphrase => RedeemPairingInvitationError::PassphraseMismatch,
        SpaceAccessError::CorruptedKeyMaterial => {
            RedeemPairingInvitationError::CorruptedKeyMaterial
        }
        other => {
            RedeemPairingInvitationError::Internal(format!("derive_master_key_for_proof: {other}"))
        }
    }
}

fn map_sponsor_reject(reason: PairingRejectReason) -> RedeemPairingInvitationError {
    match reason {
        PairingRejectReason::InvitationMismatch => {
            RedeemPairingInvitationError::SponsorRejectedInvitation
        }
        // Sponsor's `verify_proof` failed = wrong passphrase — same
        // user-facing meaning as local `WrongPassphrase`, fold into one
        // variant so UI doesn't need to distinguish "who noticed first".
        PairingRejectReason::PassphraseMismatch => RedeemPairingInvitationError::PassphraseMismatch,
        PairingRejectReason::UserRejected => RedeemPairingInvitationError::SponsorDeclined,
        PairingRejectReason::Timeout => RedeemPairingInvitationError::SponsorTimedOut,
        PairingRejectReason::Internal(m) => RedeemPairingInvitationError::SponsorInternal(m),
    }
}

fn map_admit_err(err: MembershipApplicationError) -> RedeemPairingInvitationError {
    // `AlreadyAdmitted` / `AlreadyTrusted` surface as Internal rather
    // than as "OK resume": we don't know if the previous run completed
    // setup_status.set_status, and returning Ok while that flag might
    // still be false would leave the space in a half-committed state.
    // The fix surface is a `factory_reset` followed by a fresh redeem.
    RedeemPairingInvitationError::Internal(format!("admit_member: {err}"))
}

fn map_trust_err(err: TrustedPeerApplicationError) -> RedeemPairingInvitationError {
    RedeemPairingInvitationError::Internal(format!("trust_peer: {err}"))
}

fn variant_name(message: &PairingSessionMessage) -> &'static str {
    match message {
        PairingSessionMessage::Request(_) => "Request",
        PairingSessionMessage::KeyslotOffer(_) => "KeyslotOffer",
        PairingSessionMessage::ChallengeResponse(_) => "ChallengeResponse",
        PairingSessionMessage::Confirm(_) => "Confirm",
        PairingSessionMessage::Reject(_) => "Reject",
    }
}

#[cfg(test)]
mod tests {
    //! Use-case-level tests. Scope:
    //!
    //! * Wire ordering: correct sequence of send/recv/close on the
    //!   session port.
    //! * Persistence ordering: admit → trust → setup-status all land
    //!   before `execute` returns Ok, and *any* persistence failure
    //!   short-circuits before the next step.
    //! * Error mapping: dial / local derive / sponsor reject / own TTL
    //!   / connection loss / unexpected message all surface the
    //!   user-facing enum the facade promises.
    //!
    //! Not exercised here: `derive_master_key_for_proof` side effects
    //! on disk (that's `uc-infra` adapter territory); network runtime
    //! auto-start (that's covered in `facade.rs` smoke tests).
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex as StdMutex;

    use async_trait::async_trait;
    use chrono::{DateTime, Duration as ChronoDuration};

    use uc_core::crypto::domain::{ActiveSpace, Passphrase};
    use uc_core::ids::{DeviceId, SessionId, SpaceId};
    use uc_core::membership::{MembershipError, SpaceMember};
    use uc_core::pairing::invitation::InvitationCode;
    use uc_core::pairing::session_message::{
        JoinerChallengeResponse, JoinerRequest, PairingReject, SponsorConfirm, SponsorKeyslotOffer,
    };
    use uc_core::ports::pairing::{DialError, SessionError};
    use uc_core::ports::space::SpaceAccessError;
    use uc_core::ports::LocalIdentityError;
    use uc_core::security::IdentityFingerprint;
    use uc_core::settings::model::Settings;
    use uc_core::space_access::domain::{ProofDerivedKey, SpaceAccessProofArtifact};
    use uc_core::trusted_peer::{TrustedPeer, TrustedPeerError};

    // ── fakes ────────────────────────────────────────────────────────────

    /// Scripted session port driving the entire joiner conversation.
    ///
    /// * `dial_result` — single shot result for `dial_by_invitation`.
    /// * `recv_script` — FIFO queue of results returned by `recv_next`;
    ///   each test seeds the messages it wants the sponsor to "send".
    ///   `RecvStep::Hang` models "never responds" so the use case's own
    ///   TTL fires via `tokio::time::timeout` under `start_paused`.
    #[derive(Default)]
    struct ScriptedSession {
        dial_result: StdMutex<Option<Result<PairingSessionId, DialError>>>,
        sent: StdMutex<Vec<(PairingSessionId, PairingSessionMessage)>>,
        closed: StdMutex<Vec<(PairingSessionId, Option<String>)>>,
        recv_script: StdMutex<VecDeque<RecvStep>>,
        send_next_error: StdMutex<Option<SessionError>>,
    }
    enum RecvStep {
        Msg(PairingSessionMessage),
        CleanClose,
        Err(SessionError),
        /// "Never responds" — sleeps forever so the caller's own
        /// `timeout` wrapper fires. In paused mode the runtime just
        /// parks and advances time, it doesn't actually idle real CPU.
        Hang,
    }

    impl ScriptedSession {
        fn with_dial_ok(id: &str) -> Self {
            let me = Self::default();
            *me.dial_result.lock().unwrap() = Some(Ok(PairingSessionId::new(id.to_string())));
            me
        }
        fn with_dial_err(err: DialError) -> Self {
            let me = Self::default();
            *me.dial_result.lock().unwrap() = Some(Err(err));
            me
        }
        fn push_recv(&self, step: RecvStep) {
            self.recv_script.lock().unwrap().push_back(step);
        }
        fn sent(&self) -> Vec<(PairingSessionId, PairingSessionMessage)> {
            self.sent.lock().unwrap().clone()
        }
        fn closed(&self) -> Vec<(PairingSessionId, Option<String>)> {
            self.closed.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl PairingSessionPort for ScriptedSession {
        async fn dial_by_invitation(
            &self,
            _code: &InvitationCode,
        ) -> Result<PairingSessionId, DialError> {
            // Clone rather than take: a happy test can call dial once,
            // a multi-dial test doesn't exist in Slice 1 so we just
            // replay the scripted answer.
            match self.dial_result.lock().unwrap().as_ref() {
                Some(Ok(id)) => Ok(id.clone()),
                Some(Err(err)) => Err(clone_dial_err(err)),
                None => Err(DialError::Internal("test misconfigured".into())),
            }
        }
        async fn send(
            &self,
            session: &PairingSessionId,
            message: PairingSessionMessage,
        ) -> Result<(), SessionError> {
            if let Some(err) = self.send_next_error.lock().unwrap().take() {
                return Err(err);
            }
            self.sent.lock().unwrap().push((session.clone(), message));
            Ok(())
        }
        async fn recv_next(
            &self,
            _session: &PairingSessionId,
        ) -> Result<Option<PairingSessionMessage>, SessionError> {
            let next = self.recv_script.lock().unwrap().pop_front();
            match next {
                Some(RecvStep::Msg(m)) => Ok(Some(m)),
                Some(RecvStep::CleanClose) => Ok(None),
                Some(RecvStep::Err(e)) => Err(e),
                Some(RecvStep::Hang) | None => std::future::pending().await,
            }
        }
        async fn close(&self, session: &PairingSessionId, reason: Option<String>) {
            self.closed.lock().unwrap().push((session.clone(), reason));
        }
    }

    fn clone_dial_err(err: &DialError) -> DialError {
        match err {
            DialError::InvitationNotFound => DialError::InvitationNotFound,
            DialError::InvitationExpired => DialError::InvitationExpired,
            DialError::SponsorUnreachable => DialError::SponsorUnreachable,
            DialError::ServiceUnavailable => DialError::ServiceUnavailable,
            DialError::Internal(m) => DialError::Internal(m.clone()),
        }
    }

    /// SpaceAccess fake — only `derive_master_key_for_proof` matters
    /// for this use case. Arm a preset error via `fail_next`.
    struct ScriptedSpaceAccess {
        fail_next: StdMutex<Option<SpaceAccessError>>,
        derived_key_bytes: [u8; 32],
    }
    impl ScriptedSpaceAccess {
        fn ok() -> Self {
            Self {
                fail_next: StdMutex::new(None),
                derived_key_bytes: [0xCC; 32],
            }
        }
        fn with_err(err: SpaceAccessError) -> Self {
            Self {
                fail_next: StdMutex::new(Some(err)),
                derived_key_bytes: [0xCC; 32],
            }
        }
    }
    #[async_trait]
    impl SpaceAccessPort for ScriptedSpaceAccess {
        async fn initialize(
            &self,
            _: &SpaceId,
            _: &Passphrase,
        ) -> Result<ActiveSpace, SpaceAccessError> {
            unimplemented!("not used by B2")
        }
        async fn unlock(
            &self,
            _: &SpaceId,
            _: &Passphrase,
        ) -> Result<ActiveSpace, SpaceAccessError> {
            unimplemented!("not used by B2")
        }
        async fn is_unlocked(&self, _: &SpaceId) -> bool {
            true
        }
        async fn lock(&self, _: &SpaceId) -> Result<(), SpaceAccessError> {
            Ok(())
        }
        async fn factory_reset(&self, _: &SpaceId) -> Result<(), SpaceAccessError> {
            Ok(())
        }
        async fn try_resume_session(
            &self,
            _: &SpaceId,
        ) -> Result<Option<ActiveSpace>, SpaceAccessError> {
            Ok(None)
        }
        async fn verify_keychain_access(&self) -> Result<bool, SpaceAccessError> {
            Ok(true)
        }
        async fn derive_subkey(&self, _: &[u8], _: &[u8]) -> Result<[u8; 32], SpaceAccessError> {
            Ok([0; 32])
        }
        async fn current_session_proof_key(
            &self,
        ) -> Result<Option<ProofDerivedKey>, SpaceAccessError> {
            Ok(None)
        }
        async fn prepare_join_offer(
            &self,
            _: &SpaceId,
            _: &Passphrase,
        ) -> Result<uc_core::space_access::JoinOffer, SpaceAccessError> {
            unimplemented!("not used by B2")
        }
        async fn derive_master_key_for_proof(
            &self,
            _: &uc_core::space_access::JoinOffer,
            _: &Passphrase,
        ) -> Result<ProofDerivedKey, SpaceAccessError> {
            if let Some(err) = self.fail_next.lock().unwrap().take() {
                return Err(err);
            }
            Ok(ProofDerivedKey::from_bytes(self.derived_key_bytes))
        }
    }

    struct FixedProof(Vec<u8>);
    #[async_trait]
    impl ProofPort for FixedProof {
        async fn build_proof(
            &self,
            _: &SessionId,
            _: &SpaceId,
            _: [u8; 32],
            _: &ProofDerivedKey,
        ) -> anyhow::Result<SpaceAccessProofArtifact> {
            Ok(SpaceAccessProofArtifact {
                pairing_session_id: SessionId::new("fixed".to_string()),
                space_id: SpaceId::from_str("space-xyz"),
                challenge_nonce: [0x42; 32],
                proof_bytes: self.0.clone(),
            })
        }
        async fn verify_proof(
            &self,
            _: &SpaceAccessProofArtifact,
            _: [u8; 32],
        ) -> anyhow::Result<bool> {
            unimplemented!("joiner doesn't verify")
        }
    }

    struct FixedLocal(IdentityFingerprint);
    #[async_trait]
    impl LocalIdentityPort for FixedLocal {
        async fn create(&self) -> Result<IdentityFingerprint, LocalIdentityError> {
            Ok(self.0.clone())
        }
        async fn ensure(&self) -> Result<IdentityFingerprint, LocalIdentityError> {
            Ok(self.0.clone())
        }
        async fn get_current_fingerprint(
            &self,
        ) -> Result<Option<IdentityFingerprint>, LocalIdentityError> {
            Ok(Some(self.0.clone()))
        }
    }

    struct FixedDevice(DeviceId);
    impl DeviceIdentityPort for FixedDevice {
        fn current_device_id(&self) -> DeviceId {
            self.0.clone()
        }
    }

    struct StubSettings(StdMutex<Settings>);
    impl StubSettings {
        fn named(n: &str) -> Self {
            let mut s = Settings::default();
            s.general.device_name = Some(n.into());
            Self(StdMutex::new(s))
        }
        fn blank() -> Self {
            Self(StdMutex::new(Settings::default()))
        }
    }
    #[async_trait]
    impl SettingsPort for StubSettings {
        async fn load(&self) -> anyhow::Result<Settings> {
            Ok(self.0.lock().unwrap().clone())
        }
        async fn save(&self, s: &Settings) -> anyhow::Result<()> {
            *self.0.lock().unwrap() = s.clone();
            Ok(())
        }
    }

    #[derive(Default)]
    struct RecordingSetupStatus {
        status: StdMutex<SetupStatus>,
        set_calls: StdMutex<Vec<bool>>,
    }
    #[async_trait]
    impl SetupStatusPort for RecordingSetupStatus {
        async fn get_status(&self) -> anyhow::Result<SetupStatus> {
            Ok(self.status.lock().unwrap().clone())
        }
        async fn set_status(&self, s: &SetupStatus) -> anyhow::Result<()> {
            self.set_calls.lock().unwrap().push(s.has_completed);
            *self.status.lock().unwrap() = s.clone();
            Ok(())
        }
    }

    #[derive(Default)]
    struct RecordingMemberRepo {
        saved: StdMutex<Vec<SpaceMember>>,
        fail_next: StdMutex<Option<MembershipError>>,
    }
    #[async_trait]
    impl MemberRepositoryPort for RecordingMemberRepo {
        async fn get(&self, _: &DeviceId) -> Result<Option<SpaceMember>, MembershipError> {
            Ok(None)
        }
        async fn list(&self) -> Result<Vec<SpaceMember>, MembershipError> {
            Ok(self.saved.lock().unwrap().clone())
        }
        async fn save(&self, member: &SpaceMember) -> Result<(), MembershipError> {
            if let Some(err) = self.fail_next.lock().unwrap().take() {
                return Err(err);
            }
            self.saved.lock().unwrap().push(member.clone());
            Ok(())
        }
        async fn remove(&self, _: &DeviceId) -> Result<bool, MembershipError> {
            Ok(false)
        }
    }

    #[derive(Default)]
    struct RecordingTrustRepo {
        saved: StdMutex<Vec<TrustedPeer>>,
        fail_next: StdMutex<Option<TrustedPeerError>>,
    }
    #[async_trait]
    impl TrustedPeerRepositoryPort for RecordingTrustRepo {
        async fn get(&self, _: &DeviceId) -> Result<Option<TrustedPeer>, TrustedPeerError> {
            Ok(None)
        }
        async fn list(&self) -> Result<Vec<TrustedPeer>, TrustedPeerError> {
            Ok(self.saved.lock().unwrap().clone())
        }
        async fn save(&self, peer: &TrustedPeer) -> Result<(), TrustedPeerError> {
            if let Some(err) = self.fail_next.lock().unwrap().take() {
                return Err(err);
            }
            self.saved.lock().unwrap().push(peer.clone());
            Ok(())
        }
        async fn remove(&self, _: &DeviceId) -> Result<bool, TrustedPeerError> {
            Ok(false)
        }
    }

    struct FakeClock(i64);
    impl ClockPort for FakeClock {
        fn now_ms(&self) -> i64 {
            self.0
        }
    }

    // ── fixtures ─────────────────────────────────────────────────────────

    const TEST_TTL: Duration = Duration::from_secs(30);

    fn sponsor_fp() -> IdentityFingerprint {
        IdentityFingerprint::from_raw_string("BBBBBBBBBBBBBBBB").unwrap()
    }
    fn joiner_fp() -> IdentityFingerprint {
        IdentityFingerprint::from_raw_string("AAAAAAAAAAAAAAAA").unwrap()
    }
    fn fixed_now_ms() -> i64 {
        DateTime::parse_from_rfc3339("2026-04-20T10:00:00Z")
            .unwrap()
            .timestamp_millis()
    }
    fn keyslot_offer() -> SponsorKeyslotOffer {
        SponsorKeyslotOffer {
            space_id: SpaceId::from_str("space-xyz"),
            keyslot_blob: vec![0xAA; 16],
            challenge: vec![0x42; 32],
            pairing_session_id: PairingSessionId::new("session-1"),
        }
    }
    fn sponsor_confirm() -> SponsorConfirm {
        SponsorConfirm {
            space_id: SpaceId::from_str("space-xyz"),
            sender_device_id: DeviceId::new("sponsor-device"),
            sender_device_name: "sponsor's laptop".into(),
            sender_identity_fingerprint: sponsor_fp(),
        }
    }

    struct Bundle {
        session: Arc<ScriptedSession>,
        space_access: Arc<ScriptedSpaceAccess>,
        settings: Arc<StubSettings>,
        setup_status: Arc<RecordingSetupStatus>,
        member_repo: Arc<RecordingMemberRepo>,
        trust_repo: Arc<RecordingTrustRepo>,
    }

    impl Bundle {
        fn happy() -> Self {
            Self {
                session: Arc::new(ScriptedSession::with_dial_ok("session-1")),
                space_access: Arc::new(ScriptedSpaceAccess::ok()),
                settings: Arc::new(StubSettings::named("joiner-laptop")),
                setup_status: Arc::new(RecordingSetupStatus::default()),
                member_repo: Arc::new(RecordingMemberRepo::default()),
                trust_repo: Arc::new(RecordingTrustRepo::default()),
            }
        }

        fn with_dial_err(err: DialError) -> Self {
            let mut b = Self::happy();
            b.session = Arc::new(ScriptedSession::with_dial_err(err));
            b
        }

        fn build(&self) -> RedeemPairingInvitationUseCase {
            let admit_uc = Arc::new(AdmitMemberUseCase::new(
                self.member_repo.clone() as Arc<dyn MemberRepositoryPort>
            ));
            let trust_uc = Arc::new(TrustPeerUseCase::new(
                self.trust_repo.clone() as Arc<dyn TrustedPeerRepositoryPort>
            ));
            RedeemPairingInvitationUseCase::new(
                self.session.clone(),
                self.space_access.clone(),
                Arc::new(FixedProof(vec![0xFE; 32])),
                Arc::new(FixedLocal(joiner_fp())),
                Arc::new(FixedDevice(DeviceId::new("joiner-device"))),
                self.settings.clone(),
                self.setup_status.clone(),
                admit_uc,
                trust_uc,
                Arc::new(FakeClock(fixed_now_ms())),
                TEST_TTL,
            )
        }
    }

    fn cmd(code: &str) -> RedeemPairingInvitationCommand {
        RedeemPairingInvitationCommand {
            code: InvitationCode::new(code),
            passphrase: Passphrase::new("hunter22hunter22"),
        }
    }

    // ── happy path ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn happy_path_admit_trust_mark_setup_and_close() {
        let b = Bundle::happy();
        b.session
            .push_recv(RecvStep::Msg(PairingSessionMessage::KeyslotOffer(
                keyslot_offer(),
            )));
        b.session
            .push_recv(RecvStep::Msg(PairingSessionMessage::Confirm(
                sponsor_confirm(),
            )));
        let uc = b.build();

        let out = uc.execute(cmd("CODE-1")).await.unwrap();

        // Result facts
        assert_eq!(out.sponsor_device_id.as_str(), "sponsor-device");
        assert_eq!(out.sponsor_identity_fingerprint, sponsor_fp());
        assert_eq!(out.space_id.inner(), "space-xyz");
        assert_eq!(out.self_device_id.as_str(), "joiner-device");
        assert_eq!(out.self_identity_fingerprint, joiner_fp());

        // Wire: Request then ChallengeResponse (2 sends), close once.
        let sent = b.session.sent();
        assert_eq!(sent.len(), 2);
        assert!(matches!(sent[0].1, PairingSessionMessage::Request(_)));
        match &sent[0].1 {
            PairingSessionMessage::Request(r) => {
                assert_eq!(r.invitation_code.as_str(), "CODE-1");
                assert_eq!(r.device_id.as_str(), "joiner-device");
                assert_eq!(r.device_name, "joiner-laptop");
                assert_eq!(r.identity_fingerprint, joiner_fp());
            }
            _ => unreachable!(),
        }
        assert!(matches!(
            sent[1].1,
            PairingSessionMessage::ChallengeResponse(JoinerChallengeResponse { .. })
        ));
        assert_eq!(b.session.closed().len(), 1);

        // Persistence: admit → trust → setup-status (all landed; order
        // is enforced by drive() so each Vec's first element is what
        // went in first).
        assert_eq!(b.member_repo.saved.lock().unwrap().len(), 1);
        assert_eq!(b.trust_repo.saved.lock().unwrap().len(), 1);
        let trusted = &b.trust_repo.saved.lock().unwrap()[0];
        assert_eq!(trusted.local_device_id.as_str(), "joiner-device");
        assert_eq!(trusted.peer_device_id.as_str(), "sponsor-device");

        assert_eq!(
            *b.setup_status.set_calls.lock().unwrap(),
            vec![true],
            "setup_status set exactly once, with has_completed=true"
        );
    }

    // ── dial errors ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn dial_invitation_not_found_maps_error_and_no_wire_activity() {
        let b = Bundle::with_dial_err(DialError::InvitationNotFound);
        let uc = b.build();
        let err = uc.execute(cmd("X")).await.unwrap_err();
        assert!(matches!(
            err,
            RedeemPairingInvitationError::InvitationNotFound
        ));
        // Nothing sent, nothing closed (session wasn't created).
        assert!(b.session.sent().is_empty());
        assert!(b.session.closed().is_empty());
    }

    #[tokio::test]
    async fn dial_invitation_expired_maps() {
        let b = Bundle::with_dial_err(DialError::InvitationExpired);
        let err = b.build().execute(cmd("X")).await.unwrap_err();
        assert!(matches!(
            err,
            RedeemPairingInvitationError::InvitationExpired
        ));
    }

    #[tokio::test]
    async fn dial_sponsor_unreachable_maps() {
        let b = Bundle::with_dial_err(DialError::SponsorUnreachable);
        let err = b.build().execute(cmd("X")).await.unwrap_err();
        assert!(matches!(
            err,
            RedeemPairingInvitationError::SponsorUnreachable
        ));
    }

    #[tokio::test]
    async fn dial_service_unavailable_maps() {
        let b = Bundle::with_dial_err(DialError::ServiceUnavailable);
        let err = b.build().execute(cmd("X")).await.unwrap_err();
        assert!(matches!(
            err,
            RedeemPairingInvitationError::ServiceUnavailable
        ));
    }

    // ── sponsor rejects ──────────────────────────────────────────────────

    #[tokio::test]
    async fn sponsor_reject_invitation_mismatch_after_request() {
        let b = Bundle::happy();
        b.session
            .push_recv(RecvStep::Msg(PairingSessionMessage::Reject(
                PairingReject {
                    reason: PairingRejectReason::InvitationMismatch,
                },
            )));
        let err = b.build().execute(cmd("X")).await.unwrap_err();
        assert!(matches!(
            err,
            RedeemPairingInvitationError::SponsorRejectedInvitation
        ));
        // One send (Request) and close on error path.
        assert_eq!(b.session.sent().len(), 1);
        assert_eq!(b.session.closed().len(), 1);
        // Nothing persisted.
        assert!(b.member_repo.saved.lock().unwrap().is_empty());
        assert!(b.trust_repo.saved.lock().unwrap().is_empty());
        assert!(b.setup_status.set_calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn sponsor_reject_passphrase_mismatch_after_challenge() {
        let b = Bundle::happy();
        b.session
            .push_recv(RecvStep::Msg(PairingSessionMessage::KeyslotOffer(
                keyslot_offer(),
            )));
        b.session
            .push_recv(RecvStep::Msg(PairingSessionMessage::Reject(
                PairingReject {
                    reason: PairingRejectReason::PassphraseMismatch,
                },
            )));
        let err = b.build().execute(cmd("X")).await.unwrap_err();
        assert!(matches!(
            err,
            RedeemPairingInvitationError::PassphraseMismatch
        ));
        // 2 sends (Request + ChallengeResponse), no persistence.
        assert_eq!(b.session.sent().len(), 2);
        assert!(b.member_repo.saved.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn sponsor_reject_timeout_surfaces_sponsor_timed_out() {
        let b = Bundle::happy();
        b.session
            .push_recv(RecvStep::Msg(PairingSessionMessage::KeyslotOffer(
                keyslot_offer(),
            )));
        b.session
            .push_recv(RecvStep::Msg(PairingSessionMessage::Reject(
                PairingReject {
                    reason: PairingRejectReason::Timeout,
                },
            )));
        let err = b.build().execute(cmd("X")).await.unwrap_err();
        assert!(matches!(err, RedeemPairingInvitationError::SponsorTimedOut));
    }

    #[tokio::test]
    async fn sponsor_reject_user_rejected_maps_to_sponsor_declined() {
        let b = Bundle::happy();
        b.session
            .push_recv(RecvStep::Msg(PairingSessionMessage::Reject(
                PairingReject {
                    reason: PairingRejectReason::UserRejected,
                },
            )));
        let err = b.build().execute(cmd("X")).await.unwrap_err();
        assert!(matches!(err, RedeemPairingInvitationError::SponsorDeclined));
    }

    #[tokio::test]
    async fn sponsor_reject_internal_carries_message() {
        let b = Bundle::happy();
        b.session
            .push_recv(RecvStep::Msg(PairingSessionMessage::Reject(
                PairingReject {
                    reason: PairingRejectReason::Internal("oops".into()),
                },
            )));
        let err = b.build().execute(cmd("X")).await.unwrap_err();
        match err {
            RedeemPairingInvitationError::SponsorInternal(m) => assert_eq!(m, "oops"),
            other => panic!("expected SponsorInternal, got {other:?}"),
        }
    }

    // ── own TTL ──────────────────────────────────────────────────────────

    #[tokio::test(start_paused = true)]
    async fn ttl_fires_on_first_recv_when_sponsor_silent() {
        let b = Bundle::happy();
        b.session.push_recv(RecvStep::Hang);
        let uc = b.build();
        // Spawn execute in background so we can advance time.
        let handle = tokio::spawn(async move { uc.execute(cmd("X")).await });
        tokio::time::sleep(TEST_TTL + ChronoDuration::seconds(1).to_std().unwrap()).await;
        let err = handle.await.unwrap().unwrap_err();
        assert!(matches!(err, RedeemPairingInvitationError::Timeout));
    }

    #[tokio::test(start_paused = true)]
    async fn ttl_fires_on_second_recv_when_sponsor_silent_after_offer() {
        let b = Bundle::happy();
        b.session
            .push_recv(RecvStep::Msg(PairingSessionMessage::KeyslotOffer(
                keyslot_offer(),
            )));
        b.session.push_recv(RecvStep::Hang);
        let sent_handle = b.session.clone();
        let closed_handle = b.session.clone();
        let uc = b.build();
        let handle = tokio::spawn(async move { uc.execute(cmd("X")).await });
        tokio::time::sleep(TEST_TTL + ChronoDuration::seconds(1).to_std().unwrap()).await;
        let err = handle.await.unwrap().unwrap_err();
        assert!(matches!(err, RedeemPairingInvitationError::Timeout));
        // Both sends went out before we hit the TTL.
        assert_eq!(sent_handle.sent().len(), 2);
        // Error path closes the session.
        assert_eq!(closed_handle.closed().len(), 1);
    }

    // ── local derive / build failures ────────────────────────────────────

    #[tokio::test]
    async fn local_wrong_passphrase_maps_to_passphrase_mismatch() {
        let mut b = Bundle::happy();
        b.space_access = Arc::new(ScriptedSpaceAccess::with_err(
            SpaceAccessError::WrongPassphrase,
        ));
        b.session
            .push_recv(RecvStep::Msg(PairingSessionMessage::KeyslotOffer(
                keyslot_offer(),
            )));
        let err = b.build().execute(cmd("X")).await.unwrap_err();
        assert!(matches!(
            err,
            RedeemPairingInvitationError::PassphraseMismatch
        ));
        // Only Request was sent — we never reached ChallengeResponse.
        assert_eq!(b.session.sent().len(), 1);
    }

    #[tokio::test]
    async fn local_corrupted_keyslot_maps_to_corrupted() {
        let mut b = Bundle::happy();
        b.space_access = Arc::new(ScriptedSpaceAccess::with_err(
            SpaceAccessError::CorruptedKeyMaterial,
        ));
        b.session
            .push_recv(RecvStep::Msg(PairingSessionMessage::KeyslotOffer(
                keyslot_offer(),
            )));
        let err = b.build().execute(cmd("X")).await.unwrap_err();
        assert!(matches!(
            err,
            RedeemPairingInvitationError::CorruptedKeyMaterial
        ));
    }

    // ── connection / protocol errors ─────────────────────────────────────

    #[tokio::test]
    async fn connection_closed_before_offer_surfaces_connection_lost() {
        let b = Bundle::happy();
        b.session.push_recv(RecvStep::CleanClose);
        let err = b.build().execute(cmd("X")).await.unwrap_err();
        assert!(matches!(err, RedeemPairingInvitationError::ConnectionLost));
    }

    #[tokio::test]
    async fn session_error_during_recv_maps_to_connection_lost() {
        let b = Bundle::happy();
        b.session.push_recv(RecvStep::Err(SessionError::Closed));
        let err = b.build().execute(cmd("X")).await.unwrap_err();
        assert!(matches!(err, RedeemPairingInvitationError::ConnectionLost));
    }

    #[tokio::test]
    async fn unexpected_first_frame_surfaces_internal() {
        let b = Bundle::happy();
        b.session
            .push_recv(RecvStep::Msg(PairingSessionMessage::Confirm(
                sponsor_confirm(),
            )));
        let err = b.build().execute(cmd("X")).await.unwrap_err();
        match err {
            RedeemPairingInvitationError::Internal(m) => {
                assert!(m.contains("expected KeyslotOffer"), "msg = {m}")
            }
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn unexpected_second_frame_surfaces_internal() {
        let b = Bundle::happy();
        b.session
            .push_recv(RecvStep::Msg(PairingSessionMessage::KeyslotOffer(
                keyslot_offer(),
            )));
        // Joiner's own `Request` bounced back = protocol violation.
        b.session
            .push_recv(RecvStep::Msg(PairingSessionMessage::Request(
                JoinerRequest {
                    invitation_code: InvitationCode::new("X"),
                    device_id: DeviceId::new("x"),
                    device_name: "x".into(),
                    identity_fingerprint: joiner_fp(),
                    nonce: vec![],
                },
            )));
        let err = b.build().execute(cmd("X")).await.unwrap_err();
        match err {
            RedeemPairingInvitationError::Internal(m) => {
                assert!(m.contains("expected Confirm"), "msg = {m}")
            }
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    // ── settings / local facts ───────────────────────────────────────────

    #[tokio::test]
    async fn device_name_missing_short_circuits_before_any_wire_send() {
        let mut b = Bundle::happy();
        b.settings = Arc::new(StubSettings::blank());
        // Session dials OK, then we bail before sending Request.
        let err = b.build().execute(cmd("X")).await.unwrap_err();
        assert!(matches!(
            err,
            RedeemPairingInvitationError::DeviceNameRequired
        ));
        assert!(b.session.sent().is_empty());
        // Session was dialled, so close must run on the error path.
        assert_eq!(b.session.closed().len(), 1);
    }

    // ── admit/trust failures ─────────────────────────────────────────────

    #[tokio::test]
    async fn admit_failure_after_confirm_surfaces_internal_no_trust_no_setup() {
        let b = Bundle::happy();
        *b.member_repo.fail_next.lock().unwrap() =
            Some(MembershipError::Repository("db down".into()));
        b.session
            .push_recv(RecvStep::Msg(PairingSessionMessage::KeyslotOffer(
                keyslot_offer(),
            )));
        b.session
            .push_recv(RecvStep::Msg(PairingSessionMessage::Confirm(
                sponsor_confirm(),
            )));
        let err = b.build().execute(cmd("X")).await.unwrap_err();
        match err {
            RedeemPairingInvitationError::Internal(m) => {
                assert!(m.contains("admit_member"), "msg = {m}")
            }
            other => panic!("expected Internal, got {other:?}"),
        }
        // Trust must NOT run when admit failed; setup_status NOT flipped.
        assert!(b.member_repo.saved.lock().unwrap().is_empty());
        assert!(b.trust_repo.saved.lock().unwrap().is_empty());
        assert!(b.setup_status.set_calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn trust_failure_after_admit_surfaces_internal_setup_not_marked() {
        let b = Bundle::happy();
        *b.trust_repo.fail_next.lock().unwrap() =
            Some(TrustedPeerError::Repository("trust boom".into()));
        b.session
            .push_recv(RecvStep::Msg(PairingSessionMessage::KeyslotOffer(
                keyslot_offer(),
            )));
        b.session
            .push_recv(RecvStep::Msg(PairingSessionMessage::Confirm(
                sponsor_confirm(),
            )));
        let err = b.build().execute(cmd("X")).await.unwrap_err();
        match err {
            RedeemPairingInvitationError::Internal(m) => {
                assert!(m.contains("trust_peer"), "msg = {m}")
            }
            other => panic!("expected Internal, got {other:?}"),
        }
        // Admit landed (the asymmetric side of Slice 1 "strict" persist
        // ordering — same shape as sponsor-side P7f cleanup); trust did
        // not; setup_status NOT flipped.
        assert_eq!(b.member_repo.saved.lock().unwrap().len(), 1);
        assert!(b.trust_repo.saved.lock().unwrap().is_empty());
        assert!(b.setup_status.set_calls.lock().unwrap().is_empty());
    }
}
