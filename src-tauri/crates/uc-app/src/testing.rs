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

#[inline]
fn noop_ok_unit<E>() -> Result<(), E> {
    Ok(())
}

#[inline]
fn noop_ok_none<T, E>() -> Result<Option<T>, E> {
    Ok(None)
}

#[inline]
fn noop_ok_empty_vec<T, E>() -> Result<Vec<T>, E> {
    Ok(Vec::new())
}

macro_rules! define_noop_async_port {
    (
        $name:ident : $trait:path {
            $(fn $method:ident (&self $(, $arg:ident : $arg_ty:ty )* $(,)? ) -> $ret:ty => $body:block;)+
        }
    ) => {
        pub struct $name;

        #[async_trait]
        impl $trait for $name {
            $(
                async fn $method(&self $(, $arg: $arg_ty )* ) -> $ret $body
            )+
        }
    };
}

macro_rules! define_noop_async_mut_port {
    (
        $name:ident : $trait:path {
            $(fn $method:ident (&mut self $(, $arg:ident : $arg_ty:ty )* $(,)? ) -> $ret:ty => $body:block;)+
        }
    ) => {
        pub struct $name;

        #[async_trait]
        impl $trait for $name {
            $(
                async fn $method(&mut self $(, $arg: $arg_ty )* ) -> $ret $body
            )+
        }
    };
}

define_noop_async_port! {
    NoopPairedDeviceRepository: PairedDeviceRepositoryPort {
        fn get_by_peer_id(&self, _peer_id: &PeerId) -> Result<Option<PairedDevice>, PairedDeviceRepositoryError> => {
            noop_ok_none()
        };
        fn list_all(&self) -> Result<Vec<PairedDevice>, PairedDeviceRepositoryError> => {
            noop_ok_empty_vec()
        };
        fn upsert(&self, _device: PairedDevice) -> Result<(), PairedDeviceRepositoryError> => {
            noop_ok_unit()
        };
        fn set_state(&self, _peer_id: &PeerId, _state: PairingState) -> Result<(), PairedDeviceRepositoryError> => {
            noop_ok_unit()
        };
        fn update_last_seen(
            &self,
            _peer_id: &PeerId,
            _last_seen_at: chrono::DateTime<chrono::Utc>,
        ) -> Result<(), PairedDeviceRepositoryError> => {
            noop_ok_unit()
        };
        fn delete(&self, _peer_id: &PeerId) -> Result<(), PairedDeviceRepositoryError> => {
            noop_ok_unit()
        };
        fn update_sync_settings(
            &self,
            _peer_id: &PeerId,
            _settings: Option<uc_core::settings::model::SyncSettings>,
        ) -> Result<(), PairedDeviceRepositoryError> => {
            noop_ok_unit()
        };
    }
}

define_noop_async_port! {
    NoopDiscoveryPort: DiscoveryPort {
        fn list_discovered_peers(&self) -> anyhow::Result<Vec<DiscoveredPeer>> => {
            noop_ok_empty_vec()
        };
    }
}

define_noop_async_port! {
    NoopSetupEventPort: SetupEventPort {
        fn emit_setup_state_changed(&self, _state: SetupState, _session_id: Option<String>) -> () => {
            ()
        };
    }
}

define_noop_async_port! {
    NoopNetworkControl: NetworkControlPort {
        fn start_network(&self) -> anyhow::Result<()> => {
            noop_ok_unit()
        };
    }
}

define_noop_async_port! {
    NoopSessionReadyEmitter: SessionReadyEmitter {
        fn emit_ready(&self) -> anyhow::Result<()> => {
            noop_ok_unit()
        };
    }
}

define_noop_async_port! {
    NoopLifecycleStatus: LifecycleStatusPort {
        fn set_state(&self, _state: LifecycleState) -> anyhow::Result<()> => {
            noop_ok_unit()
        };
        fn get_state(&self) -> LifecycleState => {
            LifecycleState::Idle
        };
    }
}

define_noop_async_port! {
    NoopLifecycleEventEmitter: LifecycleEventEmitter {
        fn emit_lifecycle_event(&self, _event: LifecycleEvent) -> anyhow::Result<()> => {
            noop_ok_unit()
        };
    }
}

define_noop_async_port! {
    NoopPairingTransport: PairingTransportPort {
        fn open_pairing_session(&self, _peer_id: String, _session_id: String) -> anyhow::Result<()> => {
            noop_ok_unit()
        };
        fn send_pairing_on_session(&self, _message: uc_core::network::PairingMessage) -> anyhow::Result<()> => {
            noop_ok_unit()
        };
        fn close_pairing_session(&self, _session_id: String, _reason: Option<String>) -> anyhow::Result<()> => {
            noop_ok_unit()
        };
        fn unpair_device(&self, _peer_id: String) -> anyhow::Result<()> => {
            noop_ok_unit()
        };
    }
}

define_noop_async_mut_port! {
    NoopSpaceAccessTransport: SpaceAccessTransportPort {
        fn send_offer(&mut self, _session_id: &uc_core::network::SessionId) -> anyhow::Result<()> => {
            noop_ok_unit()
        };
        fn send_proof(&mut self, _session_id: &uc_core::network::SessionId) -> anyhow::Result<()> => {
            noop_ok_unit()
        };
        fn send_result(&mut self, _session_id: &uc_core::network::SessionId) -> anyhow::Result<()> => {
            noop_ok_unit()
        };
    }
}

define_noop_async_port! {
    NoopProofPort: ProofPort {
        fn build_proof(
            &self,
            pairing_session_id: &uc_core::SessionId,
            space_id: &uc_core::ids::SpaceId,
            challenge_nonce: [u8; 32],
            _master_key: &MasterKey,
        ) -> anyhow::Result<SpaceAccessProofArtifact> => {
            Ok(SpaceAccessProofArtifact {
                pairing_session_id: pairing_session_id.clone(),
                space_id: space_id.clone(),
                challenge_nonce,
                proof_bytes: vec![],
            })
        };
        fn verify_proof(&self, _proof: &SpaceAccessProofArtifact, _expected_nonce: [u8; 32]) -> anyhow::Result<bool> => {
            Ok(true)
        };
    }
}

define_noop_async_mut_port! {
    NoopTimerPort: TimerPort {
        fn start(&mut self, _session_id: &uc_core::SessionId, _ttl_secs: u64) -> anyhow::Result<()> => {
            noop_ok_unit()
        };
        fn stop(&mut self, _session_id: &uc_core::SessionId) -> anyhow::Result<()> => {
            noop_ok_unit()
        };
    }
}

define_noop_async_mut_port! {
    NoopSpaceAccessPersistence: PersistencePort {
        fn persist_joiner_access(&mut self, _space_id: &uc_core::ids::SpaceId, _peer_id: &str) -> anyhow::Result<()> => {
            noop_ok_unit()
        };
        fn persist_sponsor_access(&mut self, _space_id: &uc_core::ids::SpaceId, _peer_id: &str) -> anyhow::Result<()> => {
            noop_ok_unit()
        };
    }
}
