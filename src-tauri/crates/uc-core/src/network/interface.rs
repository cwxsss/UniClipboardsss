//! Network-related domain models.
//!
//! Contains network interface information, manual connection requests, etc.

use serde::{Deserialize, Serialize};

/// Network interface information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterface {
    /// Interface name (e.g., "en0", "Wi-Fi", "以太网")
    pub name: String,
    /// IP address
    pub ip: String,
    /// Is loopback address
    pub is_loopback: bool,
    /// Is IPv4
    pub is_ipv4: bool,
}

/// Manual connection request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualConnectionRequest {
    /// Target device IP address
    pub ip: String,
    /// Target device port
    pub port: u16,
}

/// Manual connection response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualConnectionResponse {
    /// Whether successful
    pub success: bool,
    /// Device ID (returned on success)
    pub device_id: Option<String>,
    /// Response message
    pub message: String,
}

/// Connection request message (sent to receiver)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionRequestMessage {
    /// Requester device ID
    pub requester_device_id: String,
    /// Requester IP address
    pub requester_ip: String,
    /// Requester device alias (optional)
    pub requester_alias: Option<String>,
    /// Requester platform (optional)
    pub requester_platform: Option<String>,
}

/// Connection response message (receiver returns to initiator)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionResponseMessage {
    /// Whether to accept connection
    pub accepted: bool,
    /// Responder device ID
    pub responder_device_id: String,
    /// Responder IP address (optional)
    pub responder_ip: Option<String>,
    /// Responder device alias (optional)
    pub responder_alias: Option<String>,
}

/// Connection request decision (frontend user confirmation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionRequestDecision {
    /// Whether to accept connection
    pub accept: bool,
    /// Requester device ID
    pub requester_device_id: String,
}
