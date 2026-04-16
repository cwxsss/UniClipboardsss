use async_trait::async_trait;
use std::sync::Arc;

/// 业务层可识别的同步目标标识。
///
/// 不直接绑定 libp2p::PeerId / iroh::NodeId / Multiaddr。
/// adapter 内部可自行做映射。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SyncTargetId(pub String);

/// 出站剪贴板传输帧。
///
/// transport 不解析业务协议，只转发上层已经封装好的原始字节。
/// 使用 Arc<[u8]> 支持多目标 fanout 时零拷贝共享。
#[derive(Debug, Clone)]
pub struct OutboundClipboardFrame(pub Arc<[u8]>);

/// 入站剪贴板传输帧。
///
/// transport 只负责把原始帧字节交给上层；
/// 协议解析和解密由更高层稳定处理。
#[derive(Debug, Clone)]
pub struct InboundClipboardFrame {
    /// 来源目标标识
    pub source: SyncTargetId,

    /// 原始协议帧字节
    pub frame: Vec<u8>,
}

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClipboardTransportError {
    #[error("target unavailable")]
    TargetUnavailable,

    #[error("send failed")]
    SendFailed,

    #[error("transport timeout")]
    Timeout,

    #[error("subscription closed")]
    SubscriptionClosed,

    #[error("unsupported transport operation")]
    Unsupported,

    #[error("internal transport error: {0}")]
    Internal(String),
}

#[async_trait]
pub trait ClipboardOutboundTransportPort: Send + Sync {
    /// 向一个同步目标发送一份剪贴板传输帧。
    ///
    /// 注意：
    /// - transport adapter 应自行处理发送前所需的链路准备
    /// - transport 不负责业务协议解析/封装
    /// - use case 不感知 stream / session / relay / hole punch / ticket 等细节
    async fn send_clipboard(
        &self,
        target: &SyncTargetId,
        frame: OutboundClipboardFrame,
    ) -> Result<(), ClipboardTransportError>;
}

/// Runtime-agnostic inbound message source.
///
/// Core 只关心“下一条消息是什么”，
/// 不关心底层是 tokio channel、broadcast、stream 还是其他机制。
#[async_trait]
pub trait ClipboardInboundMessageSource: Send {
    async fn recv(&mut self) -> Result<InboundClipboardFrame, ClipboardTransportError>;
}

#[async_trait]
pub trait ClipboardInboundTransportPort: Send + Sync {
    /// 订阅入站剪贴板传输帧。
    ///
    /// 约束：
    /// - transport 层只返回原始帧字节，不直接返回解析后的业务消息
    /// - transport 层不直接返回明文
    /// - adapter 可实现为单消费者通道
    async fn subscribe_clipboard(
        &self,
    ) -> Result<Box<dyn ClipboardInboundMessageSource>, ClipboardTransportError>;
}
