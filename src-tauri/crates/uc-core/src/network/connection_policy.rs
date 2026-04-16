use crate::pairing::PairingState;

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
    pub fn allowed_protocols(state: PairingState) -> AllowedProtocols {
        match state {
            PairingState::Trusted => AllowedProtocols {
                pairing: true,
                business: true,
            },
            PairingState::Pending | PairingState::Revoked => AllowedProtocols {
                pairing: true,
                business: false,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedConnectionPolicy {
    pub pairing_state: PairingState,
    pub allowed: AllowedProtocols,
}
