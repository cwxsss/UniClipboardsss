//! `PairingFacade` — stable application-layer entry point for the pairing
//! module (phase 0.4.4).
//!
//! # Architecture
//!
//! ```text
//! External (daemon / setup) → PairingFacade → PairingOrchestrator → Ports
//!                                  │
//!                                  └─ user-intent UseCases (accept / reject / cancel)
//! ```
//!
//! The orchestrator and its user-action methods are hidden behind this
//! facade (AGENTS.md §11). Network-event dispatch is forwarded to the
//! orchestrator directly; user-initiated actions route through the
//! corresponding UseCases (D22 thin wrapper). External consumers see
//! only `PairingFacade` — no `Arc<PairingOrchestrator>` leaks out of the
//! crate.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

use uc_core::{
    network::{
        protocol::{
            PairingChallenge, PairingChallengeResponse, PairingConfirm, PairingKeyslotOffer,
            PairingRequest, PairingResponse,
        },
        SessionId,
    },
    pairing::PairingRole,
};

use super::crypto::PairingCryptoPorts;
use super::events::{PairingDomainEvent, PairingEventPort};
use super::orchestrator::{PairingConfig, PairingOrchestrator};
use super::protocol_handler::SharedTrustPeerOrchestrator;
use super::session_manager::PairingPeerInfo;
use super::state_machine::PairingAction;
use super::usecases::{AcceptPairingUseCase, CancelPairingUseCase, RejectPairingUseCase};

/// Stable application-layer entry point for the pairing module.
///
/// Holds the orchestrator and user-intent UseCases as private
/// composition. Exposes a single public surface that covers everything
/// external consumers (daemon, setup) need:
///
/// * User-triggered actions: `accept_pairing` / `reject_pairing` /
///   `cancel_pairing` / `verify_pairing` — route through UseCases.
/// * System-triggered protocol dispatch: `initiate_pairing` /
///   `handle_*` — delegate to the orchestrator directly.
/// * Session queries: `get_session_peer` / `get_session_role` /
///   `has_active_session` / `cleanup_expired_sessions`.
/// * Event subscription: implements `PairingEventPort`.
pub struct PairingFacade {
    orchestrator: Arc<PairingOrchestrator>,
    accept: AcceptPairingUseCase,
    reject: RejectPairingUseCase,
    cancel: CancelPairingUseCase,
}

impl PairingFacade {
    /// Construct the facade and return the paired `PairingAction`
    /// receiver that the daemon's action loop consumes.
    pub fn new(
        config: PairingConfig,
        trust_peer_orch: SharedTrustPeerOrchestrator,
        local_device_name: String,
        local_device_id: String,
        local_peer_id: String,
        local_identity_pubkey: Vec<u8>,
        crypto: Arc<PairingCryptoPorts>,
    ) -> (Self, mpsc::Receiver<PairingAction>) {
        let (orchestrator, action_rx) = PairingOrchestrator::new(
            config,
            trust_peer_orch,
            local_device_name,
            local_device_id,
            local_peer_id,
            local_identity_pubkey,
            crypto,
        );
        let orchestrator = Arc::new(orchestrator);
        let accept = AcceptPairingUseCase::new(Arc::clone(&orchestrator));
        let reject = RejectPairingUseCase::new(Arc::clone(&orchestrator));
        let cancel = CancelPairingUseCase::new(Arc::clone(&orchestrator));

        (
            Self {
                orchestrator,
                accept,
                reject,
                cancel,
            },
            action_rx,
        )
    }

    // ── User-intent actions (routed through UseCases) ────────────────

    /// User confirmed the short-code matches the peer's display.
    pub async fn accept_pairing(&self, session_id: &str) -> Result<()> {
        self.accept.execute(session_id).await
    }

    /// User rejected the short-code.
    pub async fn reject_pairing(&self, session_id: &str) -> Result<()> {
        self.reject.execute(session_id).await
    }

    /// User cancelled the pairing flow.
    pub async fn cancel_pairing(&self, session_id: &str) -> Result<()> {
        self.cancel.execute(session_id).await
    }

    /// UI convenience: accept when `pin_matches`, reject otherwise.
    pub async fn verify_pairing(&self, session_id: &str, pin_matches: bool) -> Result<()> {
        if pin_matches {
            self.accept_pairing(session_id).await
        } else {
            self.reject_pairing(session_id).await
        }
    }

    // ── System / network dispatch (delegated to orchestrator) ────────

    pub async fn initiate_pairing(&self, peer_id: String) -> Result<SessionId> {
        self.orchestrator.initiate_pairing(peer_id).await
    }

    pub async fn handle_incoming_request(
        &self,
        peer_id: String,
        request: PairingRequest,
    ) -> Result<()> {
        self.orchestrator
            .handle_incoming_request(peer_id, request)
            .await
    }

    pub async fn handle_challenge(
        &self,
        session_id: &str,
        peer_id: &str,
        challenge: PairingChallenge,
    ) -> Result<()> {
        self.orchestrator
            .handle_challenge(session_id, peer_id, challenge)
            .await
    }

    pub async fn handle_keyslot_offer(
        &self,
        session_id: &str,
        peer_id: &str,
        offer: PairingKeyslotOffer,
    ) -> Result<()> {
        self.orchestrator
            .handle_keyslot_offer(session_id, peer_id, offer)
            .await
    }

    pub async fn handle_challenge_response(
        &self,
        session_id: &str,
        peer_id: &str,
        response: PairingChallengeResponse,
    ) -> Result<()> {
        self.orchestrator
            .handle_challenge_response(session_id, peer_id, response)
            .await
    }

    pub async fn handle_response(
        &self,
        session_id: &str,
        peer_id: &str,
        response: PairingResponse,
    ) -> Result<()> {
        self.orchestrator
            .handle_response(session_id, peer_id, response)
            .await
    }

    pub async fn handle_confirm(
        &self,
        session_id: &str,
        peer_id: &str,
        confirm: PairingConfirm,
    ) -> Result<()> {
        self.orchestrator
            .handle_confirm(session_id, peer_id, confirm)
            .await
    }

    pub async fn handle_reject(&self, session_id: &str, peer_id: &str) -> Result<()> {
        self.orchestrator.handle_reject(session_id, peer_id).await
    }

    pub async fn handle_cancel(&self, session_id: &str, peer_id: &str) -> Result<()> {
        self.orchestrator.handle_cancel(session_id, peer_id).await
    }

    pub async fn handle_busy(
        &self,
        session_id: &str,
        peer_id: &str,
        reason: Option<String>,
    ) -> Result<()> {
        self.orchestrator
            .handle_busy(session_id, peer_id, reason)
            .await
    }

    pub async fn handle_transport_error(
        &self,
        session_id: &str,
        peer_id: &str,
        error: String,
    ) -> Result<()> {
        self.orchestrator
            .handle_transport_error(session_id, peer_id, error)
            .await
    }

    // ── Session queries ─────────────────────────────────────────────

    pub async fn get_session_peer(&self, session_id: &str) -> Option<PairingPeerInfo> {
        self.orchestrator.get_session_peer(session_id).await
    }

    pub async fn get_session_role(&self, session_id: &str) -> Option<PairingRole> {
        self.orchestrator.get_session_role(session_id).await
    }

    pub async fn has_active_session(&self, session_id: &str) -> bool {
        self.orchestrator.has_active_session(session_id).await
    }

    pub async fn cleanup_expired_sessions(&self) {
        self.orchestrator.cleanup_expired_sessions().await
    }
}

#[async_trait]
impl PairingEventPort for PairingFacade {
    async fn subscribe(&self) -> Result<mpsc::Receiver<PairingDomainEvent>> {
        PairingEventPort::subscribe(self.orchestrator.as_ref()).await
    }
}
