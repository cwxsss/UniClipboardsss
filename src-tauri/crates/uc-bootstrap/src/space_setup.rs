//! Slice 1 composition root for [`SpaceSetupFacade`].
//!
//! Assembles the pairing stack (rendezvous client + iroh session adapter +
//! identity store + proof verifier) plus the pre-existing persistence /
//! identity ports from [`WiredDependencies`] into a single
//! [`SpaceSetupAssembly`] that external callers (Tauri commands, CLI, daemon)
//! can drive through `Arc<SpaceSetupFacade>`.
//!
//! Everything iroh-specific stays inside
//! [`uc_infra::network::iroh::IrohNode`] so this module depends only on
//! core ports + the `IrohNode` handle. When Slice 2 adds a clipboard-sync
//! transport, the extension point is `IrohNode::install_clipboard` rather
//! than a second stack.
//!
//! Shutdown is a two-step coordinated teardown: first drive the facade's
//! own shutdown (aborts the sponsor-side inbound orchestrator task + best-
//! effort `stop_network`), then shut the iroh router down so live
//! connections see a clean `CONNECTION_CLOSE` rather than waiting for peer
//! timeouts.

use std::sync::Arc;

use tracing::{info, instrument};

use uc_application::facade::{
    MemberRosterDeps, MemberRosterFacade, SpaceSetupDeps, SpaceSetupFacade,
};
use uc_application::space_access::HmacProofAdapter;
use uc_core::ports::space::ProofPort;
use uc_core::ports::{LocalIdentityPort, PresencePort};
use uc_infra::network::iroh::{IrohIdentityStore, IrohNode, IrohNodeBuilder, IrohNodeError};
// Re-exported so external callers can parametrise the assembly without
// having to `use uc_infra` themselves.
pub use uc_infra::network::iroh::IrohNodeConfig;
use uc_infra::security::Sha256IdentityFingerprintFactory;

use crate::assembly::WiredDependencies;

/// Output of [`build_space_setup_assembly`]. External callers keep the
/// whole assembly alive for the process lifetime; they only dispatch
/// user-facing commands through [`Self::facade`] / [`Self::roster`] and
/// run [`Self::shutdown`] once on exit.
pub struct SpaceSetupAssembly {
    pub facade: Arc<SpaceSetupFacade>,
    /// Slice 2 Phase 1 · T9:roster 查询门面(`list_with_presence` +
    /// `subscribe_presence_events`)。CLI `members` 命令从这里拿状态,
    /// tauri `get_roster` 将来也走同一条。共享同一个 `peer_addr_repo` /
    /// `presence` 实例,所以 F1 hook 填好的缓存这里能直接读到。
    pub roster: Arc<MemberRosterFacade>,
    /// The shared iroh node. Held privately so callers can't bind a second
    /// node or install additional handlers after `spawn` — that would
    /// fragment peer identity (§"共用网络栈" decision, Slice 1 planning).
    iroh_node: IrohNode,
}

impl SpaceSetupAssembly {
    /// Coordinated teardown. Order matters:
    ///
    /// 1. [`SpaceSetupFacade::on_shutdown`] aborts the sponsor-side inbound
    ///    orchestrator task so the `pairing_events` receiver is dropped
    ///    before the adapter is torn down.
    /// 2. [`IrohNode::shutdown`] shuts the iroh router, which fires
    ///    `ProtocolHandler::shutdown` on the pairing handler and emits
    ///    `CONNECTION_CLOSE` to any live peer.
    #[instrument(skip_all)]
    pub async fn shutdown(self) {
        self.facade.on_shutdown().await;
        self.iroh_node.shutdown().await;
    }
}

/// Failures during Slice 1 assembly. Bootstrap callers surface these as
/// fatal startup errors — there is no useful retry here.
#[derive(Debug, thiserror::Error)]
pub enum SpaceSetupAssemblyError {
    #[error(transparent)]
    IrohNode(#[from] IrohNodeError),
}

/// Assemble the Slice 1 `SpaceSetupFacade` from an already-wired dependency
/// graph. Blocking responsibility: binds an iroh `Endpoint` and spawns its
/// router, so must be called inside a tokio runtime.
///
/// The resulting facade owns the entire Slice 1 lifecycle surface (A1 / A2
/// / B1 / B2 / F2). Call sites that also want to drive legacy setup should
/// keep holding their pre-existing `SetupFacade` alongside; the two
/// coexist until Slice 5 retires libp2p.
#[instrument(skip_all)]
pub async fn build_space_setup_assembly(
    wired: &WiredDependencies,
    iroh_config: IrohNodeConfig,
) -> Result<SpaceSetupAssembly, SpaceSetupAssemblyError> {
    let deps = &wired.deps;

    // IdentityFingerprintFactory is stateless — the one in SecurityPorts is
    // the same `Sha256IdentityFingerprintFactory` ZST, but we construct a
    // fresh one here rather than down-casting through `dyn` because
    // `IrohIdentityStore::new` takes the concrete factory trait object and
    // we'd have to re-wrap anyway.
    let identity_store = Arc::new(IrohIdentityStore::new(
        Arc::clone(&deps.security.secure_storage),
        Arc::new(Sha256IdentityFingerprintFactory),
    ));

    // Bind the shared iroh node + install the pairing transport. The
    // returned PairingHandlers carry the trait objects SpaceSetupDeps
    // wants; the iroh Router stays inside `IrohNode` so iroh types don't
    // leak out of this module.
    let mut builder = IrohNodeBuilder::bind(&identity_store, iroh_config).await?;
    let handlers = builder.install_pairing(
        Arc::clone(&deps.device.device_identity),
        Arc::clone(&deps.settings),
    );
    // Slice 2 Phase 1 · T8:在同一 iroh 节点上装 presence handler。must
    // be before `builder.spawn()`(install_* 要求 router 未 spawn)。
    // `Arc<dyn PresencePort>` 喂给 SpaceSetupDeps,facade 内部再构造
    // `EnsureReachableAllUseCase` 给 F1 hook 用。
    let presence: Arc<dyn PresencePort> = builder.install_presence(
        Arc::clone(&wired.peer_addr_repo),
        Arc::clone(&deps.system.clock),
    );
    let iroh_node = builder.spawn();

    // HMAC proof adapter verifies the joiner's ChallengeResponse against
    // the master key that `SpaceAccessPort::derive_master_key_for_proof`
    // stashes in-session. Fed the same `space_access` the use cases use
    // so the cache-miss fallback can still find the current session key.
    let proof_port: Arc<dyn ProofPort> = Arc::new(HmacProofAdapter::new_with_space_access(
        Arc::clone(&deps.security.space_access),
    ));

    let local_identity: Arc<dyn LocalIdentityPort> = identity_store;

    let facade = Arc::new(SpaceSetupFacade::new(SpaceSetupDeps {
        space_access: Arc::clone(&deps.security.space_access),
        local_identity: Arc::clone(&local_identity),
        device_identity: Arc::clone(&deps.device.device_identity),
        member_repo: Arc::clone(&deps.device.member_repo),
        setup_status: Arc::clone(&deps.setup_status),
        settings: Arc::clone(&deps.settings),
        clock: Arc::clone(&deps.system.clock),
        network_control: Arc::clone(&deps.network_control),
        pairing_invitation: handlers.invitation,
        pairing_session: handlers.session,
        pairing_events: handlers.events,
        proof_port,
        trusted_peer_repo: Arc::clone(&wired.trusted_peer_repo),
        peer_addr_repo: Arc::clone(&wired.peer_addr_repo),
        presence: Arc::clone(&presence),
    }));

    // Slice 2 Phase 1 · T9:roster 门面和 space_setup facade 共享同一组
    // 实例(`member_repo` / `local_identity` / `presence`),这样 F1 hook
    // 通过 `presence.ensure_reachable_all` 填好的缓存,`list_with_presence`
    // 能直接读到。Facade 本身是纯 thin wrapper,构造非常便宜。
    let roster = Arc::new(MemberRosterFacade::new(MemberRosterDeps {
        member_repo: Arc::clone(&deps.device.member_repo),
        local_identity,
        presence,
    }));

    info!("Slice 1 SpaceSetupFacade + MemberRosterFacade assembled");
    Ok(SpaceSetupAssembly {
        facade,
        roster,
        iroh_node,
    })
}
