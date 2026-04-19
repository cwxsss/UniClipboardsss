use crate::ids::{SessionId, SpaceId};

#[derive(Clone, Debug)]
pub struct SpaceAccessProofArtifact {
    pub pairing_session_id: SessionId,
    pub space_id: SpaceId,
    pub challenge_nonce: [u8; 32],
    pub proof_bytes: Vec<u8>,
}

/// Sponsor 发给 joiner 的 pairing offer——空间接入流程的载体。
///
/// `keyslot_blob` 是 adapter 自序列化的不透明字节（承载 KEK wrap 后的 MasterKey
/// 等加密物料），领域层不关心其布局；`challenge_nonce` 是 sponsor 产生的 32
/// 字节挑战值，joiner 用它 + 自身派生的 MasterKey 构造 proof 回传，sponsor
/// 验证 proof 以确认 joiner 拿到正确口令。
#[derive(Clone, Debug)]
pub struct JoinOffer {
    pub space_id: SpaceId,
    pub keyslot_blob: Vec<u8>,
    pub challenge_nonce: [u8; 32],
}
