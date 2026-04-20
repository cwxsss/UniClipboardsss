//! Process-wide iroh node shared by every Slice 1+ transport.
//!
//! A single [`iroh::Endpoint`] per process owns the Ed25519 identity, the
//! UDP socket, and the NAT-traversal / relay state. Every business
//! transport (pairing, clipboard sync, blob transfer) registers its own
//! ALPN on the same [`iroh::protocol::Router`] instead of binding a new
//! endpoint — see `uc-infra/AGENTS.md` §4.2 (technical detail stays
//! contained) and the Slice 1 decision log on shared endpoint ownership.
//!
//! The builder pattern is deliberate: each `install_*` method is where a
//! new transport slices in. Slice 1 ships [`install_pairing`]; Slice 2 / 3
//! will add `install_clipboard` / `install_blobs` on the same builder.
//!
//! [`install_pairing`]: IrohNodeBuilder::install_pairing

use std::sync::Arc;

use iroh::protocol::{Router, RouterBuilder};
use iroh::{Endpoint, RelayMode};
use tracing::{debug, instrument};

use uc_core::ports::pairing::{PairingEventPort, PairingSessionPort};
use uc_core::ports::pairing_invitation::PairingInvitationPort;
use uc_core::ports::{DeviceIdentityPort, LocalIdentityError, SettingsPort};

use crate::pairing::{IrohPairingSessionAdapter, PAIRING_ALPN};
use crate::rendezvous::RendezvousPairingInvitationAdapter;

use super::identity_store::IrohIdentityStore;

/// The three pairing ports produced by [`IrohNodeBuilder::install_pairing`].
///
/// `session` and `events` share the same underlying
/// [`IrohPairingSessionAdapter`] — both trait objects point at one Arc so
/// sponsor-side inbound events and the outbound dial/send path use the same
/// session map. `invitation` is the rendezvous HTTP adapter, which talks to
/// the same endpoint (its ticket = the endpoint's own [`iroh::EndpointAddr`]).
pub struct PairingHandlers {
    pub session: Arc<dyn PairingSessionPort>,
    pub events: Arc<dyn PairingEventPort>,
    pub invitation: Arc<dyn PairingInvitationPort>,
}

/// Live iroh node with a spawned [`Router`].
///
/// Owns the [`Router`] so shutdown runs through a single call site; Slice 2 /
/// 3 add handlers by extending [`IrohNodeBuilder`], not by adding shutdown
/// paths here.
pub struct IrohNode {
    #[allow(dead_code)] // held so the endpoint stays alive for the router's lifetime
    endpoint: Arc<Endpoint>,
    router: Router,
}

impl IrohNode {
    /// Shut the iroh node down cleanly. Triggers
    /// [`iroh::protocol::ProtocolHandler::shutdown`] on every registered
    /// handler, stops the accept loop, and drops the underlying UDP socket
    /// + relay session.
    ///
    /// Best-effort: caller is on the teardown path so we log and swallow
    /// the error — there is no recourse, and leaking an iroh node past a
    /// process exit is harmless (the OS reaps the socket).
    #[instrument(skip_all)]
    pub async fn shutdown(self) {
        if let Err(err) = self.router.shutdown().await {
            tracing::warn!(error = %err, "iroh router shutdown failed; continuing teardown");
        }
        debug!("iroh node shut down");
    }
}

/// Bootstrap-time configuration for [`IrohNodeBuilder`]. Defaults cover
/// production; integration tests override the rendezvous URL (pointing at
/// a mock server) and usually disable relays (loopback-only handshake).
#[derive(Debug, Clone, Default)]
pub struct IrohNodeConfig {
    /// Override rendezvous base URL. `None` → use
    /// [`crate::rendezvous::RENDEZVOUS_BASE_URL`].
    pub rendezvous_base_url: Option<String>,
    /// If true, bind the endpoint with iroh's relays disabled. Needed for
    /// loopback-only integration tests; production leaves this `false` so
    /// iroh can fall back to the public relay mesh when NAT blocks direct
    /// UDP.
    pub disable_relays: bool,
}

/// Staged builder — bind endpoint, install transport handlers, then
/// [`spawn`](Self::spawn) the router.
pub struct IrohNodeBuilder {
    endpoint: Arc<Endpoint>,
    /// Held in `Option` so `install_*` methods can `take()` + reassign the
    /// builder (iroh's `RouterBuilder::accept` consumes `self`).
    router_builder: Option<RouterBuilder>,
    /// Retained so `install_*` methods can read the rendezvous override
    /// when constructing the per-transport adapters.
    config: IrohNodeConfig,
}

impl IrohNodeBuilder {
    /// Bind the iroh endpoint, reusing the Ed25519 secret persisted by
    /// [`IrohIdentityStore`] so the endpoint's on-wire identity matches the
    /// fingerprint `LocalIdentityPort` hands out to domain code.
    ///
    /// Registers [`PAIRING_ALPN`] up front — Slice 1 always has pairing. A
    /// future slice that wants to opt out would add a separate `bind_bare`
    /// constructor; there's no Slice 1 use case for that.
    #[instrument(skip_all)]
    pub async fn bind(
        identity_store: &IrohIdentityStore,
        config: IrohNodeConfig,
    ) -> Result<Self, IrohNodeError> {
        let secret = identity_store.ensure_secret_key()?;
        let relay_mode = if config.disable_relays {
            RelayMode::Disabled
        } else {
            RelayMode::Default
        };
        let endpoint = Endpoint::builder()
            .secret_key(secret)
            .alpns(vec![PAIRING_ALPN.to_vec()])
            .relay_mode(relay_mode)
            .bind()
            .await
            .map_err(|err| IrohNodeError::Bind(err.to_string()))?;
        let endpoint = Arc::new(endpoint);
        let router_builder = Router::builder((*endpoint).clone());
        debug!(
            endpoint_id = %endpoint.id().fmt_short(),
            disable_relays = config.disable_relays,
            rendezvous_override = config.rendezvous_base_url.is_some(),
            "iroh node bound; ready to install transport handlers"
        );
        Ok(Self {
            endpoint,
            router_builder: Some(router_builder),
            config,
        })
    }

    /// Install the pairing transport:
    ///
    /// * Registers [`IrohPairingSessionAdapter`] as the [`PAIRING_ALPN`]
    ///   [`iroh::protocol::ProtocolHandler`] so sponsor-side incoming
    ///   connections are accepted.
    /// * Returns the pairing session / event / invitation ports. The first
    ///   two are the same `Arc` cast to two trait objects.
    pub fn install_pairing(
        &mut self,
        device_identity: Arc<dyn DeviceIdentityPort>,
        settings: Arc<dyn SettingsPort>,
    ) -> PairingHandlers {
        let adapter = Arc::new(match &self.config.rendezvous_base_url {
            Some(url) => {
                IrohPairingSessionAdapter::with_base_url(Arc::clone(&self.endpoint), url.clone())
            }
            None => IrohPairingSessionAdapter::new(Arc::clone(&self.endpoint)),
        });

        // `RouterBuilder::accept` consumes `self`; take + reassign so the
        // builder can be called again for a Slice 2 handler in the same
        // chain.
        let builder = self
            .router_builder
            .take()
            .expect("router_builder missing — install_* called after spawn");
        let builder = adapter.install_handler(builder);
        self.router_builder = Some(builder);

        let invitation: Arc<dyn PairingInvitationPort> =
            Arc::new(match self.config.rendezvous_base_url.clone() {
                Some(url) => RendezvousPairingInvitationAdapter::with_base_url(
                    Arc::clone(&self.endpoint),
                    device_identity,
                    settings,
                    url,
                ),
                None => RendezvousPairingInvitationAdapter::new(
                    Arc::clone(&self.endpoint),
                    device_identity,
                    settings,
                ),
            });

        PairingHandlers {
            session: adapter.clone(),
            events: adapter,
            invitation,
        }
    }

    /// Finalize the builder: spawn the [`Router`]. After this point no more
    /// `install_*` calls are allowed.
    pub fn spawn(self) -> IrohNode {
        let router = self
            .router_builder
            .expect("router_builder missing — spawn called twice")
            .spawn();
        IrohNode {
            endpoint: self.endpoint,
            router,
        }
    }
}

/// Bootstrap-time failures binding the iroh endpoint. Kept small on
/// purpose — deeper iroh errors are summarised into a string rather than
/// threaded as typed variants per `uc-infra/AGENTS.md` §9.1 (infra error
/// types don't leak third-party error types upward).
#[derive(Debug, thiserror::Error)]
pub enum IrohNodeError {
    #[error("failed to bind iroh endpoint: {0}")]
    Bind(String),

    #[error(transparent)]
    Identity(#[from] LocalIdentityError),
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;
    use std::sync::Mutex as StdMutex;

    use async_trait::async_trait;
    use uc_core::ids::DeviceId;
    use uc_core::ports::{SecureStorageError, SecureStoragePort};
    use uc_core::settings::model::Settings;

    use crate::security::Sha256IdentityFingerprintFactory;

    #[derive(Default)]
    struct InMemorySecureStorage {
        map: StdMutex<HashMap<String, Vec<u8>>>,
    }
    impl SecureStoragePort for InMemorySecureStorage {
        fn get(&self, key: &str) -> Result<Option<Vec<u8>>, SecureStorageError> {
            Ok(self.map.lock().unwrap().get(key).cloned())
        }
        fn set(&self, key: &str, value: &[u8]) -> Result<(), SecureStorageError> {
            self.map
                .lock()
                .unwrap()
                .insert(key.to_string(), value.to_vec());
            Ok(())
        }
        fn delete(&self, key: &str) -> Result<(), SecureStorageError> {
            self.map.lock().unwrap().remove(key);
            Ok(())
        }
    }

    struct FixedDeviceIdentity(DeviceId);
    impl DeviceIdentityPort for FixedDeviceIdentity {
        fn current_device_id(&self) -> DeviceId {
            self.0.clone()
        }
    }

    struct InMemorySettings(StdMutex<Settings>);
    #[async_trait]
    impl SettingsPort for InMemorySettings {
        async fn load(&self) -> anyhow::Result<Settings> {
            Ok(self.0.lock().unwrap().clone())
        }
        async fn save(&self, s: &Settings) -> anyhow::Result<()> {
            *self.0.lock().unwrap() = s.clone();
            Ok(())
        }
    }

    fn identity_store() -> Arc<IrohIdentityStore> {
        Arc::new(IrohIdentityStore::new(
            Arc::new(InMemorySecureStorage::default()),
            Arc::new(Sha256IdentityFingerprintFactory),
        ))
    }

    #[tokio::test]
    async fn bind_install_pairing_spawn_and_shutdown_cleanly() {
        let store = identity_store();
        let mut builder = IrohNodeBuilder::bind(&store, IrohNodeConfig::default())
            .await
            .expect("bind");
        let handlers = builder.install_pairing(
            Arc::new(FixedDeviceIdentity(DeviceId::new("device-1"))),
            Arc::new(InMemorySettings(StdMutex::new(Settings::default()))),
        );
        // Ports are handed out as trait objects so ownership (and hence
        // the session adapter) survives past the node's spawn.
        drop(handlers);
        let node = builder.spawn();
        // Clean shutdown exits without hanging; the test runner's default
        // timeout would catch a deadlock.
        node.shutdown().await;
    }

    #[tokio::test]
    async fn bind_is_idempotent_across_builds_for_same_store() {
        // The endpoint id is derived from the Ed25519 secret, so a second
        // bind against the same store must see the same id (rotating it
        // would break every peer that already remembered us).
        let store = identity_store();
        let first = IrohNodeBuilder::bind(&store, IrohNodeConfig::default())
            .await
            .expect("first bind");
        let first_id = first.endpoint.id();
        let first_node = first.spawn();
        first_node.shutdown().await;

        let second = IrohNodeBuilder::bind(&store, IrohNodeConfig::default())
            .await
            .expect("second bind");
        assert_eq!(second.endpoint.id(), first_id);
        second.spawn().shutdown().await;
    }
}
