//! libp2p stream protocol identifiers shared across network adapters.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolId {
    Pairing,
    PairingStream,
    Business,
    FileTransfer,
    FileTransferV2,
}

impl ProtocolId {
    pub const fn as_str(&self) -> &'static str {
        match self {
            ProtocolId::Pairing => "/uc-pairing/1.0.0",
            ProtocolId::PairingStream => "/uniclipboard/pairing-stream/1.0.0",
            ProtocolId::Business => "/uniclipboard/business/1.0.0",
            ProtocolId::FileTransfer => "/uniclipboard/file-transfer/1.0.0",
            ProtocolId::FileTransferV2 => "/uniclipboard/file-transfer/2.0.0",
        }
    }
}
