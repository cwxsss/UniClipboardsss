//! iroh-blobs backed implementation of [`BlobTransferPort`].
//!
//! Adapter 只处理已经加密好的密文字节:发布到本地 iroh-blobs store、生成
//! ticket、按 ticket 拉取、记录本地保留标签。加解密与明文去重分别由
//! 上层 use case 和 sqlite `BlobReferenceRepositoryPort` 负责。

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use iroh::Endpoint;
use iroh_blobs::{
    store::fs::FsStore, ticket::BlobTicket as NativeBlobTicket, BlobFormat, Hash, HashAndFormat,
};
use iroh_tickets::Ticket;
use tracing::{debug, instrument};

use uc_core::ports::blob::{BlobDigest, BlobError, BlobTicket, BlobTransferPort, TagReason};

pub const BLOBS_ALPN: &[u8] = iroh_blobs::ALPN;

pub struct IrohBlobTransferAdapter {
    endpoint: Arc<Endpoint>,
    store: FsStore,
}

impl IrohBlobTransferAdapter {
    pub fn new(endpoint: Arc<Endpoint>, store: FsStore) -> Self {
        Self { endpoint, store }
    }

    fn native_hash(digest: &BlobDigest) -> Hash {
        Hash::from_bytes(*digest.as_bytes())
    }

    fn core_digest(hash: Hash) -> BlobDigest {
        BlobDigest::from_bytes(*hash.as_bytes())
    }

    fn parse_ticket(ticket: &BlobTicket) -> Result<NativeBlobTicket, BlobError> {
        NativeBlobTicket::from_bytes(ticket.as_bytes()).map_err(|_| BlobError::InvalidTicket)
    }

    fn tag_name(reason: &TagReason) -> String {
        match reason {
            TagReason::ClipboardEntry(entry_id) => {
                format!("uc-clipboard-entry:{}", entry_id.as_ref())
            }
        }
    }
}

#[async_trait]
impl BlobTransferPort for IrohBlobTransferAdapter {
    #[instrument(skip_all, fields(bytes = ciphertext.len()))]
    async fn publish(&self, ciphertext: Bytes) -> Result<BlobDigest, BlobError> {
        let tag = self
            .store
            .blobs()
            .add_bytes(ciphertext)
            .await
            .map_err(|e| BlobError::Internal(e.to_string()))?;
        Ok(Self::core_digest(tag.hash))
    }

    #[instrument(skip_all)]
    async fn issue_ticket(&self, digest: &BlobDigest) -> Result<BlobTicket, BlobError> {
        if !self.has(digest).await? {
            return Err(BlobError::NotFound);
        }

        let ticket = NativeBlobTicket::new(
            self.endpoint.addr(),
            Self::native_hash(digest),
            BlobFormat::Raw,
        );
        Ok(BlobTicket::from_bytes(ticket.to_bytes()))
    }

    #[instrument(skip_all)]
    async fn fetch(&self, ticket: &BlobTicket) -> Result<Bytes, BlobError> {
        let native = Self::parse_ticket(ticket)?;
        let digest = Self::core_digest(native.hash());
        if self.has(&digest).await? {
            return self
                .store
                .blobs()
                .get_bytes(native.hash())
                .await
                .map_err(|e| BlobError::Internal(e.to_string()));
        }

        let connection = self
            .endpoint
            .connect(native.addr().clone(), BLOBS_ALPN)
            .await
            .map_err(|e| BlobError::Unavailable(e.to_string()))?;
        drop(connection);

        self.store
            .downloader(&self.endpoint)
            .download(native.hash_and_format(), [native.addr().id])
            .await
            .map_err(|e| BlobError::Unavailable(e.to_string()))?;

        self.store
            .blobs()
            .get_bytes(native.hash())
            .await
            .map_err(|e| BlobError::Unavailable(e.to_string()))
    }

    #[instrument(skip_all)]
    async fn has(&self, digest: &BlobDigest) -> Result<bool, BlobError> {
        let hash = Self::native_hash(digest);
        let observed = self
            .store
            .blobs()
            .observe(hash)
            .await
            .map_err(|e| BlobError::Internal(e.to_string()))?;
        Ok(observed.is_complete())
    }

    #[instrument(skip_all)]
    async fn tag(&self, digest: &BlobDigest, reason: TagReason) -> Result<(), BlobError> {
        let name = Self::tag_name(&reason);
        self.store
            .tags()
            .set(
                name.as_bytes(),
                HashAndFormat::raw(Self::native_hash(digest)),
            )
            .await
            .map_err(|e| BlobError::Internal(e.to_string()))
    }

    #[instrument(skip_all)]
    async fn untag(&self, _digest: &BlobDigest, reason: TagReason) -> Result<(), BlobError> {
        let name = Self::tag_name(&reason);
        let removed = self
            .store
            .tags()
            .delete(name.as_bytes())
            .await
            .map_err(|e| BlobError::Internal(e.to_string()))?;
        debug!(removed, "blob tag removed");
        Ok(())
    }

    fn digest_of(&self, ticket: &BlobTicket) -> Result<BlobDigest, BlobError> {
        let native = Self::parse_ticket(ticket)?;
        Ok(Self::core_digest(native.hash()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;
    use std::time::Duration;

    use iroh::{protocol::Router, RelayMode};
    use tempfile::{tempdir, TempDir};
    use uc_core::ids::EntryId;

    struct Fixture {
        adapter: IrohBlobTransferAdapter,
        router: Router,
        store: FsStore,
        _tempdir: TempDir,
    }

    impl Fixture {
        async fn bind() -> anyhow::Result<Self> {
            let tempdir = tempdir()?;
            let store = FsStore::load(store_path(&tempdir)).await?;
            let endpoint = Endpoint::builder()
                .relay_mode(RelayMode::Disabled)
                .bind()
                .await?;
            let protocol = iroh_blobs::BlobsProtocol::new(&store, None);
            let router = Router::builder(endpoint.clone())
                .accept(BLOBS_ALPN, protocol)
                .spawn();
            let endpoint = Arc::new(endpoint);
            let adapter = IrohBlobTransferAdapter::new(endpoint, store.clone());

            Ok(Self {
                adapter,
                router,
                store,
                _tempdir: tempdir,
            })
        }

        async fn wait_for_direct_addr(&self) -> anyhow::Result<()> {
            for _ in 0..100 {
                if self.router.endpoint().addr().ip_addrs().next().is_some() {
                    return Ok(());
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            anyhow::bail!("iroh endpoint never published a loopback direct address")
        }

        async fn shutdown(self) -> anyhow::Result<()> {
            self.router.shutdown().await?;
            Ok(())
        }
    }

    fn store_path(tempdir: &TempDir) -> PathBuf {
        tempdir.path().join("iroh-blobs")
    }

    fn unknown_digest() -> BlobDigest {
        BlobDigest::from_bytes([0x7f; 32])
    }

    #[tokio::test]
    async fn publish_same_bytes_returns_stable_digest() -> anyhow::Result<()> {
        let fixture = Fixture::bind().await?;
        let payload = Bytes::from_static(b"slice3-t4-stable");

        let first = fixture.adapter.publish(payload.clone()).await?;
        let second = fixture.adapter.publish(payload).await?;

        assert_eq!(first, second);
        fixture.shutdown().await?;
        Ok(())
    }

    #[tokio::test]
    async fn has_reports_present_and_missing_blobs() -> anyhow::Result<()> {
        let fixture = Fixture::bind().await?;

        let digest = fixture
            .adapter
            .publish(Bytes::from_static(b"slice3-t4-has"))
            .await?;

        assert!(fixture.adapter.has(&digest).await?);
        assert!(!fixture.adapter.has(&unknown_digest()).await?);
        fixture.shutdown().await?;
        Ok(())
    }

    #[tokio::test]
    async fn issue_ticket_and_digest_of_round_trip() -> anyhow::Result<()> {
        let fixture = Fixture::bind().await?;
        fixture.wait_for_direct_addr().await?;
        let digest = fixture
            .adapter
            .publish(Bytes::from_static(b"slice3-t4-ticket"))
            .await?;

        let ticket = fixture.adapter.issue_ticket(&digest).await?;

        assert_eq!(fixture.adapter.digest_of(&ticket)?, digest);
        assert_eq!(
            BlobTicket::from_bytes(ticket.as_bytes().to_vec()).as_bytes(),
            ticket.as_bytes()
        );
        fixture.shutdown().await?;
        Ok(())
    }

    #[tokio::test]
    async fn digest_of_invalid_ticket_returns_invalid_ticket() -> anyhow::Result<()> {
        let fixture = Fixture::bind().await?;
        let ticket = BlobTicket::from_bytes(vec![1, 2, 3, 4, 5]);

        let err = fixture
            .adapter
            .digest_of(&ticket)
            .expect_err("corrupt ticket must fail");

        assert!(matches!(err, BlobError::InvalidTicket));
        fixture.shutdown().await?;
        Ok(())
    }

    #[tokio::test]
    async fn issue_ticket_for_missing_digest_returns_not_found() -> anyhow::Result<()> {
        let fixture = Fixture::bind().await?;

        let err = fixture
            .adapter
            .issue_ticket(&unknown_digest())
            .await
            .expect_err("missing digest must not mint a ticket");

        assert!(matches!(err, BlobError::NotFound));
        fixture.shutdown().await?;
        Ok(())
    }

    #[tokio::test]
    async fn fetch_self_ticket_returns_original_bytes() -> anyhow::Result<()> {
        let fixture = Fixture::bind().await?;
        fixture.wait_for_direct_addr().await?;
        let payload = Bytes::from_static(b"slice3-t5-self-fetch");
        let digest = fixture.adapter.publish(payload.clone()).await?;
        let ticket = fixture.adapter.issue_ticket(&digest).await?;

        let fetched = fixture.adapter.fetch(&ticket).await?;

        assert_eq!(fetched, payload);
        fixture.shutdown().await?;
        Ok(())
    }

    #[tokio::test]
    async fn fetch_remote_ticket_returns_original_bytes() -> anyhow::Result<()> {
        let provider = Fixture::bind().await?;
        let receiver = Fixture::bind().await?;
        provider.wait_for_direct_addr().await?;
        receiver.wait_for_direct_addr().await?;
        let payload = Bytes::from_static(b"slice3-t5-remote-fetch");
        let digest = provider.adapter.publish(payload.clone()).await?;
        let ticket = provider.adapter.issue_ticket(&digest).await?;

        let fetched = receiver.adapter.fetch(&ticket).await?;

        assert_eq!(fetched, payload);
        receiver.shutdown().await?;
        provider.shutdown().await?;
        Ok(())
    }

    #[tokio::test]
    async fn tag_then_untag_is_idempotent() -> anyhow::Result<()> {
        let fixture = Fixture::bind().await?;
        let digest = fixture
            .adapter
            .publish(Bytes::from_static(b"slice3-t6-tag"))
            .await?;
        let reason = TagReason::ClipboardEntry(EntryId::from_str("entry-a"));

        fixture.adapter.tag(&digest, reason.clone()).await?;
        fixture.adapter.untag(&digest, reason.clone()).await?;
        fixture.adapter.untag(&digest, reason).await?;

        fixture.shutdown().await?;
        Ok(())
    }

    #[tokio::test]
    async fn multiple_tag_reasons_are_independent() -> anyhow::Result<()> {
        let fixture = Fixture::bind().await?;
        let digest = fixture
            .adapter
            .publish(Bytes::from_static(b"slice3-t6-multi-tag"))
            .await?;
        let first = TagReason::ClipboardEntry(EntryId::from_str("entry-a"));
        let second = TagReason::ClipboardEntry(EntryId::from_str("entry-b"));

        fixture.adapter.tag(&digest, first.clone()).await?;
        fixture.adapter.tag(&digest, second.clone()).await?;
        fixture.adapter.untag(&digest, first.clone()).await?;

        let second_tag = IrohBlobTransferAdapter::tag_name(&second);
        assert!(fixture
            .store
            .tags()
            .get(second_tag.as_bytes())
            .await?
            .is_some());

        fixture.adapter.untag(&digest, second).await?;
        fixture.shutdown().await?;
        Ok(())
    }
}
