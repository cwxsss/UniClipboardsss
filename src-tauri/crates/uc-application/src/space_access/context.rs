use uc_core::crypto::SecretString;
use uc_core::ids::SpaceId;
use uc_core::space_access::state::DenyReason;
use uc_core::space_access::{JoinOffer, SpaceAccessProofArtifact};

#[derive(Clone, Debug)]
pub struct SpaceAccessJoinerOffer {
    pub space_id: SpaceId,
    pub keyslot_blob: Vec<u8>,
    pub challenge_nonce: [u8; 32],
}

#[derive(Default)]
pub struct SpaceAccessContext {
    pub prepared_offer: Option<JoinOffer>,
    pub joiner_offer: Option<SpaceAccessJoinerOffer>,
    pub joiner_passphrase: Option<SecretString>,
    pub proof_artifact: Option<SpaceAccessProofArtifact>,
    pub sponsor_peer_id: Option<String>,
    /// 对端设备名，由 pairing 协议完成时（`PairingSucceeded`）由上层写入，
    /// 供 `Granted` 转移点构造 `AdmitMember` 输入。
    pub peer_device_name: Option<String>,
    /// 对端身份指纹，由上层在 pairing 完成后从 `trusted_peer` 仓库读出写入，
    /// 供 `Granted` 转移点构造 `AdmitMember` 输入。
    pub peer_fingerprint: Option<String>,
    pub result_success: Option<bool>,
    pub result_deny_reason: Option<DenyReason>,
}
