//! Placeholder network port implementation
//! 占位符网络端口实现

#![allow(deprecated)] // frozen libp2p path; legacy PairingTransportPort removed in Slice 5

use anyhow::Result;
use async_trait::async_trait;
use uc_core::network::PairingMessage;
use uc_core::ports::PairingTransportPort;

const DISABLED_PAIRING_RUNTIME_ERROR: &str = "local pairing runtime is disabled in this process";

/// Declares which process owns the local pairing runtime.
/// 声明哪个进程拥有本地 pairing runtime。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingRuntimeOwner {
    CurrentProcess,
    ExternalDaemon,
}

/// Pairing transport that always fails because this process does not host pairing runtime.
/// 当前进程不承载 pairing runtime 时使用的失败型 pairing transport。
#[derive(Debug, Default, Clone, Copy)]
pub struct DisabledPairingTransport;

#[async_trait]
impl PairingTransportPort for DisabledPairingTransport {
    async fn open_pairing_session(&self, _peer_id: String, _session_id: String) -> Result<()> {
        Err(anyhow::anyhow!(DISABLED_PAIRING_RUNTIME_ERROR))
    }

    async fn send_pairing_on_session(&self, _message: PairingMessage) -> Result<()> {
        Err(anyhow::anyhow!(DISABLED_PAIRING_RUNTIME_ERROR))
    }

    async fn close_pairing_session(
        &self,
        _session_id: String,
        _reason: Option<String>,
    ) -> Result<()> {
        Err(anyhow::anyhow!(DISABLED_PAIRING_RUNTIME_ERROR))
    }

    async fn unpair_device(&self, _peer_id: String) -> Result<()> {
        Err(anyhow::anyhow!(DISABLED_PAIRING_RUNTIME_ERROR))
    }
}
