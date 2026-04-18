/// Runtime trust status of a remote peer, used by network adapters to decide
/// which libp2p protocols are negotiable for an inbound / outbound stream.
///
/// 与配对流程中的 `PairingStateMachine::PairingState` 是两回事：
/// - `PeerTrustStatus` 只关心"此刻该对端是否为已接纳成员"，由 `MemberRepositoryPort`
///   的命中/未命中结果合成。
/// - `PairingStateMachine::PairingState` 描述的是某次配对流程会话自身的状态
///   （Idle / RequestSent / Finalizing …），生命周期只存在于会话内存中。
///
/// Phase 4b PR-5 起 `uc-core::pairing::PairingState` 彻底下线；运行时
/// 连接策略改由本类型（两态：`Trusted` / `Untrusted`）单独承载。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum PeerTrustStatus {
    /// 对端已登记为本 space 的成员 → 允许 pairing + business 两类协议。
    Trusted,
    /// 对端尚未登记或已被撤销 → 仅允许 pairing 协议（用于再次建立信任）。
    Untrusted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProtocolKind {
    Pairing,
    Business,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllowedProtocols {
    pairing: bool,
    business: bool,
}

impl AllowedProtocols {
    pub fn allows(&self, kind: ProtocolKind) -> bool {
        match kind {
            ProtocolKind::Pairing => self.pairing,
            ProtocolKind::Business => self.business,
        }
    }
}

pub struct ConnectionPolicy;

impl ConnectionPolicy {
    pub fn allowed_protocols(status: PeerTrustStatus) -> AllowedProtocols {
        match status {
            PeerTrustStatus::Trusted => AllowedProtocols {
                pairing: true,
                business: true,
            },
            PeerTrustStatus::Untrusted => AllowedProtocols {
                pairing: true,
                business: false,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedConnectionPolicy {
    pub trust: PeerTrustStatus,
    pub allowed: AllowedProtocols,
}
