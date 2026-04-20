//! B2 · `RedeemPairingInvitationUseCase` (joiner side).
//!
//! Thin composition layer: delegates wire + crypto to
//! [`JoinerHandshakeCoordinator`], then persists the sponsor's facts
//! (`admit_member` → `trust_peer` → `setup_status`) and maps the
//! outcome to the facade-level result.
//!
//! ## Ordering: persist before declaring success
//!
//! Mirrors sponsor-side P7f cleanup: `admit` → `trust` →
//! `setup_status.set_status(completed)` all land **before** `execute`
//! returns `Ok`. Any persistence failure short-circuits the remaining
//! steps and surfaces as
//! [`RedeemPairingInvitationError::Internal`] — the caller never gets
//! a success result that isn't backed by fully committed local state.
//!
//! `setup_status` is flipped **last**, because `has_completed=true` is
//! the marker `UnlockSpaceUseCase` keys off on the next launch.
//! Flipping it before `trust_peer` landed would leave the policy
//! resolver seeing "setup done" but no trusted peers, blocking every
//! inbound session.
//!
//! ## Why no FSM
//!
//! See F-053: the joiner flow is linear once the passphrase is
//! collected up front (Slice 1 UX), and `SpaceAccessStateMachine`'s
//! default action order is inverted from the persist-before-success
//! ordering this use case wants.
//!
//! ## Why a coordinator below
//!
//! Extracted in post-P7h cleanup: the original 11-arg use case was
//! doing dial + identity assembly + crypto + recv/decode + admit +
//! trust + setup-status in one struct. That broke symmetry with the
//! sponsor side (which already split wire/crypto into
//! [`SponsorHandshakeCoordinator`]). The use case is now 5 deps and
//! one-to-one mirrors `PairingInboundOrchestrator`'s composition
//! shape.
//!
//! [`JoinerHandshakeCoordinator`]:
//!     crate::pairing_outbound::joiner_handshake::JoinerHandshakeCoordinator
//! [`SponsorHandshakeCoordinator`]:
//!     crate::pairing_inbound::sponsor_handshake::SponsorHandshakeCoordinator

use std::sync::Arc;

use chrono::{DateTime, Utc};
use tracing::{info, instrument};

use uc_core::ports::{ClockPort, SetupStatusPort};
use uc_core::setup::SetupStatus;
use uc_core::{MemberRepositoryPort, MemberSyncPreferences, TrustedPeerRepositoryPort};

use crate::facade::space_setup::{
    RedeemPairingInvitationCommand, RedeemPairingInvitationError, RedeemPairingInvitationResult,
};
use crate::membership::errors::MembershipApplicationError;
use crate::membership::usecases::{AdmitMember, AdmitMemberUseCase};
use crate::pairing_outbound::joiner_handshake::{
    JoinerHandshakeCoordinator, JoinerHandshakeOutcome,
};
use crate::trusted_peer::errors::TrustedPeerApplicationError;
use crate::trusted_peer::usecases::{TrustPeer, TrustPeerUseCase};

pub(crate) type AdmitMemberUc = AdmitMemberUseCase<dyn MemberRepositoryPort>;
pub(crate) type TrustPeerUc = TrustPeerUseCase<dyn TrustedPeerRepositoryPort>;

pub(crate) struct RedeemPairingInvitationUseCase {
    handshake: Arc<JoinerHandshakeCoordinator>,
    admit_member: Arc<AdmitMemberUc>,
    trust_peer: Arc<TrustPeerUc>,
    setup_status: Arc<dyn SetupStatusPort>,
    clock: Arc<dyn ClockPort>,
}

impl RedeemPairingInvitationUseCase {
    pub(crate) fn new(
        handshake: Arc<JoinerHandshakeCoordinator>,
        admit_member: Arc<AdmitMemberUc>,
        trust_peer: Arc<TrustPeerUc>,
        setup_status: Arc<dyn SetupStatusPort>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        Self {
            handshake,
            admit_member,
            trust_peer,
            setup_status,
            clock,
        }
    }

    #[instrument(skip_all, fields(code = %cmd.code.as_str()))]
    pub(crate) async fn execute(
        &self,
        cmd: RedeemPairingInvitationCommand,
    ) -> Result<RedeemPairingInvitationResult, RedeemPairingInvitationError> {
        let outcome = self.handshake.handshake(&cmd.code, &cmd.passphrase).await?;
        self.persist(outcome).await
    }

    async fn persist(
        &self,
        outcome: JoinerHandshakeOutcome,
    ) -> Result<RedeemPairingInvitationResult, RedeemPairingInvitationError> {
        let now = self.now_utc()?;

        // Admit sponsor as member.
        let admit_input = AdmitMember {
            device_id: outcome.sponsor_device_id.clone(),
            device_name: outcome.sponsor_device_name.clone(),
            identity_fingerprint: outcome.sponsor_identity_fingerprint.clone(),
            joined_at: now,
            sync_preferences: MemberSyncPreferences::default(),
        };
        self.admit_member
            .execute(admit_input)
            .await
            .map_err(map_admit_err)?;

        // Trust sponsor.
        let trust_input = TrustPeer {
            local_device_id: outcome.self_device_id.clone(),
            peer_device_id: outcome.sponsor_device_id.clone(),
            peer_fingerprint: outcome.sponsor_identity_fingerprint.clone(),
            trusted_at: now,
        };
        self.trust_peer
            .execute(trust_input)
            .await
            .map_err(map_trust_err)?;

        // Mark setup complete (ordering rationale: see module doc).
        self.setup_status
            .set_status(&SetupStatus {
                has_completed: true,
            })
            .await
            .map_err(|e| {
                RedeemPairingInvitationError::Internal(format!("setup_status.set_status: {e}"))
            })?;

        info!(
            sponsor_device_id = %outcome.sponsor_device_id.as_str(),
            space_id = %outcome.space_id,
            "joiner pairing complete; local space ready"
        );

        Ok(RedeemPairingInvitationResult {
            sponsor_device_id: outcome.sponsor_device_id,
            sponsor_identity_fingerprint: outcome.sponsor_identity_fingerprint,
            space_id: outcome.space_id,
            self_device_id: outcome.self_device_id,
            self_identity_fingerprint: outcome.self_identity_fingerprint,
        })
    }

    fn now_utc(&self) -> Result<DateTime<Utc>, RedeemPairingInvitationError> {
        DateTime::<Utc>::from_timestamp_millis(self.clock.now_ms()).ok_or_else(|| {
            RedeemPairingInvitationError::Internal("clock returned invalid timestamp".into())
        })
    }
}

fn map_admit_err(err: MembershipApplicationError) -> RedeemPairingInvitationError {
    // `AlreadyAdmitted` / `AlreadyTrusted` also land here: we can't
    // distinguish a retry-of-completed-run from a half-committed-state
    // without a separate resume flag, so fail loudly. Recovery path is
    // a factory_reset followed by a fresh redeem.
    RedeemPairingInvitationError::Internal(format!("admit_member: {err}"))
}

fn map_trust_err(err: TrustedPeerApplicationError) -> RedeemPairingInvitationError {
    RedeemPairingInvitationError::Internal(format!("trust_peer: {err}"))
}

#[cfg(test)]
mod tests {
    //! Composition tests only: wire + crypto covered in
    //! [`crate::pairing_outbound::joiner_handshake::tests`]. Here we
    //! verify that a coordinator outcome drives admit → trust →
    //! setup-status in the right order, and that each step's failure
    //! short-circuits the remaining ones without flipping
    //! `setup_status`.
    //!
    //! To avoid mocking the coordinator behind a trait (symmetric with
    //! how sponsor-side orchestrator tests use a real
    //! `SponsorHandshakeCoordinator`), these tests construct a real
    //! `JoinerHandshakeCoordinator` with scripted session/crypto fakes
    //! that deliver a happy-path outcome. That's a small amount of
    //! wire-test overlap with the coordinator's own tests, but keeps
    //! the use case under the same seams production uses.
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex as StdMutex;

    use async_trait::async_trait;

    use uc_core::crypto::domain::{ActiveSpace, Passphrase};
    use uc_core::ids::{DeviceId, SessionId, SpaceId};
    use uc_core::membership::{MembershipError, SpaceMember};
    use uc_core::pairing::invitation::InvitationCode;
    use uc_core::pairing::session_message::{
        PairingSessionMessage, SponsorConfirm, SponsorKeyslotOffer,
    };
    use uc_core::ports::pairing::{DialError, PairingSessionId, PairingSessionPort, SessionError};
    use uc_core::ports::space::{ProofPort, SpaceAccessError, SpaceAccessPort};
    use uc_core::ports::{DeviceIdentityPort, LocalIdentityError, LocalIdentityPort, SettingsPort};
    use uc_core::security::IdentityFingerprint;
    use uc_core::settings::model::Settings;
    use uc_core::space_access::domain::{JoinOffer, ProofDerivedKey, SpaceAccessProofArtifact};
    use uc_core::trusted_peer::{TrustedPeer, TrustedPeerError};

    use chrono::DateTime;
    use tokio::time::Duration;

    // ── minimal wire fakes (for producing a happy-path outcome) ──────────

    #[derive(Default)]
    struct HappySession {
        sent: StdMutex<Vec<PairingSessionMessage>>,
        recv: StdMutex<VecDeque<PairingSessionMessage>>,
        closed: StdMutex<u32>,
    }
    impl HappySession {
        fn primed() -> Self {
            let me = Self::default();
            me.recv
                .lock()
                .unwrap()
                .push_back(PairingSessionMessage::KeyslotOffer(SponsorKeyslotOffer {
                    space_id: SpaceId::from_str("space-xyz"),
                    keyslot_blob: vec![0xAA; 16],
                    challenge: vec![0x42; 32],
                    pairing_session_id: PairingSessionId::new("session-1"),
                }));
            me.recv
                .lock()
                .unwrap()
                .push_back(PairingSessionMessage::Confirm(SponsorConfirm {
                    space_id: SpaceId::from_str("space-xyz"),
                    sender_device_id: DeviceId::new("sponsor-device"),
                    sender_device_name: "sponsor's laptop".into(),
                    sender_identity_fingerprint: sponsor_fp(),
                }));
            me
        }
    }
    #[async_trait]
    impl PairingSessionPort for HappySession {
        async fn dial_by_invitation(
            &self,
            _: &InvitationCode,
        ) -> Result<PairingSessionId, DialError> {
            Ok(PairingSessionId::new("session-1"))
        }
        async fn send(
            &self,
            _: &PairingSessionId,
            m: PairingSessionMessage,
        ) -> Result<(), SessionError> {
            self.sent.lock().unwrap().push(m);
            Ok(())
        }
        async fn recv_next(
            &self,
            _: &PairingSessionId,
        ) -> Result<Option<PairingSessionMessage>, SessionError> {
            Ok(self.recv.lock().unwrap().pop_front())
        }
        async fn close(&self, _: &PairingSessionId, _: Option<String>) {
            *self.closed.lock().unwrap() += 1;
        }
    }

    struct UnreachableSession;
    #[async_trait]
    impl PairingSessionPort for UnreachableSession {
        async fn dial_by_invitation(
            &self,
            _: &InvitationCode,
        ) -> Result<PairingSessionId, DialError> {
            Err(DialError::InvitationNotFound)
        }
        async fn send(
            &self,
            _: &PairingSessionId,
            _: PairingSessionMessage,
        ) -> Result<(), SessionError> {
            unreachable!("dial fails before send")
        }
        async fn recv_next(
            &self,
            _: &PairingSessionId,
        ) -> Result<Option<PairingSessionMessage>, SessionError> {
            unreachable!("dial fails before recv")
        }
        async fn close(&self, _: &PairingSessionId, _: Option<String>) {
            unreachable!("no session to close")
        }
    }

    struct HappySpaceAccess;
    #[async_trait]
    impl SpaceAccessPort for HappySpaceAccess {
        async fn initialize(
            &self,
            _: &SpaceId,
            _: &Passphrase,
        ) -> Result<ActiveSpace, SpaceAccessError> {
            unimplemented!()
        }
        async fn unlock(
            &self,
            _: &SpaceId,
            _: &Passphrase,
        ) -> Result<ActiveSpace, SpaceAccessError> {
            unimplemented!()
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
        ) -> Result<JoinOffer, SpaceAccessError> {
            unimplemented!()
        }
        async fn derive_master_key_for_proof(
            &self,
            _: &JoinOffer,
            _: &Passphrase,
        ) -> Result<ProofDerivedKey, SpaceAccessError> {
            Ok(ProofDerivedKey::from_bytes([0xCC; 32]))
        }
    }

    struct FixedProof;
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
                proof_bytes: vec![0xFE; 32],
            })
        }
        async fn verify_proof(
            &self,
            _: &SpaceAccessProofArtifact,
            _: [u8; 32],
        ) -> anyhow::Result<bool> {
            unimplemented!()
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

    struct NamedSettings(String);
    #[async_trait]
    impl SettingsPort for NamedSettings {
        async fn load(&self) -> anyhow::Result<Settings> {
            let mut s = Settings::default();
            s.general.device_name = Some(self.0.clone());
            Ok(s)
        }
        async fn save(&self, _: &Settings) -> anyhow::Result<()> {
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
        async fn save(&self, m: &SpaceMember) -> Result<(), MembershipError> {
            if let Some(err) = self.fail_next.lock().unwrap().take() {
                return Err(err);
            }
            self.saved.lock().unwrap().push(m.clone());
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
        async fn save(&self, p: &TrustedPeer) -> Result<(), TrustedPeerError> {
            if let Some(err) = self.fail_next.lock().unwrap().take() {
                return Err(err);
            }
            self.saved.lock().unwrap().push(p.clone());
            Ok(())
        }
        async fn remove(&self, _: &DeviceId) -> Result<bool, TrustedPeerError> {
            Ok(false)
        }
    }

    struct RecordingSetupStatus {
        fail_next: StdMutex<bool>,
        set_calls: StdMutex<Vec<bool>>,
    }
    impl RecordingSetupStatus {
        fn ok() -> Self {
            Self {
                fail_next: StdMutex::new(false),
                set_calls: StdMutex::new(Vec::new()),
            }
        }
        fn failing() -> Self {
            Self {
                fail_next: StdMutex::new(true),
                set_calls: StdMutex::new(Vec::new()),
            }
        }
    }
    #[async_trait]
    impl SetupStatusPort for RecordingSetupStatus {
        async fn get_status(&self) -> anyhow::Result<SetupStatus> {
            Ok(SetupStatus::default())
        }
        async fn set_status(&self, s: &SetupStatus) -> anyhow::Result<()> {
            if *self.fail_next.lock().unwrap() {
                return Err(anyhow::anyhow!("setup-status backend down"));
            }
            self.set_calls.lock().unwrap().push(s.has_completed);
            Ok(())
        }
    }

    struct FixedClock(i64);
    impl ClockPort for FixedClock {
        fn now_ms(&self) -> i64 {
            self.0
        }
    }

    // ── fixtures ─────────────────────────────────────────────────────────

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
    fn cmd(code: &str) -> RedeemPairingInvitationCommand {
        RedeemPairingInvitationCommand {
            code: InvitationCode::new(code),
            passphrase: Passphrase::new("hunter22hunter22"),
        }
    }

    struct Harness {
        session: Arc<HappySession>,
        member_repo: Arc<RecordingMemberRepo>,
        trust_repo: Arc<RecordingTrustRepo>,
        setup_status: Arc<RecordingSetupStatus>,
    }

    impl Harness {
        fn build(
            session: Arc<dyn PairingSessionPort>,
            session_handle: Arc<HappySession>,
            member_repo: Arc<RecordingMemberRepo>,
            trust_repo: Arc<RecordingTrustRepo>,
            setup_status: Arc<RecordingSetupStatus>,
        ) -> (RedeemPairingInvitationUseCase, Self) {
            let handshake = JoinerHandshakeCoordinator::new(
                session,
                Arc::new(HappySpaceAccess),
                Arc::new(FixedProof),
                Arc::new(FixedLocal(joiner_fp())),
                Arc::new(FixedDevice(DeviceId::new("joiner-device"))),
                Arc::new(NamedSettings("joiner-laptop".into())),
                Duration::from_secs(30),
            );
            let admit_uc = Arc::new(AdmitMemberUseCase::new(
                member_repo.clone() as Arc<dyn MemberRepositoryPort>
            ));
            let trust_uc = Arc::new(TrustPeerUseCase::new(
                trust_repo.clone() as Arc<dyn TrustedPeerRepositoryPort>
            ));
            let uc = RedeemPairingInvitationUseCase::new(
                handshake,
                admit_uc,
                trust_uc,
                setup_status.clone(),
                Arc::new(FixedClock(fixed_now_ms())),
            );
            (
                uc,
                Self {
                    session: session_handle,
                    member_repo,
                    trust_repo,
                    setup_status,
                },
            )
        }

        fn happy() -> (RedeemPairingInvitationUseCase, Self) {
            let session = Arc::new(HappySession::primed());
            Self::build(
                session.clone(),
                session,
                Arc::new(RecordingMemberRepo::default()),
                Arc::new(RecordingTrustRepo::default()),
                Arc::new(RecordingSetupStatus::ok()),
            )
        }
    }

    // ── tests ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn happy_path_admit_trust_mark_setup_and_return_facts() {
        let (uc, h) = Harness::happy();
        let out = uc.execute(cmd("CODE-1")).await.unwrap();
        assert_eq!(out.sponsor_device_id.as_str(), "sponsor-device");
        assert_eq!(out.sponsor_identity_fingerprint, sponsor_fp());
        assert_eq!(out.space_id.inner(), "space-xyz");
        assert_eq!(out.self_device_id.as_str(), "joiner-device");
        assert_eq!(out.self_identity_fingerprint, joiner_fp());

        assert_eq!(h.member_repo.saved.lock().unwrap().len(), 1);
        let trusted = &h.trust_repo.saved.lock().unwrap()[0];
        assert_eq!(trusted.local_device_id.as_str(), "joiner-device");
        assert_eq!(trusted.peer_device_id.as_str(), "sponsor-device");
        assert_eq!(
            *h.setup_status.set_calls.lock().unwrap(),
            vec![true],
            "setup_status flipped exactly once to has_completed=true"
        );
        assert_eq!(*h.session.closed.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn coordinator_error_passes_through_without_touching_persistence() {
        let session = Arc::new(HappySession::default()); // not primed — won't be used
        let unreachable: Arc<dyn PairingSessionPort> = Arc::new(UnreachableSession);
        let (uc, h) = Harness::build(
            unreachable,
            session, // handle kept around so the Harness's `session.closed` stays consistent with the "no close" assertion
            Arc::new(RecordingMemberRepo::default()),
            Arc::new(RecordingTrustRepo::default()),
            Arc::new(RecordingSetupStatus::ok()),
        );
        let err = uc.execute(cmd("X")).await.unwrap_err();
        assert!(matches!(
            err,
            RedeemPairingInvitationError::InvitationNotFound
        ));
        // Persistence untouched.
        assert!(h.member_repo.saved.lock().unwrap().is_empty());
        assert!(h.trust_repo.saved.lock().unwrap().is_empty());
        assert!(h.setup_status.set_calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn admit_failure_aborts_before_trust_and_setup_status() {
        let member_repo = Arc::new(RecordingMemberRepo::default());
        *member_repo.fail_next.lock().unwrap() =
            Some(MembershipError::Repository("db down".into()));
        let session = Arc::new(HappySession::primed());
        let (uc, h) = Harness::build(
            session.clone(),
            session,
            member_repo,
            Arc::new(RecordingTrustRepo::default()),
            Arc::new(RecordingSetupStatus::ok()),
        );
        let err = uc.execute(cmd("X")).await.unwrap_err();
        match err {
            RedeemPairingInvitationError::Internal(m) => {
                assert!(m.contains("admit_member"), "msg = {m}")
            }
            other => panic!("expected Internal, got {other:?}"),
        }
        assert!(h.member_repo.saved.lock().unwrap().is_empty());
        assert!(h.trust_repo.saved.lock().unwrap().is_empty());
        assert!(h.setup_status.set_calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn trust_failure_lands_admit_but_does_not_mark_setup() {
        let trust_repo = Arc::new(RecordingTrustRepo::default());
        *trust_repo.fail_next.lock().unwrap() =
            Some(TrustedPeerError::Repository("trust boom".into()));
        let session = Arc::new(HappySession::primed());
        let (uc, h) = Harness::build(
            session.clone(),
            session,
            Arc::new(RecordingMemberRepo::default()),
            trust_repo,
            Arc::new(RecordingSetupStatus::ok()),
        );
        let err = uc.execute(cmd("X")).await.unwrap_err();
        match err {
            RedeemPairingInvitationError::Internal(m) => {
                assert!(m.contains("trust_peer"), "msg = {m}")
            }
            other => panic!("expected Internal, got {other:?}"),
        }
        // admit landed (Slice 1 "strict" ordering — no admit-rollback
        // compensation; the user-visible surface is the Internal error,
        // and recovery path is factory_reset + fresh redeem).
        assert_eq!(h.member_repo.saved.lock().unwrap().len(), 1);
        assert!(h.trust_repo.saved.lock().unwrap().is_empty());
        assert!(h.setup_status.set_calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn setup_status_failure_lands_admit_and_trust_but_surfaces_internal() {
        let session = Arc::new(HappySession::primed());
        let (uc, h) = Harness::build(
            session.clone(),
            session,
            Arc::new(RecordingMemberRepo::default()),
            Arc::new(RecordingTrustRepo::default()),
            Arc::new(RecordingSetupStatus::failing()),
        );
        let err = uc.execute(cmd("X")).await.unwrap_err();
        match err {
            RedeemPairingInvitationError::Internal(m) => {
                assert!(m.contains("setup_status.set_status"), "msg = {m}")
            }
            other => panic!("expected Internal, got {other:?}"),
        }
        // admit + trust both landed; setup_status call was attempted
        // (and failed), so set_calls stays empty.
        assert_eq!(h.member_repo.saved.lock().unwrap().len(), 1);
        assert_eq!(h.trust_repo.saved.lock().unwrap().len(), 1);
        assert!(h.setup_status.set_calls.lock().unwrap().is_empty());
    }
}
