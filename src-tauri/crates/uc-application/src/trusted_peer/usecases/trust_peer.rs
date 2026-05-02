use std::sync::Arc;

use chrono::{DateTime, Utc};
use uc_core::security::IdentityFingerprint;
use uc_core::{DeviceId, TrustedPeer, TrustedPeerRepositoryPort};

use crate::trusted_peer::errors::TrustedPeerApplicationError;

/// Input for establishing trust with a peer.
///
/// 典型触发点：用户在 `AwaitingUserVerification` 状态下确认对端身份后，
/// 状态机内部调用本 UseCase 把 `TrustedPeer` 落盘（DOMAIN §5.1）。
#[derive(Debug, Clone)]
pub struct TrustPeer {
    pub local_device_id: DeviceId,
    pub peer_device_id: DeviceId,
    pub peer_fingerprint: IdentityFingerprint,
    pub trusted_at: DateTime<Utc>,
}

/// 登记一个 `TrustedPeer`。
///
/// 幂等策略：同一 `peer_device_id` 已存在会返回 `AlreadyTrusted`，和
/// `AdmitMemberUseCase` 的冲突策略对称。fingerprint 轮换属于合法业务场景，
/// 但应当走 "先 Distrust 再 Trust" 的显式流程，而不是静默覆盖。
pub struct TrustPeerUseCase<R: ?Sized> {
    repository: Arc<R>,
}

impl<R> TrustPeerUseCase<R>
where
    R: TrustedPeerRepositoryPort + ?Sized,
{
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        input: TrustPeer,
    ) -> Result<TrustedPeer, TrustedPeerApplicationError> {
        if self.repository.get(&input.peer_device_id).await?.is_some() {
            return Err(TrustedPeerApplicationError::AlreadyTrusted(
                input.peer_device_id,
            ));
        }

        let peer = TrustedPeer {
            local_device_id: input.local_device_id,
            peer_device_id: input.peer_device_id,
            peer_fingerprint: input.peer_fingerprint,
            trusted_at: input.trusted_at,
        };

        self.repository.save(&peer).await?;
        Ok(peer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trusted_peer::testing::InMemoryTrustedPeerRepository;

    fn fp_for(seed: &str) -> IdentityFingerprint {
        let mut raw: String = seed.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        raw.make_ascii_uppercase();
        while raw.len() < 16 {
            raw.push('A');
        }
        IdentityFingerprint::from_raw_string(&raw[..16]).unwrap()
    }

    fn fixture(peer_id: &str) -> TrustPeer {
        TrustPeer {
            local_device_id: DeviceId::new("local-1"),
            peer_device_id: DeviceId::new(peer_id),
            peer_fingerprint: fp_for(&format!("FP{peer_id}")),
            trusted_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn first_trust_saves_and_returns_aggregate() {
        let repo = Arc::new(InMemoryTrustedPeerRepository::new());
        let uc = TrustPeerUseCase::new(repo.clone());

        let saved = uc.execute(fixture("peer-a")).await.unwrap();
        assert_eq!(saved.peer_device_id.as_str(), "peer-a");

        let loaded = repo.get(&saved.peer_device_id).await.unwrap().unwrap();
        assert_eq!(loaded, saved);
    }

    #[tokio::test]
    async fn second_trust_for_same_peer_returns_already_trusted() {
        let repo = Arc::new(InMemoryTrustedPeerRepository::new());
        let uc = TrustPeerUseCase::new(repo);
        uc.execute(fixture("peer-a")).await.unwrap();

        let err = uc.execute(fixture("peer-a")).await.unwrap_err();
        assert_eq!(
            err,
            TrustedPeerApplicationError::AlreadyTrusted(DeviceId::new("peer-a"))
        );
    }
}
