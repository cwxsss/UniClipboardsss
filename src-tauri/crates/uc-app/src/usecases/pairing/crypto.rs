//! Crypto port bundle injected into the pairing state machine.
//!
//! Groups the three pure-cryptographic ports the state machine needs so its
//! constructors stay narrow and call sites pass a single shared `Arc`.

use std::sync::Arc;

use uc_core::ports::security::{
    IdentityFingerprintFactoryPort, PinHasherPort, ShortCodeGeneratorPort,
};

/// Cryptographic capabilities required by `PairingStateMachine`.
///
/// All three ports are pure (no IO, no mutable state) — the state machine
/// invokes them inline during `transition`. They are bundled together so
/// the state machine signature does not balloon with three separate
/// `Arc<dyn ...>` parameters.
pub struct PairingCryptoPorts {
    pub pin_hasher: Arc<dyn PinHasherPort>,
    pub short_code: Arc<dyn ShortCodeGeneratorPort>,
    pub fingerprint: Arc<dyn IdentityFingerprintFactoryPort>,
}

impl std::fmt::Debug for PairingCryptoPorts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairingCryptoPorts").finish_non_exhaustive()
    }
}
