use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use crate::ports::{IdentityStoreError, IdentityStorePort};
use libp2p::identity::Keypair;
use uc_core::ports::SecureStoragePort;

const IDENTITY_KEY: &str = "libp2p-identity:v1";
const IDENTITY_DIR: &str = "identity";
const IDENTITY_FILE: &str = "libp2p_identity.pb";

fn load_identity_from_storage(
    storage: &dyn SecureStoragePort,
) -> Result<Option<Vec<u8>>, IdentityStoreError> {
    storage
        .get(IDENTITY_KEY)
        .map_err(|e| IdentityStoreError::Store(e.to_string()))
}

fn store_identity_in_storage(
    storage: &dyn SecureStoragePort,
    identity: &[u8],
) -> Result<(), IdentityStoreError> {
    storage
        .set(IDENTITY_KEY, identity)
        .map_err(|e| IdentityStoreError::Store(e.to_string()))
}

#[derive(Clone)]
pub struct SystemIdentityStore {
    storage: Arc<dyn SecureStoragePort>,
}

impl SystemIdentityStore {
    pub fn new(storage: Arc<dyn SecureStoragePort>) -> Self {
        Self { storage }
    }
}

impl IdentityStorePort for SystemIdentityStore {
    fn load_identity(&self) -> Result<Option<Vec<u8>>, IdentityStoreError> {
        load_identity_from_storage(self.storage.as_ref())
    }

    fn store_identity(&self, identity: &[u8]) -> Result<(), IdentityStoreError> {
        store_identity_in_storage(self.storage.as_ref(), identity)
    }
}

#[derive(Clone)]
pub struct FileIdentityStore {
    path: PathBuf,
}

impl FileIdentityStore {
    pub fn new(app_data_root: PathBuf) -> Self {
        let path = app_data_root.join(IDENTITY_DIR).join(IDENTITY_FILE);
        Self { path }
    }

    fn read_identity(&self) -> Result<Option<Vec<u8>>, IdentityStoreError> {
        match std::fs::read(&self.path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(IdentityStoreError::Store(format!(
                "failed to read identity file: {err}"
            ))),
        }
    }

    fn write_identity(&self, identity: &[u8]) -> Result<(), IdentityStoreError> {
        let parent = self.path.parent().ok_or_else(|| {
            IdentityStoreError::Store("identity path missing parent directory".to_string())
        })?;
        std::fs::create_dir_all(parent).map_err(|err| {
            IdentityStoreError::Store(format!("failed to create identity dir: {err}"))
        })?;

        let tmp_path = self.path.with_extension("tmp");
        std::fs::write(&tmp_path, identity).map_err(|err| {
            IdentityStoreError::Store(format!("failed to write identity temp file: {err}"))
        })?;

        std::fs::rename(&tmp_path, &self.path).map_err(|err| {
            IdentityStoreError::Store(format!("failed to commit identity file: {err}"))
        })?;

        Ok(())
    }
}

impl IdentityStorePort for FileIdentityStore {
    fn load_identity(&self) -> Result<Option<Vec<u8>>, IdentityStoreError> {
        self.read_identity()
    }

    fn store_identity(&self, identity: &[u8]) -> Result<(), IdentityStoreError> {
        self.write_identity(identity)
    }
}

pub fn load_or_create_identity(
    store: &dyn IdentityStorePort,
) -> Result<Keypair, IdentityStoreError> {
    if let Some(bytes) = store.load_identity()? {
        let keypair = Keypair::from_protobuf_encoding(&bytes).map_err(|e| {
            IdentityStoreError::Corrupt(format!("failed to decode identity keypair: {e}"))
        })?;
        Ok(keypair)
    } else {
        let keypair = Keypair::generate_ed25519();
        let bytes = keypair.to_protobuf_encoding().map_err(|e| {
            IdentityStoreError::Store(format!("failed to encode identity keypair: {e}"))
        })?;
        store.store_identity(&bytes)?;
        Ok(keypair)
    }
}
