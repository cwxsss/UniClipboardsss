//! Application facade for the space-access flow.
//!
//! Per `uc-application/AGENTS.md` §11.4, external consumers (bootstrap,
//! daemon, setup) must go through a Facade — `SpaceAccessOrchestrator`
//! itself stays `pub(crate)`. This facade is a thin delegation layer; all
//! real flow logic lives in the orchestrator.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{mpsc, Mutex};

use uc_core::ids::{SessionId, SpaceId};
use uc_core::space_access::event::SpaceAccessEvent;
use uc_core::space_access::state::SpaceAccessState;

use uc_core::crypto::SecretString;

use super::context::{SpaceAccessContext, SpaceAccessJoinerOffer, SpaceAccessOffer};
use super::events::{SpaceAccessCompletedEvent, SpaceAccessEventPort};
use super::executor::SpaceAccessExecutor;
use super::orchestrator::{AdmitMemberUseCaseDyn, SpaceAccessError, SpaceAccessOrchestrator};

/// External-facing entry point for the space-access flow.
///
/// Wraps a `SpaceAccessOrchestrator` and exposes the minimum surface the
/// daemon / setup / bootstrap layers actually need. The orchestrator itself
/// is `pub(crate)` and never leaks through this facade — call sites that
/// previously reached into `orchestrator.context()` to mutate `sponsor_peer_id`
/// etc. now go through the explicit setter methods below.
#[derive(Clone)]
pub struct SpaceAccessFacade {
    orchestrator: Arc<SpaceAccessOrchestrator>,
}

impl SpaceAccessFacade {
    /// Builds a facade with no `AdmitMemberUseCase` injected. Reaching
    /// `Granted` on the joiner side will therefore skip local member
    /// registration (only the trust relationship is persisted). Real
    /// runtimes should call [`SpaceAccessFacade::with_admit_member`].
    pub fn new() -> Self {
        Self {
            orchestrator: Arc::new(SpaceAccessOrchestrator::new()),
        }
    }

    /// Builds a facade with an `AdmitMemberUseCase` injected so that the
    /// joiner-side `Granted` transition also registers the sponsor peer as
    /// a local space member. See `SpaceAccessOrchestrator::with_admit_member`.
    pub fn with_admit_member(admit_member: Arc<AdmitMemberUseCaseDyn>) -> Self {
        Self {
            orchestrator: Arc::new(SpaceAccessOrchestrator::new().with_admit_member(admit_member)),
        }
    }

    /// Current flow state. Callers rely on this for UI projections and
    /// cross-layer event broadcasts.
    pub async fn get_state(&self) -> SpaceAccessState {
        self.orchestrator.get_state().await
    }

    /// Drop all in-flight context and move back to `Idle`. Used by setup
    /// cancellation / tear-down paths.
    pub async fn reset(&self) {
        self.orchestrator.reset().await;
    }

    /// Records the remote peer id that this flow is talking to. This used
    /// to be `orchestrator.context().lock().await.sponsor_peer_id = …`;
    /// the setter preserves the exact semantics without exposing the
    /// internal context handle.
    pub async fn set_sponsor_peer_id(&self, peer_id: Option<String>) {
        self.orchestrator.context().lock().await.sponsor_peer_id = peer_id;
    }

    /// Atomically seeds the context fields a joiner-side flow needs before
    /// dispatching `JoinRequested` — replaces the `context().lock()` triple
    /// write at the start of `submit_passphrase`.
    pub async fn initiate_joiner_flow(
        &self,
        joiner_offer: SpaceAccessJoinerOffer,
        passphrase: SecretString,
        sponsor_peer_id: Option<String>,
    ) {
        let ctx = self.orchestrator.context();
        let mut guard = ctx.lock().await;
        guard.joiner_offer = Some(joiner_offer);
        guard.joiner_passphrase = Some(passphrase);
        guard.sponsor_peer_id = sponsor_peer_id;
    }

    /// Reads the latest `joiner_offer` cached in the context. Returns `None`
    /// when the joiner side has not yet received a sponsor's offer.
    pub async fn peek_joiner_offer(&self) -> Option<SpaceAccessJoinerOffer> {
        self.orchestrator
            .context()
            .lock()
            .await
            .joiner_offer
            .clone()
    }

    /// Reads the sponsor-prepared offer (keyslot + nonce) that the sponsor
    /// side caches before sending it to a joiner.
    pub async fn peek_prepared_offer(&self) -> Option<SpaceAccessOffer> {
        self.orchestrator
            .context()
            .lock()
            .await
            .prepared_offer
            .clone()
    }

    /// Records the remote peer's display name and identity fingerprint,
    /// both of which feed `AdmitMember` when the flow reaches `Granted`.
    /// Either argument may be `None` if the source (pairing `PeerInfo` or
    /// `trusted_peer` repository) failed to provide it — the admit step
    /// will simply WARN and skip.
    pub async fn set_peer_identity(
        &self,
        device_name: Option<String>,
        fingerprint: Option<String>,
    ) {
        let ctx = self.orchestrator.context();
        let mut guard = ctx.lock().await;
        guard.peer_device_name = device_name;
        guard.peer_fingerprint = fingerprint;
    }

    /// Starts a sponsor-side authorization. Delegates to the orchestrator;
    /// executor is passed by `&mut` because its port references cannot be
    /// held across a non-borrowed boundary.
    pub async fn start_sponsor_authorization(
        &self,
        executor: &mut SpaceAccessExecutor<'_>,
        pairing_session_id: SessionId,
        space_id: SpaceId,
        ttl_secs: u64,
    ) -> Result<SpaceAccessState, SpaceAccessError> {
        self.orchestrator
            .start_sponsor_authorization(executor, pairing_session_id, space_id, ttl_secs)
            .await
    }

    /// Drives the state machine with a single event. Mirror of
    /// `SpaceAccessOrchestrator::dispatch`.
    pub async fn dispatch(
        &self,
        executor: &mut SpaceAccessExecutor<'_>,
        event: SpaceAccessEvent,
        pairing_session_id: Option<SessionId>,
    ) -> Result<SpaceAccessState, SpaceAccessError> {
        self.orchestrator
            .dispatch(executor, event, pairing_session_id)
            .await
    }

    /// Returns the shared `SpaceAccessContext` handle for bootstrap to wire
    /// the internal adapters (`SpaceAccessNetworkAdapter`). External
    /// consumers **must not** use this to mutate context fields directly —
    /// call the explicit setters above. This escape hatch exists only so
    /// that bootstrap can keep today's adapter-construction shape without
    /// pulling the adapter build itself into the facade.
    pub fn context_handle(&self) -> Arc<Mutex<SpaceAccessContext>> {
        self.orchestrator.context()
    }
}

impl Default for SpaceAccessFacade {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SpaceAccessEventPort for SpaceAccessFacade {
    async fn subscribe(&self) -> anyhow::Result<mpsc::Receiver<SpaceAccessCompletedEvent>> {
        SpaceAccessEventPort::subscribe(self.orchestrator.as_ref()).await
    }
}
