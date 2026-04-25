#![allow(deprecated)] // bridges legacy PairingTransportPort; replaced in Slice 5

//! Test-only scaffolding for setup UseCases (phase B.6).
//!
//! Assembles a `SetupOrchestrator` with a minimal set of fake ports so that
//! UseCase-granularity tests can exercise the dispatch path without pulling
//! in real infrastructure (network / crypto / pairing / space access).
//!
//! The fakes are intentionally the narrowest possible implementation that
//! satisfies each port contract — most methods are noops that return `Ok(())`.
//! Tests that need richer behavior (recording, error injection) use the
//! handles returned by [`TestHarness`] to introspect state after dispatch.

#![cfg(test)]
#![allow(dead_code)]

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::{mpsc, Mutex};

use uc_core::crypto::domain::{ActiveSpace, Passphrase as DomainPassphrase};
use uc_core::ids::{SessionId, SpaceId};
use uc_core::network::PairingMessage;
use uc_core::ports::space::{
    PersistencePort, ProofPort, SpaceAccessError, SpaceAccessPort, SpaceAccessTransportPort,
};
use uc_core::ports::{NetworkControlPort, PairingTransportPort, SetupStatusPort, TimerPort};
use uc_core::setup::SetupStatus;
use uc_core::space_access::{JoinOffer, ProofDerivedKey, SpaceAccessProofArtifact};

use super::event_port::SetupEventPort;
use super::orchestrator::SetupOrchestrator;
use super::pairing_facade::SetupPairingFacadePort;
use super::ports::SetupAppLifecyclePort;
use super::state::SetupState;
use crate::pairing::PairingDomainEvent;
use crate::space_access::SpaceAccessFacade;

// ────────────────────────── Fake ports ──────────────────────────

pub(crate) struct FakeSetupStatus {
    inner: Mutex<SetupStatus>,
}

impl FakeSetupStatus {
    pub fn new(status: SetupStatus) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(status),
        })
    }

    pub fn default_not_completed() -> Arc<Self> {
        Self::new(SetupStatus::default())
    }

    pub fn completed() -> Arc<Self> {
        Self::new(SetupStatus {
            has_completed: true,
            space_id: None,
        })
    }

    pub async fn snapshot(&self) -> SetupStatus {
        self.inner.lock().await.clone()
    }
}

#[async_trait]
impl SetupStatusPort for FakeSetupStatus {
    async fn get_status(&self) -> Result<SetupStatus> {
        Ok(self.inner.lock().await.clone())
    }

    async fn set_status(&self, status: &SetupStatus) -> Result<()> {
        *self.inner.lock().await = status.clone();
        Ok(())
    }
}

pub(crate) struct FakeAppLifecycle {
    pub calls: Mutex<u32>,
}

impl FakeAppLifecycle {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            calls: Mutex::new(0),
        })
    }
}

#[async_trait]
impl SetupAppLifecyclePort for FakeAppLifecycle {
    async fn ensure_ready(&self) -> Result<()> {
        *self.calls.lock().await += 1;
        Ok(())
    }
}

pub(crate) struct FakeSetupEvents {
    pub emissions: Mutex<Vec<(SetupState, Option<String>)>>,
}

impl FakeSetupEvents {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            emissions: Mutex::new(Vec::new()),
        })
    }

    pub async fn snapshot(&self) -> Vec<(SetupState, Option<String>)> {
        self.emissions.lock().await.clone()
    }
}

#[async_trait]
impl SetupEventPort for FakeSetupEvents {
    async fn emit_setup_state_changed(&self, state: SetupState, session_id: Option<String>) {
        self.emissions.lock().await.push((state, session_id));
    }
}

pub(crate) struct FakePairingFacade {
    pub initiate_err: bool,
}

impl FakePairingFacade {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            initiate_err: false,
        })
    }

    pub fn failing_initiate() -> Arc<Self> {
        Arc::new(Self { initiate_err: true })
    }
}

#[async_trait]
impl SetupPairingFacadePort for FakePairingFacade {
    async fn subscribe(&self) -> Result<mpsc::Receiver<PairingDomainEvent>> {
        let (_tx, rx) = mpsc::channel(8);
        Ok(rx)
    }

    async fn initiate_pairing(&self, _peer_id: String) -> Result<String> {
        if self.initiate_err {
            Err(anyhow::anyhow!("fake initiate pairing failure"))
        } else {
            Ok("fake-session".to_string())
        }
    }

    async fn accept_pairing(&self, _session_id: &str) -> Result<()> {
        Ok(())
    }

    async fn reject_pairing(&self, _session_id: &str) -> Result<()> {
        Ok(())
    }

    async fn cancel_pairing(&self, _session_id: &str) -> Result<()> {
        Ok(())
    }

    async fn verify_pairing(&self, _session_id: &str, _pin_matches: bool) -> Result<()> {
        Ok(())
    }
}

pub(crate) struct FakeNetworkControl;

#[async_trait]
impl NetworkControlPort for FakeNetworkControl {
    async fn start_network(&self) -> Result<()> {
        Ok(())
    }
}

pub(crate) struct FakePairingTransport;

#[async_trait]
impl PairingTransportPort for FakePairingTransport {
    async fn open_pairing_session(&self, _peer_id: String, _session_id: String) -> Result<()> {
        Ok(())
    }

    async fn send_pairing_on_session(&self, _message: PairingMessage) -> Result<()> {
        Ok(())
    }

    async fn close_pairing_session(
        &self,
        _session_id: String,
        _reason: Option<String>,
    ) -> Result<()> {
        Ok(())
    }

    async fn unpair_device(&self, _peer_id: String) -> Result<()> {
        Ok(())
    }
}

pub(crate) struct FakeSpaceAccessTransport;

#[async_trait]
impl SpaceAccessTransportPort for FakeSpaceAccessTransport {
    async fn send_offer(&mut self, _session_id: &SessionId) -> Result<()> {
        Ok(())
    }
    async fn send_proof(&mut self, _session_id: &SessionId) -> Result<()> {
        Ok(())
    }
    async fn send_result(&mut self, _session_id: &SessionId) -> Result<()> {
        Ok(())
    }
}

pub(crate) struct FakeTimer;

#[async_trait]
impl TimerPort for FakeTimer {
    async fn start(&mut self, _session_id: &SessionId, _ttl_secs: u64) -> Result<()> {
        Ok(())
    }
    async fn stop(&mut self, _session_id: &SessionId) -> Result<()> {
        Ok(())
    }
}

pub(crate) struct FakePersistence;

#[async_trait]
impl PersistencePort for FakePersistence {
    async fn persist_joiner_access(&mut self, _space_id: &SpaceId, _peer_id: &str) -> Result<()> {
        Ok(())
    }
    async fn persist_sponsor_access(&mut self, _space_id: &SpaceId, _peer_id: &str) -> Result<()> {
        Ok(())
    }
}

pub(crate) struct FakeProof {
    pub verify_result: bool,
}

#[async_trait]
impl ProofPort for FakeProof {
    async fn build_proof(
        &self,
        pairing_session_id: &SessionId,
        space_id: &SpaceId,
        _challenge_nonce: [u8; 32],
        _derived_key: &ProofDerivedKey,
    ) -> Result<SpaceAccessProofArtifact> {
        Ok(SpaceAccessProofArtifact {
            pairing_session_id: pairing_session_id.clone(),
            space_id: space_id.clone(),
            challenge_nonce: [0u8; 32],
            proof_bytes: Vec::new(),
        })
    }

    async fn verify_proof(
        &self,
        _proof: &SpaceAccessProofArtifact,
        _expected_nonce: [u8; 32],
    ) -> Result<bool> {
        Ok(self.verify_result)
    }
}

pub(crate) struct NoopSpaceAccess;

#[async_trait]
impl SpaceAccessPort for NoopSpaceAccess {
    async fn initialize(
        &self,
        space_id: &SpaceId,
        _passphrase: &DomainPassphrase,
    ) -> Result<ActiveSpace, SpaceAccessError> {
        // Phase C: setup action `CreateEncryptedSpace` 直接调本方法(取代
        // 原 FakeInitializeEncryption 的成功桩)。Noop 语义下返回 Ok,
        // 让 submit_new_space_passphrase 测试能走到 Completed。
        Ok(ActiveSpace::new(space_id.clone()))
    }

    async fn unlock(
        &self,
        _space_id: &SpaceId,
        _passphrase: &DomainPassphrase,
    ) -> Result<ActiveSpace, SpaceAccessError> {
        Err(SpaceAccessError::Internal("noop unlock".into()))
    }

    async fn is_unlocked(&self, _space_id: &SpaceId) -> bool {
        false
    }

    async fn lock(&self, _space_id: &SpaceId) -> Result<(), SpaceAccessError> {
        Err(SpaceAccessError::Internal("noop lock".into()))
    }

    async fn factory_reset(&self, _space_id: &SpaceId) -> Result<(), SpaceAccessError> {
        Ok(())
    }

    async fn prepare_join_offer(
        &self,
        _space_id: &SpaceId,
        _passphrase: &DomainPassphrase,
    ) -> Result<JoinOffer, SpaceAccessError> {
        Err(SpaceAccessError::Internal("noop prepare_join_offer".into()))
    }

    async fn derive_master_key_for_proof(
        &self,
        _offer: &JoinOffer,
        _passphrase: &DomainPassphrase,
    ) -> Result<ProofDerivedKey, SpaceAccessError> {
        Err(SpaceAccessError::Internal(
            "noop derive_master_key_for_proof".into(),
        ))
    }

    async fn try_resume_session(
        &self,
        _space_id: &SpaceId,
    ) -> Result<Option<ActiveSpace>, SpaceAccessError> {
        Ok(None)
    }

    async fn verify_keychain_access(&self) -> Result<bool, SpaceAccessError> {
        Ok(false)
    }

    async fn derive_subkey(
        &self,
        _salt: &[u8],
        _info: &[u8],
    ) -> Result<[u8; 32], SpaceAccessError> {
        Err(SpaceAccessError::Internal("noop derive_subkey".into()))
    }

    async fn current_session_proof_key(&self) -> Result<Option<ProofDerivedKey>, SpaceAccessError> {
        Ok(None)
    }
}

// ────────────────────────── Harness ──────────────────────────

pub(crate) struct TestHarness {
    pub orchestrator: Arc<SetupOrchestrator>,
    pub events: Arc<FakeSetupEvents>,
    pub status: Arc<FakeSetupStatus>,
    pub app_lifecycle: Arc<FakeAppLifecycle>,
}

pub(crate) struct HarnessOptions {
    pub status: Arc<FakeSetupStatus>,
    pub pairing_facade: Arc<FakePairingFacade>,
    pub proof_verify: bool,
}

impl Default for HarnessOptions {
    fn default() -> Self {
        Self {
            status: FakeSetupStatus::default_not_completed(),
            pairing_facade: FakePairingFacade::new(),
            proof_verify: true,
        }
    }
}

pub(crate) fn build_harness(opts: HarnessOptions) -> TestHarness {
    let events = FakeSetupEvents::new();
    let app_lifecycle = FakeAppLifecycle::new();
    let status = opts.status.clone();
    let space_access_facade = Arc::new(SpaceAccessFacade::new());

    let orchestrator = Arc::new(SetupOrchestrator::new(
        Arc::new(super::usecases::MarkSetupCompleteUsecase::new(
            status.clone(),
        )),
        status.clone(),
        app_lifecycle.clone(),
        opts.pairing_facade,
        events.clone(),
        space_access_facade,
        Arc::new(FakeNetworkControl),
        Arc::new(NoopSpaceAccess),
        Arc::new(FakePairingTransport),
        Arc::new(Mutex::new(FakeSpaceAccessTransport)),
        Arc::new(FakeProof {
            verify_result: opts.proof_verify,
        }),
        Arc::new(Mutex::new(FakeTimer)),
        Arc::new(Mutex::new(FakePersistence)),
    ));

    TestHarness {
        orchestrator,
        events,
        status,
        app_lifecycle,
    }
}

pub(crate) fn build_default_harness() -> TestHarness {
    build_harness(HarnessOptions::default())
}

/// Force the orchestrator's internal state to a specific value, bypassing the
/// state machine. Used by tests that want to start from mid-flow states
/// (e.g. `JoinSpaceInputPassphrase`) without driving the full prelude of
/// events.
pub(crate) async fn seed_state(harness: &TestHarness, state: SetupState) {
    harness.orchestrator.context.set_state(state).await;
}

/// Seed the orchestrator's `pairing_session_id` so downstream action executors
/// that require an active session (e.g. `ConfirmPeerTrust`) can proceed.
pub(crate) async fn seed_pairing_session(harness: &TestHarness, session_id: &str) {
    *harness.orchestrator.pairing_session_id.lock().await = Some(session_id.to_string());
}

/// Seed the orchestrator's `selected_peer_id`. Used by join flows that
/// skipped the `ChooseJoinPeer` prelude.
pub(crate) async fn seed_selected_peer(harness: &TestHarness, peer_id: &str) {
    *harness.orchestrator.selected_peer_id.lock().await = Some(peer_id.to_string());
}
