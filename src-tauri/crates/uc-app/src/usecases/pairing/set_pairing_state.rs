use anyhow::Result;
use std::sync::Arc;
use uc_core::network::PairingState;
use uc_core::ports::PairedDeviceRepositoryPort;
use uc_core::PeerId;

pub struct SetPairingState {
    repo: Arc<dyn PairedDeviceRepositoryPort>,
}

impl SetPairingState {
    pub fn new(repo: Arc<dyn PairedDeviceRepositoryPort>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, peer_id: PeerId, state: PairingState) -> Result<()> {
        self.repo
            .set_state(&peer_id, state)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to set pairing state: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_mocks::MockPairedDeviceRepository;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use uc_core::network::PairingState;
    use uc_core::PeerId;

    #[tokio::test]
    async fn test_set_pairing_state_updates_repo() {
        let captured: Arc<Mutex<HashMap<String, PairingState>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let captured_clone = captured.clone();

        let mut repo = MockPairedDeviceRepository::new();
        repo.expect_set_state().returning(move |peer_id, state| {
            let captured = captured_clone.clone();
            let peer_id_str = peer_id.as_str().to_string();
            futures::executor::block_on(async move {
                captured.lock().await.insert(peer_id_str, state);
            });
            Ok(())
        });

        let uc = SetPairingState::new(Arc::new(repo));
        uc.execute(PeerId::from("peer"), PairingState::Trusted)
            .await
            .unwrap();

        let guard = captured.lock().await;
        assert_eq!(guard.get("peer").cloned(), Some(PairingState::Trusted));
    }
}
