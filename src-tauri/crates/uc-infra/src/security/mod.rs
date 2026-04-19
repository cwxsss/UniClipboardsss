mod blob_cipher_adapter;
mod decrypting_clipboard_event_repo;
mod decrypting_representation_repo;
mod default_key_scope;
mod encrypted_blob_store;
mod encrypting_clipboard_event_writer;
mod encryption_state;
mod encryption_state_repo;
mod hashing;
mod identity_fingerprint;
mod key_material;
mod scope_identifier;
mod session;
mod space_access_adapter;
pub(crate) mod v1_aead;

pub use blob_cipher_adapter::BlobCipherAdapter;
pub use decrypting_clipboard_event_repo::DecryptingClipboardEventRepository;
pub use decrypting_representation_repo::DecryptingClipboardRepresentationRepository;
pub use default_key_scope::DefaultKeyScope;
pub use encrypted_blob_store::EncryptedBlobStore;
pub use encrypting_clipboard_event_writer::EncryptingClipboardEventWriter;
pub use encryption_state_repo::FileEncryptionStateRepository;
pub use hashing::{hash_pin, verify_pin, Argon2PinHasher, Blake3Hasher};
pub use identity_fingerprint::{
    FingerprintError, IdentityFingerprint, Sha256IdentityFingerprintFactory,
    Sha256ShortCodeGenerator, ShortCodeGenerator,
};
pub use key_material::KeyMaterialStore;
pub use session::InMemorySession;
pub use space_access_adapter::DefaultSpaceAccessAdapter;
