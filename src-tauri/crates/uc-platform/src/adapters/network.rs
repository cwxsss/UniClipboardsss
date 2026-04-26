//! Disabled network stub.
//!
//! Slice 4 P5b 物理删除了 libp2p adapter,5c 又拆掉了 6 个废弃 trait 与
//! `NetworkPorts` 聚合。剩下的 `NetworkControlPort` 只剩
//! `SpaceSetupFacade.auto_start_network` / `on_shutdown` 两处 best-effort
//! 调用——iroh endpoint 由 `SpaceSetupAssembly` 直接驱动,这里全部 no-op。
//!
//! 后续 phase 把 SpaceSetupFacade 的 NetworkControlPort 依赖也拆掉之后,
//! `NetworkControlPort` trait + 本桩可以一起退场。

use anyhow::Result;
use async_trait::async_trait;

use uc_core::ports::NetworkControlPort;

#[derive(Debug, Default, Clone, Copy)]
pub struct DisabledNetwork;

#[async_trait]
impl NetworkControlPort for DisabledNetwork {
    async fn start_network(&self) -> Result<()> {
        tracing::debug!(
            "DisabledNetwork::start_network — no-op (iroh stack drives real transport)"
        );
        Ok(())
    }

    async fn stop_network(&self) -> Result<()> {
        Ok(())
    }
}
