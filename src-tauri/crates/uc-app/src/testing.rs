//! Shared noop/mock implementations for test use.
//!
//! This module provides reusable noop implementations of port traits
//! that return `Ok(default)` values. Tests can import these instead of
//! defining their own identical noop structs.
//!
//! Only simple noops live here. Test-specific mocks that record calls
//! or return specific test data remain in their respective test modules.

use async_trait::async_trait;

use uc_core::network::{DiscoveredPeer, PairedDevice, PairingState};
use uc_core::ports::network_control::NetworkControlPort;
use uc_core::ports::space::{PersistencePort, ProofPort, SpaceAccessTransportPort};
use uc_core::ports::{
    DiscoveryPort, PairedDeviceRepositoryError, PairedDeviceRepositoryPort, PairingTransportPort,
    SetupEventPort, TimerPort,
};
use uc_core::security::model::MasterKey;
use uc_core::security::space_access::SpaceAccessProofArtifact;
use uc_core::setup::SetupState;
use uc_core::PeerId;

use crate::usecases::{
    LifecycleEvent, LifecycleEventEmitter, LifecycleState, LifecycleStatusPort, SessionReadyEmitter,
};

// ---------------------------------------------------------------------------
// PairedDeviceRepositoryPort
// ---------------------------------------------------------------------------

pub struct NoopPairedDeviceRepository;

#[async_trait]
impl PairedDeviceRepositoryPort for NoopPairedDeviceRepository {
    async fn get_by_peer_id(
        &self,
        _peer_id: &PeerId,
    ) -> Result<Option<PairedDevice>, PairedDeviceRepositoryError> {
        Ok(None)
    }

    async fn list_all(&self) -> Result<Vec<PairedDevice>, PairedDeviceRepositoryError> {
        Ok(Vec::new())
    }

    async fn upsert(&self, _device: PairedDevice) -> Result<(), PairedDeviceRepositoryError> {
        Ok(())
    }

    async fn set_state(
        &self,
        _peer_id: &PeerId,
        _state: PairingState,
    ) -> Result<(), PairedDeviceRepositoryError> {
        Ok(())
    }

    async fn update_last_seen(
        &self,
        _peer_id: &PeerId,
        _last_seen_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), PairedDeviceRepositoryError> {
        Ok(())
    }

    async fn delete(&self, _peer_id: &PeerId) -> Result<(), PairedDeviceRepositoryError> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// DiscoveryPort
// ---------------------------------------------------------------------------

pub struct NoopDiscoveryPort;

#[async_trait]
impl DiscoveryPort for NoopDiscoveryPort {
    async fn list_discovered_peers(&self) -> anyhow::Result<Vec<DiscoveredPeer>> {
        Ok(Vec::new())
    }
}

// ---------------------------------------------------------------------------
// SetupEventPort
// ---------------------------------------------------------------------------

pub struct NoopSetupEventPort;

#[async_trait]
impl SetupEventPort for NoopSetupEventPort {
    async fn emit_setup_state_changed(&self, _state: SetupState, _session_id: Option<String>) {}
}

// ---------------------------------------------------------------------------
// NetworkControlPort
// ---------------------------------------------------------------------------

pub struct NoopNetworkControl;

#[async_trait]
impl NetworkControlPort for NoopNetworkControl {
    async fn start_network(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// SessionReadyEmitter
// ---------------------------------------------------------------------------

pub struct NoopSessionReadyEmitter;

#[async_trait]
impl SessionReadyEmitter for NoopSessionReadyEmitter {
    async fn emit_ready(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// LifecycleStatusPort
// ---------------------------------------------------------------------------

pub struct NoopLifecycleStatus;

#[async_trait]
impl LifecycleStatusPort for NoopLifecycleStatus {
    async fn set_state(&self, _state: LifecycleState) -> anyhow::Result<()> {
        Ok(())
    }

    async fn get_state(&self) -> LifecycleState {
        LifecycleState::Idle
    }
}

// ---------------------------------------------------------------------------
// LifecycleEventEmitter
// ---------------------------------------------------------------------------

pub struct NoopLifecycleEventEmitter;

#[async_trait]
impl LifecycleEventEmitter for NoopLifecycleEventEmitter {
    async fn emit_lifecycle_event(&self, _event: LifecycleEvent) -> anyhow::Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// PairingTransportPort
// ---------------------------------------------------------------------------

pub struct NoopPairingTransport;

#[async_trait]
impl PairingTransportPort for NoopPairingTransport {
    async fn open_pairing_session(
        &self,
        _peer_id: String,
        _session_id: String,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn send_pairing_on_session(
        &self,
        _message: uc_core::network::PairingMessage,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn close_pairing_session(
        &self,
        _session_id: String,
        _reason: Option<String>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn unpair_device(&self, _peer_id: String) -> anyhow::Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// SpaceAccessTransportPort
// ---------------------------------------------------------------------------

pub struct NoopSpaceAccessTransport;

#[async_trait]
impl SpaceAccessTransportPort for NoopSpaceAccessTransport {
    async fn send_offer(
        &mut self,
        _session_id: &uc_core::network::SessionId,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn send_proof(
        &mut self,
        _session_id: &uc_core::network::SessionId,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn send_result(
        &mut self,
        _session_id: &uc_core::network::SessionId,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ProofPort
// ---------------------------------------------------------------------------

pub struct NoopProofPort;

#[async_trait]
impl ProofPort for NoopProofPort {
    async fn build_proof(
        &self,
        pairing_session_id: &uc_core::SessionId,
        space_id: &uc_core::ids::SpaceId,
        challenge_nonce: [u8; 32],
        _master_key: &MasterKey,
    ) -> anyhow::Result<SpaceAccessProofArtifact> {
        Ok(SpaceAccessProofArtifact {
            pairing_session_id: pairing_session_id.clone(),
            space_id: space_id.clone(),
            challenge_nonce,
            proof_bytes: vec![],
        })
    }

    async fn verify_proof(
        &self,
        _proof: &SpaceAccessProofArtifact,
        _expected_nonce: [u8; 32],
    ) -> anyhow::Result<bool> {
        Ok(true)
    }
}

// ---------------------------------------------------------------------------
// TimerPort
// ---------------------------------------------------------------------------

pub struct NoopTimerPort;

#[async_trait]
impl TimerPort for NoopTimerPort {
    async fn start(
        &mut self,
        _session_id: &uc_core::SessionId,
        _ttl_secs: u64,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn stop(&mut self, _session_id: &uc_core::SessionId) -> anyhow::Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// PersistencePort (space access)
// ---------------------------------------------------------------------------

pub struct NoopSpaceAccessPersistence;

#[async_trait]
impl PersistencePort for NoopSpaceAccessPersistence {
    async fn persist_joiner_access(
        &mut self,
        _space_id: &uc_core::ids::SpaceId,
        _peer_id: &str,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn persist_sponsor_access(
        &mut self,
        _space_id: &uc_core::ids::SpaceId,
        _peer_id: &str,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}
