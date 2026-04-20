//! Application-layer errors for the Slice 1 facade.

use thiserror::Error;

/// Failure modes of A1 `InitializeSpaceUseCase`.
///
/// Kept narrower than the ports' native error types so callers can branch
/// on **what action to take next** (ask user again / surface a support
/// message / crash-logs) without having to understand cryptographic
/// details.
#[derive(Debug, Error)]
pub enum InitializeSpaceError {
    /// `passphrase` and `passphrase_confirm` differed. UI should keep the
    /// user on the current form.
    #[error("passphrase and confirmation do not match")]
    PassphraseMismatch,

    /// No device name available — neither in the command nor in
    /// `Settings.general.device_name`.
    #[error("device name is required but not provided")]
    DeviceNameRequired,

    /// The local space has already been initialised. User should unlock
    /// (A2) instead, or run a factory reset first.
    #[error("space is already initialised")]
    AlreadyInitialized,

    /// A local identity already exists (previous A1/B2 run left state).
    /// Current policy is loud failure so data inconsistencies are caught;
    /// the joiner path uses `ensure()` where retry is expected.
    #[error("local identity already exists")]
    IdentityAlreadyExists,

    /// Failed to read or persist settings / membership / setup-status —
    /// message carries adapter-level context for logs.
    #[error("storage failure: {0}")]
    StorageFailed(String),

    /// Any other uncategorised failure (adapter internal / infra-layer
    /// bug). Treat as fatal for the current action.
    #[error("internal error: {0}")]
    Internal(String),
}

/// Failure modes of B1 `IssuePairingInvitationUseCase`.
///
/// Mirrors
/// [`uc_core::ports::pairing_invitation::InvitationError`] at the
/// application boundary, keeping the upstream-port variant names so UI
/// can branch on intent ("start network" vs. "retry later") without
/// having to import the infra-port enum.
#[derive(Debug, Error)]
pub enum IssuePairingInvitationError {
    /// Underlying network runtime has not been started. UI should surface
    /// "start network first" (A1/A2 completing auto-starts it, so this
    /// typically means startup failed earlier and the user needs to retry).
    #[error("network is not started")]
    NetworkNotStarted,

    /// Rendezvous service unreachable / transient failure. UI may offer a
    /// manual retry.
    #[error("pairing invitation service unavailable")]
    ServiceUnavailable,

    /// Uncategorised adapter-side failure; message for logs only.
    #[error("internal error: {0}")]
    Internal(String),
}

/// Failure modes of B2 `RedeemPairingInvitationUseCase` (joiner side).
///
/// 应用层把三类来源的失败统一成"下一步动作"导向:
///
/// * 本机参数/状态问题（`DeviceNameRequired`）→ UI 让用户补齐再试。
/// * 网络/凭证问题（`InvitationNotFound/Expired` / `PassphraseMismatch` /
///   `SponsorUnreachable` 等）→ UI 展示具体原因，用户决定改口令 /
///   重新要邀请 / 等对方上线再试。
/// * sponsor 主动拒绝（`SponsorRejectedInvitation` / `SponsorDeclined` /
///   `SponsorTimedOut` / `SponsorInternal`）→ 不是本机错，信息告知并让
///   用户重新开始。
#[derive(Debug, Error)]
pub enum RedeemPairingInvitationError {
    /// Rendezvous 服务端没有这条邀请（typo / 从未 issue / 已经被消费）。
    #[error("invitation not found")]
    InvitationNotFound,

    /// 邀请已过 TTL — 让用户重新找 sponsor 要一份。
    #[error("invitation has expired")]
    InvitationExpired,

    /// Sponsor 在线广告过 address，但连接没打通（NAT / relay / 对方掉线）。
    #[error("sponsor is not reachable")]
    SponsorUnreachable,

    /// Rendezvous 服务不可达。
    #[error("pairing invitation service unavailable")]
    ServiceUnavailable,

    /// 口令错。覆盖两种来源:(a) 本机 `derive_master_key_for_proof` 解
    /// keyslot 失败；(b) sponsor 收到 proof 后 `verify_proof` 拒绝后发
    /// `Reject(PassphraseMismatch)`。两者语义相同 — UI 提示"再试一次
    /// 口令"。
    #[error("wrong passphrase")]
    PassphraseMismatch,

    /// Sponsor 发来的 keyslot 字节无法解析或版本不支持。属于数据/版本
    /// 故障，和 A2 `UnlockSpaceError::CorruptedKeyMaterial` 同义。
    #[error("space key material corrupted")]
    CorruptedKeyMaterial,

    /// 本机 `Settings.general.device_name` 为空且 command 里也没给 —
    /// UI 应该在进入 join flow 前先收集 device name（和 A1 一致）。
    #[error("device name is required but not provided")]
    DeviceNameRequired,

    /// sponsor 收到 `JoinerRequest` 后 code 未命中任何 pending 邀请，回
    /// `Reject(InvitationMismatch)`。多半 race：code 在 sponsor 这边已
    /// 过期或被别的 joiner 消费。
    #[error("sponsor did not recognise the invitation code")]
    SponsorRejectedInvitation,

    /// sponsor UI 明确拒绝本次配对（Slice 1 未暴露审批 UI，保留语义位）。
    #[error("sponsor declined the pairing request")]
    SponsorDeclined,

    /// sponsor 侧 TTL watchdog 先触发（P7g）— 对方还没看到本机的
    /// `ChallengeResponse`。UI 应提示"网络慢或 sponsor 没响应，重新试"。
    #[error("sponsor timed out the handshake")]
    SponsorTimedOut,

    /// sponsor 回 `Reject(Internal(..))` — 对方本地 persist / settings 出
    /// 问题。消息面向日志。
    #[error("sponsor internal error: {0}")]
    SponsorInternal(String),

    /// 本机等 sponsor 回消息时 TTL 耗尽（recv 超时）。
    #[error("pairing handshake timed out")]
    Timeout,

    /// 握手中途 transport 掉线（sponsor 关闭 stream / iroh connection
    /// 中断 / recv 收到 EOF）。
    #[error("connection lost mid-handshake")]
    ConnectionLost,

    /// 非预期消息、adapter 内部错、序列化等兜底。消息面向日志。
    #[error("internal error: {0}")]
    Internal(String),
}

/// Failure modes of A2 `UnlockSpaceUseCase`.
#[derive(Debug, Error)]
pub enum UnlockSpaceError {
    /// Setup has not been completed — there is no space to unlock yet.
    #[error("setup has not been completed")]
    SetupNotCompleted,

    /// Space exists only logically (setup marked complete) but the
    /// underlying keyslot is missing / corrupted.
    #[error("space is not initialised")]
    SpaceNotInitialized,

    /// Passphrase did not unwrap the stored master key.
    #[error("wrong passphrase")]
    WrongPassphrase,

    /// Stored keyslot was corrupted or in an unsupported format.
    #[error("space key material corrupted")]
    CorruptedKeyMaterial,

    /// Uncategorised infra / adapter failure.
    #[error("internal error: {0}")]
    Internal(String),
}
