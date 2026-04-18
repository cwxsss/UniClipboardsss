//! Network protocol types.

pub mod connection_policy;
pub mod events;
pub mod protocol;
pub mod session;

pub use connection_policy::{
    AllowedProtocols, ConnectionPolicy, PeerTrustStatus, ProtocolKind, ResolvedConnectionPolicy,
};
pub use events::{
    ConnectedPeer, DiscoveredPeer, NetworkEvent, NetworkStatus, ProtocolDenyReason,
    ProtocolDirection,
};
pub use protocol::{BinaryRepresentation, ClipboardBinaryPayload};
pub use protocol::{
    ClipboardMessage, DeviceAnnounceMessage, HeartbeatMessage, PairingBusy, PairingCancel,
    PairingChallenge, PairingChallengeResponse, PairingConfirm, PairingKeyslotOffer,
    PairingMessage, PairingReject, PairingRequest, PairingResponse, ProtocolMessage,
    MIME_IMAGE_PREFIX, MIME_TEXT_HTML, MIME_TEXT_PLAIN, MIME_TEXT_RTF,
};
pub use session::SessionId;
