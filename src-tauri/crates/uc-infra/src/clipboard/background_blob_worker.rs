//! Background worker to materialize blobs from staged representations.
//! 从暂存表示中异步生成 blob 的后台工作者。

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{debug, error, info_span, warn, Instrument};
use uc_core::clipboard::{MimeType, PayloadAvailability};
use uc_core::clipboard::{ThumbnailMetadata, TimestampMs};
use uc_core::ids::RepresentationId;
use uc_core::ports::clipboard::{
    ProcessingUpdateOutcome, ThumbnailGeneratorPort, ThumbnailRepositoryPort,
};
use uc_core::ports::{
    BlobWriterPort, ClipboardRepresentationRepositoryPort, ClockPort, ContentHashPort,
};

use crate::clipboard::{RepresentationCache, SpoolManager};

/// Check if an image MIME type needs conversion to PNG before blob storage.
/// Returns true for image/* types that are not already PNG or WebP.
fn should_convert_to_png(mime: Option<&MimeType>) -> bool {
    match mime {
        Some(m) => {
            let s = m.as_str();
            s.starts_with("image/") && s != "image/png" && s != "image/webp"
        }
        None => false,
    }
}

/// Result of converting an image to PNG, including pre-decoded RGBA pixels
/// to avoid re-decoding for thumbnail generation.
struct ConvertedImage {
    png_bytes: Vec<u8>,
    rgba_bytes: Vec<u8>,
    width: u32,
    height: u32,
}

/// Convert image bytes (any format supported by the `image` crate) to PNG.
///
/// Uses fast compression because the blob store applies zstd compression on top,
/// making aggressive PNG compression redundant. This significantly reduces encoding time
/// for large images (e.g. 34MB TIFF → PNG drops from ~2s to ~0.5s).
///
/// Returns both PNG bytes and pre-decoded RGBA pixels to avoid re-decoding for thumbnails.
fn convert_image_to_png(image_bytes: &[u8]) -> Result<ConvertedImage> {
    let img = image::load_from_memory(image_bytes).context("decode image for PNG conversion")?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();

    let mut png_bytes = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new_with_quality(
        std::io::Cursor::new(&mut png_bytes),
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Sub,
    );
    img.write_with_encoder(encoder)
        .context("encode image as PNG")?;

    Ok(ConvertedImage {
        png_bytes,
        rgba_bytes: rgba.into_raw(),
        width,
        height,
    })
}

/// Background worker that materializes blob data from cache/spool.
/// 从缓存/磁盘缓存中物化 blob 数据的后台工作者。
pub struct BackgroundBlobWorker {
    worker_rx: mpsc::Receiver<RepresentationId>,
    cache: Arc<RepresentationCache>,
    spool: Arc<SpoolManager>,
    repo: Arc<dyn ClipboardRepresentationRepositoryPort>,
    blob_writer: Arc<dyn BlobWriterPort>,
    hasher: Arc<dyn ContentHashPort>,
    thumbnail_repo: Arc<dyn ThumbnailRepositoryPort>,
    thumbnail_generator: Arc<dyn ThumbnailGeneratorPort>,
    clock: Arc<dyn ClockPort>,
    retry_max_attempts: u32,
    retry_backoff: Duration,
}

impl BackgroundBlobWorker {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        worker_rx: mpsc::Receiver<RepresentationId>,
        cache: Arc<RepresentationCache>,
        spool: Arc<SpoolManager>,
        repo: Arc<dyn ClipboardRepresentationRepositoryPort>,
        blob_writer: Arc<dyn BlobWriterPort>,
        hasher: Arc<dyn ContentHashPort>,
        thumbnail_repo: Arc<dyn ThumbnailRepositoryPort>,
        thumbnail_generator: Arc<dyn ThumbnailGeneratorPort>,
        clock: Arc<dyn ClockPort>,
        retry_max_attempts: u32,
        retry_backoff: Duration,
    ) -> Self {
        Self {
            worker_rx,
            cache,
            spool,
            repo,
            blob_writer,
            hasher,
            thumbnail_repo,
            thumbnail_generator,
            clock,
            retry_max_attempts,
            retry_backoff,
        }
    }

    /// Run the worker loop until the channel is closed.
    /// 运行工作循环，直到通道关闭。
    pub async fn run(mut self) {
        while let Some(rep_id) = self.worker_rx.recv().await {
            let span = info_span!(
                "infra.background_blob_worker",
                representation_id = %rep_id,
            );
            let result = self.process_with_retry(rep_id).instrument(span).await;
            if let Err(err) = result {
                error!(error = %err, "Failed to process representation");
            }
        }
    }

    async fn process_with_retry(&self, rep_id: RepresentationId) -> Result<()> {
        let mut attempt: u32 = 1;
        loop {
            match self.process_once(&rep_id).await {
                Ok(ProcessResult::Completed) => return Ok(()),
                Ok(ProcessResult::MissingBytes) => return Ok(()),
                Err(err) => {
                    if attempt >= self.retry_max_attempts {
                        let last_error = format!("worker failed after {attempt} attempts: {err}");
                        self.mark_failed(&rep_id, &last_error).await?;
                        return Err(err);
                    }

                    warn!(
                        attempt,
                        max_attempts = self.retry_max_attempts,
                        error = %err,
                        "Processing failed; retrying"
                    );
                    let backoff = self.retry_backoff.mul_f32(attempt as f32);
                    sleep(backoff).await;
                    attempt = attempt.saturating_add(1);
                }
            }
        }
    }

    async fn process_once(&self, rep_id: &RepresentationId) -> Result<ProcessResult> {
        // Transition to Processing (idempotent for staged/processing).
        match self
            .repo
            .update_processing_result(
                rep_id,
                &[PayloadAvailability::Staged, PayloadAvailability::Processing],
                None,
                PayloadAvailability::Processing,
                None,
            )
            .await
        {
            Ok(ProcessingUpdateOutcome::Updated(_)) => {}
            Ok(ProcessingUpdateOutcome::StateMismatch) => {
                warn!(
                    representation_id = %rep_id,
                    "Skipping processing due to state mismatch"
                );
                return Ok(ProcessResult::Completed);
            }
            Ok(ProcessingUpdateOutcome::NotFound) => {
                warn!(representation_id = %rep_id, "Representation missing");
                return Ok(ProcessResult::Completed);
            }
            Err(err) => {
                // Propagate error to trigger retry in process_with_retry
                return Err(err);
            }
        }

        let cached = self.cache.get(rep_id).await;

        let raw_bytes = if let Some(bytes) = cached {
            tracing::debug!(representation_id = %rep_id, "Worker cache hit");
            bytes
        } else {
            match self.spool.read(rep_id).await? {
                Some(bytes) => {
                    tracing::debug!(representation_id = %rep_id, "Worker spool hit");
                    bytes
                }
                None => {
                    let last_error = "cache/spool miss: bytes not available";
                    warn!(
                        representation_id = %rep_id,
                        cache_hit = false,
                        "Bytes missing in cache and spool; returning representation to Staged"
                    );
                    match self
                        .repo
                        .update_processing_result(
                            rep_id,
                            &[PayloadAvailability::Processing],
                            None,
                            PayloadAvailability::Staged,
                            Some(last_error),
                        )
                        .await
                    {
                        Ok(ProcessingUpdateOutcome::Updated(_)) => {}
                        Ok(ProcessingUpdateOutcome::StateMismatch) => {
                            warn!(
                                representation_id = %rep_id,
                                "Skipping revert to Staged due to state mismatch"
                            );
                        }
                        Ok(ProcessingUpdateOutcome::NotFound) => {
                            warn!(representation_id = %rep_id, "Representation missing");
                        }
                        Err(err) => {
                            warn!(
                                representation_id = %rep_id,
                                error = %err,
                                "Failed to revert representation to Staged after cache/spool miss"
                            );
                        }
                    }
                    return Ok(ProcessResult::MissingBytes);
                }
            }
        };

        // Check if this representation needs format conversion (e.g. TIFF -> PNG).
        // Fetch the representation metadata to read its MIME type.
        let rep_meta = self.repo.get_representation_by_id(rep_id).await?;
        let mime = rep_meta.as_ref().and_then(|r| r.mime_type.as_ref());
        let needs_conversion = should_convert_to_png(mime);

        // Pre-decoded RGBA pixels from format conversion, used to skip
        // re-decoding in thumbnail generation.
        let mut pre_decoded_rgba: Option<(Vec<u8>, u32, u32)> = None;

        let (blob_bytes, mime_updated) = if needs_conversion {
            let original_mime = mime.map(|m| m.as_str()).unwrap_or("unknown");
            let original_size = raw_bytes.len();
            match convert_image_to_png(&raw_bytes) {
                Ok(converted) => {
                    debug!(
                        representation_id = %rep_id,
                        original_mime = %original_mime,
                        original_size = original_size,
                        converted_size = converted.png_bytes.len(),
                        "Converted image to PNG for blob storage"
                    );
                    pre_decoded_rgba =
                        Some((converted.rgba_bytes, converted.width, converted.height));
                    (converted.png_bytes, true)
                }
                Err(err) => {
                    warn!(
                        representation_id = %rep_id,
                        original_mime = %original_mime,
                        error = %err,
                        "Failed to convert image to PNG; storing original bytes"
                    );
                    (raw_bytes, false)
                }
            }
        } else {
            (raw_bytes, false)
        };

        let content_hash = self
            .hasher
            .hash_bytes(&blob_bytes)
            .context("failed to hash representation bytes")?;

        // BlobWriterPort should handle deduplication; data is passed as-is.
        let blob = self
            .blob_writer
            .write_if_absent(&content_hash, &blob_bytes)
            .await
            .context("failed to write blob")?;

        // Update the representation MIME type in DB if conversion occurred.
        if mime_updated {
            if let Err(err) = self
                .repo
                .update_mime_type(rep_id, &MimeType("image/png".to_string()))
                .await
            {
                warn!(
                    representation_id = %rep_id,
                    error = %err,
                    "Failed to update MIME type to image/png after conversion"
                );
            }
        }

        let updated = self
            .repo
            .update_processing_result(
                rep_id,
                &[PayloadAvailability::Processing],
                Some(&blob.blob_id),
                PayloadAvailability::BlobReady,
                None,
            )
            .await;

        match updated {
            Ok(ProcessingUpdateOutcome::Updated(_)) => {
                if let Err(err) = self.spool.delete(rep_id).await {
                    warn!(
                        representation_id = %rep_id,
                        error = %err,
                        "Failed to delete spool entry after blob materialization"
                    );
                }
                self.try_generate_thumbnail(rep_id, &blob_bytes, pre_decoded_rgba)
                    .await;
                Ok(ProcessResult::Completed)
            }
            Ok(ProcessingUpdateOutcome::StateMismatch) => {
                warn!(
                    representation_id = %rep_id,
                    "Skipping update due to state mismatch"
                );
                Ok(ProcessResult::Completed)
            }
            Ok(ProcessingUpdateOutcome::NotFound) => {
                warn!(representation_id = %rep_id, "Representation missing");
                Ok(ProcessResult::Completed)
            }
            Err(err) => {
                warn!(
                    representation_id = %rep_id,
                    error = %err,
                    "Failed to update representation state after blob write"
                );
                Err(err)
            }
        }
    }

    async fn mark_failed(&self, rep_id: &RepresentationId, last_error: &str) -> Result<()> {
        match self
            .repo
            .update_processing_result(
                rep_id,
                &[PayloadAvailability::Processing, PayloadAvailability::Staged],
                None,
                PayloadAvailability::Failed {
                    last_error: last_error.to_string(),
                },
                Some(last_error),
            )
            .await
        {
            Ok(ProcessingUpdateOutcome::Updated(_)) => {}
            Ok(ProcessingUpdateOutcome::StateMismatch) => {
                warn!(
                    representation_id = %rep_id,
                    "Skipping mark_failed due to state mismatch"
                );
            }
            Ok(ProcessingUpdateOutcome::NotFound) => {
                warn!(representation_id = %rep_id, "Representation missing");
            }
            Err(err) => {
                error!(
                    representation_id = %rep_id,
                    error = %err,
                    "Failed to mark representation as Failed"
                );
            }
        }
        Ok(())
    }

    async fn try_generate_thumbnail(
        &self,
        rep_id: &RepresentationId,
        raw_bytes: &[u8],
        pre_decoded_rgba: Option<(Vec<u8>, u32, u32)>,
    ) {
        if let Err(err) = self
            .generate_thumbnail(rep_id, raw_bytes, pre_decoded_rgba)
            .await
        {
            error!(
                representation_id = %rep_id,
                error = %err,
                "Failed to generate thumbnail"
            );
        }
    }

    async fn generate_thumbnail(
        &self,
        rep_id: &RepresentationId,
        raw_bytes: &[u8],
        pre_decoded_rgba: Option<(Vec<u8>, u32, u32)>,
    ) -> Result<()> {
        let rep = match self.repo.get_representation_by_id(rep_id).await? {
            Some(rep) => rep,
            None => {
                warn!(
                    representation_id = %rep_id,
                    "Representation missing while generating thumbnail"
                );
                return Ok(());
            }
        };

        if rep.inline_data.is_some() {
            return Ok(());
        }

        let is_image = rep
            .mime_type
            .as_ref()
            .map(|mime| mime.as_str().starts_with("image/"))
            .unwrap_or(false);
        if !is_image {
            return Ok(());
        }

        if self
            .thumbnail_repo
            .get_by_representation_id(rep_id)
            .await?
            .is_some()
        {
            return Ok(());
        }

        let generated = if let Some((rgba, width, height)) = pre_decoded_rgba {
            self.thumbnail_generator
                .generate_thumbnail_from_rgba(&rgba, width, height)
                .await
                .context("failed to generate thumbnail from pre-decoded RGBA")?
        } else {
            self.thumbnail_generator
                .generate_thumbnail(raw_bytes)
                .await
                .context("failed to generate thumbnail")?
        };

        let thumbnail_hash = self
            .hasher
            .hash_bytes(&generated.thumbnail_bytes)
            .context("failed to hash thumbnail bytes")?;

        let thumbnail_blob = self
            .blob_writer
            .write_if_absent(&thumbnail_hash, &generated.thumbnail_bytes)
            .await
            .context("failed to write thumbnail blob")?;

        let created_at_ms = TimestampMs::from_epoch_millis(self.clock.now_ms());
        let thumbnail_size_bytes = thumbnail_blob.size_bytes;
        let metadata = ThumbnailMetadata::new(
            rep_id.clone(),
            thumbnail_blob.blob_id,
            generated.thumbnail_mime_type,
            generated.original_width,
            generated.original_height,
            thumbnail_size_bytes,
            Some(created_at_ms),
        );
        self.thumbnail_repo
            .insert_thumbnail(&metadata)
            .await
            .context("failed to insert thumbnail metadata")?;

        Ok(())
    }
}

enum ProcessResult {
    Completed,
    MissingBytes,
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockall::mock;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use uc_core::blob::BlobStorageLocator;
    use uc_core::clipboard::{PersistedClipboardRepresentation, ThumbnailMetadata, TimestampMs};
    use uc_core::ids::{EventId, FormatId, RepresentationId};
    use uc_core::ports::clipboard::{
        ClipboardRepresentationRepositoryPort, GeneratedThumbnail, ProcessingUpdateOutcome,
        ThumbnailGeneratorPort, ThumbnailRepositoryPort,
    };
    use uc_core::ports::ClockPort;
    use uc_core::{Blob, BlobId, ContentHash, HashAlgorithm, MimeType};

    type BlobStore = Arc<Mutex<HashMap<ContentHash, Blob>>>;
    type RepresentationStore =
        Arc<Mutex<HashMap<RepresentationId, PersistedClipboardRepresentation>>>;
    type ThumbnailStore = Arc<Mutex<HashMap<RepresentationId, ThumbnailMetadata>>>;
    type CallCounter = Arc<Mutex<u32>>;

    mock! {
        Hasher {}

        impl ContentHashPort for Hasher {
            #[mockall::concretize]
            fn hash_bytes(&self, bytes: &[u8]) -> Result<ContentHash>;
        }
    }

    mock! {
        BlobWriter {}

        #[async_trait::async_trait]
        impl BlobWriterPort for BlobWriter {
            #[mockall::concretize]
            async fn write_if_absent(
                &self,
                content_id: &ContentHash,
                encrypted_bytes: &[u8],
            ) -> Result<Blob>;
        }
    }

    mock! {
        RepresentationRepo {}

        #[async_trait::async_trait]
        impl ClipboardRepresentationRepositoryPort for RepresentationRepo {
            async fn get_representation(
                &self,
                event_id: &EventId,
                representation_id: &RepresentationId,
            ) -> Result<Option<PersistedClipboardRepresentation>>;
            async fn get_representation_by_id(
                &self,
                representation_id: &RepresentationId,
            ) -> Result<Option<PersistedClipboardRepresentation>>;
            async fn get_representation_by_blob_id(
                &self,
                blob_id: &BlobId,
            ) -> Result<Option<PersistedClipboardRepresentation>>;
            async fn update_blob_id(
                &self,
                representation_id: &RepresentationId,
                blob_id: &BlobId,
            ) -> Result<()>;
            async fn update_blob_id_if_none(
                &self,
                representation_id: &RepresentationId,
                blob_id: &BlobId,
            ) -> Result<bool>;
            #[mockall::concretize]
            async fn update_processing_result(
                &self,
                rep_id: &RepresentationId,
                expected_states: &[PayloadAvailability],
                blob_id: Option<&BlobId>,
                new_state: PayloadAvailability,
                last_error: Option<&str>,
            ) -> Result<ProcessingUpdateOutcome>;
        }
    }

    mock! {
        ThumbnailRepo {}

        #[async_trait::async_trait]
        impl ThumbnailRepositoryPort for ThumbnailRepo {
            async fn get_by_representation_id(
                &self,
                representation_id: &RepresentationId,
            ) -> Result<Option<ThumbnailMetadata>>;
            async fn insert_thumbnail(&self, metadata: &ThumbnailMetadata) -> Result<()>;
        }
    }

    mock! {
        ThumbnailGenerator {}

        #[async_trait::async_trait]
        impl ThumbnailGeneratorPort for ThumbnailGenerator {
            #[mockall::concretize]
            async fn generate_thumbnail(&self, image_bytes: &[u8]) -> Result<GeneratedThumbnail>;
            #[mockall::concretize]
            async fn generate_thumbnail_from_rgba(
                &self,
                rgba_bytes: &[u8],
                width: u32,
                height: u32,
            ) -> Result<GeneratedThumbnail>;
        }
    }

    mock! {
        Clock {}

        impl ClockPort for Clock {
            fn now_ms(&self) -> i64;
        }
    }

    fn make_hasher() -> MockHasher {
        let mut hasher = MockHasher::new();
        hasher.expect_hash_bytes().returning(|bytes| {
            let hash = blake3::hash(bytes);
            Ok(ContentHash {
                alg: HashAlgorithm::Blake3V1,
                bytes: *hash.as_bytes(),
            })
        });
        hasher
    }

    fn make_blob_writer() -> (MockBlobWriter, BlobStore) {
        let blobs: BlobStore = Arc::new(Mutex::new(HashMap::new()));
        let mut writer = MockBlobWriter::new();
        let blobs_for_write = Arc::clone(&blobs);
        writer
            .expect_write_if_absent()
            .returning(move |content_id, encrypted_bytes| {
                let mut store = blobs_for_write.lock().expect("blob store poisoned");
                if let Some(existing) = store.get(content_id) {
                    return Ok(existing.clone());
                }
                let blob = Blob::new(
                    BlobId::new(),
                    BlobStorageLocator::new_local_fs(PathBuf::from("/tmp/mock")),
                    encrypted_bytes.len() as i64,
                    content_id.clone(),
                    0,
                    None,
                );
                store.insert(content_id.clone(), blob.clone());
                Ok(blob)
            });
        (writer, blobs)
    }

    fn make_flaky_blob_writer() -> MockBlobWriter {
        let attempts = Arc::new(Mutex::new(0u32));
        let mut writer = MockBlobWriter::new();
        writer.expect_write_if_absent().returning({
            let attempts = Arc::clone(&attempts);
            move |content_id, encrypted_bytes| {
                let mut attempt_count = attempts.lock().expect("attempt counter poisoned");
                *attempt_count += 1;
                if *attempt_count == 1 {
                    return Err(anyhow::anyhow!("transient error"));
                }
                Ok(Blob::new(
                    BlobId::new(),
                    BlobStorageLocator::new_local_fs(PathBuf::from("/tmp/mock")),
                    encrypted_bytes.len() as i64,
                    content_id.clone(),
                    0,
                    None,
                ))
            }
        });
        writer
    }

    fn make_representation_repo(
        reps: HashMap<RepresentationId, PersistedClipboardRepresentation>,
    ) -> (MockRepresentationRepo, RepresentationStore) {
        let store: RepresentationStore = Arc::new(Mutex::new(reps));
        let mut repo = MockRepresentationRepo::new();

        repo.expect_get_representation().returning(|_, _| Ok(None));
        {
            let store = Arc::clone(&store);
            repo.expect_get_representation_by_id()
                .returning(move |representation_id| {
                    let reps = store.lock().expect("representation store poisoned");
                    Ok(reps.get(representation_id).cloned())
                });
        }
        repo.expect_get_representation_by_blob_id()
            .returning(|_| Ok(None));
        repo.expect_update_blob_id().returning(|_, _| Ok(()));
        repo.expect_update_blob_id_if_none()
            .returning(|_, _| Ok(false));

        {
            let store = Arc::clone(&store);
            repo.expect_update_processing_result().returning(
                move |rep_id, expected_states, blob_id, new_state, last_error| {
                    let mut reps = store.lock().expect("representation store poisoned");
                    let current = match reps.get_mut(rep_id) {
                        Some(rep) => rep,
                        None => return Ok(ProcessingUpdateOutcome::NotFound),
                    };

                    let expected_state_strs: Vec<&str> =
                        expected_states.iter().map(|s| s.as_str()).collect();
                    if !expected_state_strs.contains(&current.payload_state.as_str()) {
                        return Ok(ProcessingUpdateOutcome::StateMismatch);
                    }

                    current.payload_state = new_state.clone();
                    current.last_error = last_error.map(|value| value.to_string());

                    if let Some(blob_id) = blob_id {
                        current.blob_id = Some(blob_id.clone());
                    }

                    Ok(ProcessingUpdateOutcome::Updated(current.clone()))
                },
            );
        }

        (repo, store)
    }

    fn clone_thumbnail_metadata(metadata: &ThumbnailMetadata) -> ThumbnailMetadata {
        ThumbnailMetadata::new(
            metadata.representation_id.clone(),
            metadata.thumbnail_blob_id.clone(),
            metadata.thumbnail_mime_type.clone(),
            metadata.original_width,
            metadata.original_height,
            metadata.original_size_bytes,
            metadata.created_at_ms,
        )
    }

    fn make_thumbnail_repo() -> (MockThumbnailRepo, ThumbnailStore) {
        let store: ThumbnailStore = Arc::new(Mutex::new(HashMap::new()));
        let mut repo = MockThumbnailRepo::new();
        {
            let store = Arc::clone(&store);
            repo.expect_get_by_representation_id()
                .returning(move |representation_id| {
                    let thumbnails = store.lock().expect("thumbnail store poisoned");
                    Ok(thumbnails
                        .get(representation_id)
                        .map(clone_thumbnail_metadata))
                });
        }
        {
            let store = Arc::clone(&store);
            repo.expect_insert_thumbnail().returning(move |metadata| {
                let mut thumbnails = store.lock().expect("thumbnail store poisoned");
                thumbnails.insert(
                    metadata.representation_id.clone(),
                    clone_thumbnail_metadata(metadata),
                );
                Ok(())
            });
        }
        (repo, store)
    }

    fn clone_generated_thumbnail(generated: &GeneratedThumbnail) -> GeneratedThumbnail {
        GeneratedThumbnail {
            thumbnail_bytes: generated.thumbnail_bytes.clone(),
            thumbnail_mime_type: generated.thumbnail_mime_type.clone(),
            original_width: generated.original_width,
            original_height: generated.original_height,
        }
    }

    fn make_thumbnail_generator_success(
        generated: GeneratedThumbnail,
    ) -> (MockThumbnailGenerator, CallCounter) {
        let generated = Arc::new(generated);
        let calls: CallCounter = Arc::new(Mutex::new(0));
        let mut generator = MockThumbnailGenerator::new();
        {
            let generated = Arc::clone(&generated);
            let calls = Arc::clone(&calls);
            generator.expect_generate_thumbnail().returning(move |_| {
                let mut call_count = calls.lock().expect("call counter poisoned");
                *call_count += 1;
                Ok(clone_generated_thumbnail(&generated))
            });
        }
        {
            let generated = Arc::clone(&generated);
            let calls = Arc::clone(&calls);
            generator
                .expect_generate_thumbnail_from_rgba()
                .returning(move |_, _, _| {
                    let mut call_count = calls.lock().expect("call counter poisoned");
                    *call_count += 1;
                    Ok(clone_generated_thumbnail(&generated))
                });
        }
        (generator, calls)
    }

    fn make_thumbnail_generator_failure() -> (MockThumbnailGenerator, CallCounter) {
        let calls: CallCounter = Arc::new(Mutex::new(0));
        let mut generator = MockThumbnailGenerator::new();
        {
            let calls = Arc::clone(&calls);
            generator.expect_generate_thumbnail().returning(move |_| {
                let mut call_count = calls.lock().expect("call counter poisoned");
                *call_count += 1;
                Err(anyhow::anyhow!("thumbnail generator failed"))
            });
        }
        {
            let calls = Arc::clone(&calls);
            generator
                .expect_generate_thumbnail_from_rgba()
                .returning(move |_, _, _| {
                    let mut call_count = calls.lock().expect("call counter poisoned");
                    *call_count += 1;
                    Err(anyhow::anyhow!("thumbnail generator failed"))
                });
        }
        (generator, calls)
    }

    fn make_clock(now_ms: i64) -> MockClock {
        let mut clock = MockClock::new();
        clock.expect_now_ms().return_const(now_ms);
        clock
    }

    fn read_representation(
        store: &RepresentationStore,
        rep_id: &RepresentationId,
    ) -> Option<PersistedClipboardRepresentation> {
        let reps = store.lock().expect("representation store poisoned");
        reps.get(rep_id).cloned()
    }

    fn read_thumbnail(
        store: &ThumbnailStore,
        rep_id: &RepresentationId,
    ) -> Option<ThumbnailMetadata> {
        let thumbnails = store.lock().expect("thumbnail store poisoned");
        thumbnails.get(rep_id).map(clone_thumbnail_metadata)
    }

    fn read_call_count(counter: &CallCounter) -> u32 {
        *counter.lock().expect("call counter poisoned")
    }

    fn create_representation(rep_id: &RepresentationId) -> PersistedClipboardRepresentation {
        PersistedClipboardRepresentation::new_staged(
            rep_id.clone(),
            FormatId::new(),
            Some(MimeType("image/png".to_string())),
            1024,
        )
    }

    fn default_thumbnail_deps() -> (
        Arc<MockThumbnailRepo>,
        ThumbnailStore,
        Arc<MockThumbnailGenerator>,
        CallCounter,
        Arc<MockClock>,
    ) {
        let (repo, store) = make_thumbnail_repo();
        let (generator, calls) = make_thumbnail_generator_success(GeneratedThumbnail {
            thumbnail_bytes: vec![1, 2, 3],
            thumbnail_mime_type: MimeType("image/webp".to_string()),
            original_width: 1,
            original_height: 1,
        });
        let clock = make_clock(1);
        (
            Arc::new(repo),
            store,
            Arc::new(generator),
            calls,
            Arc::new(clock),
        )
    }

    #[tokio::test]
    async fn test_worker_generates_thumbnail() -> Result<()> {
        let rep_id = RepresentationId::new();
        let rep = create_representation(&rep_id);

        let mut reps = HashMap::new();
        reps.insert(rep_id.clone(), rep);

        let (repo, _) = make_representation_repo(reps);
        let repo = Arc::new(repo);
        let cache = Arc::new(RepresentationCache::new(10, 10_000));
        cache.put(&rep_id, vec![1, 2, 3, 4]).await;
        let spool = Arc::new(SpoolManager::new(tempfile::tempdir()?.path(), 10_000)?);
        let (blob_writer, _) = make_blob_writer();
        let blob_writer = Arc::new(blob_writer);
        let hasher = Arc::new(make_hasher());
        let (thumbnail_repo, thumbnail_store) = make_thumbnail_repo();
        let thumbnail_repo = Arc::new(thumbnail_repo);
        let thumbnail_bytes = vec![8, 9, 10];
        let (thumbnail_generator, thumbnail_calls) =
            make_thumbnail_generator_success(GeneratedThumbnail {
                thumbnail_bytes: thumbnail_bytes.clone(),
                thumbnail_mime_type: MimeType("image/webp".to_string()),
                original_width: 120,
                original_height: 80,
            });
        let thumbnail_generator = Arc::new(thumbnail_generator);
        let clock = Arc::new(make_clock(123));

        let (tx, rx) = mpsc::channel(4);
        let worker = BackgroundBlobWorker::new(
            rx,
            cache,
            spool,
            repo.clone(),
            blob_writer,
            hasher,
            thumbnail_repo.clone(),
            thumbnail_generator.clone(),
            clock,
            3,
            Duration::from_millis(1),
        );

        let handle = tokio::spawn(worker.run());
        tx.send(rep_id.clone()).await?;
        drop(tx);
        handle.await?;

        let thumbnail = read_thumbnail(&thumbnail_store, &rep_id).expect("thumbnail missing");
        assert_eq!(thumbnail.thumbnail_mime_type.as_str(), "image/webp");
        assert_eq!(thumbnail.original_width, 120);
        assert_eq!(thumbnail.original_height, 80);
        assert_eq!(thumbnail.original_size_bytes, thumbnail_bytes.len() as i64);
        assert_eq!(
            thumbnail.created_at_ms,
            Some(TimestampMs::from_epoch_millis(123))
        );
        assert_eq!(read_call_count(&thumbnail_calls), 1);
        Ok(())
    }

    #[tokio::test]
    async fn test_worker_skips_thumbnail_when_existing() -> Result<()> {
        let rep_id = RepresentationId::new();
        let rep = create_representation(&rep_id);

        let mut reps = HashMap::new();
        reps.insert(rep_id.clone(), rep);

        let (repo, _) = make_representation_repo(reps);
        let repo = Arc::new(repo);
        let cache = Arc::new(RepresentationCache::new(10, 10_000));
        cache.put(&rep_id, vec![1, 2, 3, 4]).await;
        let spool = Arc::new(SpoolManager::new(tempfile::tempdir()?.path(), 10_000)?);
        let (blob_writer, _) = make_blob_writer();
        let blob_writer = Arc::new(blob_writer);
        let hasher = Arc::new(make_hasher());
        let (thumbnail_repo, thumbnail_store) = make_thumbnail_repo();
        let existing = ThumbnailMetadata::new(
            rep_id.clone(),
            BlobId::new(),
            MimeType("image/webp".to_string()),
            120,
            80,
            1024,
            Some(TimestampMs::from_epoch_millis(1)),
        );
        {
            let mut thumbnails = thumbnail_store.lock().expect("thumbnail store poisoned");
            thumbnails.insert(rep_id.clone(), clone_thumbnail_metadata(&existing));
        }
        let thumbnail_repo = Arc::new(thumbnail_repo);
        let (thumbnail_generator, thumbnail_calls) =
            make_thumbnail_generator_success(GeneratedThumbnail {
                thumbnail_bytes: vec![8, 9, 10],
                thumbnail_mime_type: MimeType("image/webp".to_string()),
                original_width: 120,
                original_height: 80,
            });
        let thumbnail_generator = Arc::new(thumbnail_generator);
        let clock = Arc::new(make_clock(123));

        let (tx, rx) = mpsc::channel(4);
        let worker = BackgroundBlobWorker::new(
            rx,
            cache,
            spool,
            repo.clone(),
            blob_writer,
            hasher,
            thumbnail_repo.clone(),
            thumbnail_generator.clone(),
            clock,
            3,
            Duration::from_millis(1),
        );

        let handle = tokio::spawn(worker.run());
        tx.send(rep_id.clone()).await?;
        drop(tx);
        handle.await?;

        let thumbnail = read_thumbnail(&thumbnail_store, &rep_id).expect("thumbnail missing");
        assert_eq!(thumbnail.thumbnail_blob_id, existing.thumbnail_blob_id);
        assert_eq!(read_call_count(&thumbnail_calls), 0);
        Ok(())
    }

    #[tokio::test]
    async fn test_worker_does_not_insert_thumbnail_on_generator_failure() -> Result<()> {
        let rep_id = RepresentationId::new();
        let rep = create_representation(&rep_id);

        let mut reps = HashMap::new();
        reps.insert(rep_id.clone(), rep);

        let (repo, rep_store) = make_representation_repo(reps);
        let repo = Arc::new(repo);
        let cache = Arc::new(RepresentationCache::new(10, 10_000));
        cache.put(&rep_id, vec![1, 2, 3, 4]).await;
        let spool = Arc::new(SpoolManager::new(tempfile::tempdir()?.path(), 10_000)?);
        let (blob_writer, _) = make_blob_writer();
        let blob_writer = Arc::new(blob_writer);
        let hasher = Arc::new(make_hasher());
        let (thumbnail_repo, thumbnail_store) = make_thumbnail_repo();
        let thumbnail_repo = Arc::new(thumbnail_repo);
        let (thumbnail_generator, thumbnail_calls) = make_thumbnail_generator_failure();
        let thumbnail_generator = Arc::new(thumbnail_generator);
        let clock = Arc::new(make_clock(123));

        let (tx, rx) = mpsc::channel(4);
        let worker = BackgroundBlobWorker::new(
            rx,
            cache,
            spool,
            repo.clone(),
            blob_writer,
            hasher,
            thumbnail_repo.clone(),
            thumbnail_generator.clone(),
            clock,
            3,
            Duration::from_millis(1),
        );

        let handle = tokio::spawn(worker.run());
        tx.send(rep_id.clone()).await?;
        drop(tx);
        handle.await?;

        let thumbnail = read_thumbnail(&thumbnail_store, &rep_id);
        assert!(thumbnail.is_none());

        let updated = read_representation(&rep_store, &rep_id);
        let updated = updated.expect("representation missing");
        assert_eq!(updated.payload_state(), PayloadAvailability::BlobReady);
        assert_eq!(read_call_count(&thumbnail_calls), 1);
        Ok(())
    }

    #[tokio::test]
    async fn test_worker_processes_staged_representations() -> Result<()> {
        let rep_id = RepresentationId::new();
        let rep = create_representation(&rep_id);

        let mut reps = HashMap::new();
        reps.insert(rep_id.clone(), rep);

        let (repo, rep_store) = make_representation_repo(reps);
        let repo = Arc::new(repo);
        let cache = Arc::new(RepresentationCache::new(10, 10_000));
        cache.put(&rep_id, vec![1, 2, 3]).await;
        let spool = Arc::new(SpoolManager::new(tempfile::tempdir()?.path(), 10_000)?);
        let (blob_writer, _) = make_blob_writer();
        let blob_writer = Arc::new(blob_writer);
        let hasher = Arc::new(make_hasher());
        let (thumbnail_repo, _, thumbnail_generator, _, clock) = default_thumbnail_deps();

        let (tx, rx) = mpsc::channel(4);
        let worker = BackgroundBlobWorker::new(
            rx,
            cache,
            spool,
            repo.clone(),
            blob_writer,
            hasher,
            thumbnail_repo,
            thumbnail_generator,
            clock,
            3,
            Duration::from_millis(1),
        );

        let handle = tokio::spawn(worker.run());
        tx.send(rep_id.clone()).await?;
        drop(tx);
        handle.await?;

        let updated = read_representation(&rep_store, &rep_id);
        let updated = updated.expect("representation missing");
        assert_eq!(updated.payload_state(), PayloadAvailability::BlobReady);
        assert!(updated.blob_id.is_some());
        Ok(())
    }

    #[tokio::test]
    async fn test_worker_falls_back_to_spool() -> Result<()> {
        let rep_id = RepresentationId::new();
        let rep = create_representation(&rep_id);

        let mut reps = HashMap::new();
        reps.insert(rep_id.clone(), rep);

        let (repo, rep_store) = make_representation_repo(reps);
        let repo = Arc::new(repo);
        let cache = Arc::new(RepresentationCache::new(10, 10_000));
        let temp_dir = tempfile::tempdir()?;
        let spool = Arc::new(SpoolManager::new(temp_dir.path(), 10_000)?);
        spool.write(&rep_id, &[9, 9, 9]).await?;

        let (blob_writer, _) = make_blob_writer();
        let blob_writer = Arc::new(blob_writer);
        let hasher = Arc::new(make_hasher());
        let (thumbnail_repo, _, thumbnail_generator, _, clock) = default_thumbnail_deps();

        let (tx, rx) = mpsc::channel(4);
        let worker = BackgroundBlobWorker::new(
            rx,
            cache,
            spool,
            repo.clone(),
            blob_writer,
            hasher,
            thumbnail_repo,
            thumbnail_generator,
            clock,
            3,
            Duration::from_millis(1),
        );

        let handle = tokio::spawn(worker.run());
        tx.send(rep_id.clone()).await?;
        drop(tx);
        handle.await?;

        let updated = read_representation(&rep_store, &rep_id);
        let updated = updated.expect("representation missing");
        assert_eq!(updated.payload_state(), PayloadAvailability::BlobReady);
        Ok(())
    }

    #[tokio::test]
    async fn test_worker_does_not_mark_lost_on_cache_miss() -> Result<()> {
        let rep_id = RepresentationId::new();
        let rep = create_representation(&rep_id);

        let mut reps = HashMap::new();
        reps.insert(rep_id.clone(), rep);

        let (repo, rep_store) = make_representation_repo(reps);
        let repo = Arc::new(repo);
        let cache = Arc::new(RepresentationCache::new(10, 10_000));
        let spool = Arc::new(SpoolManager::new(tempfile::tempdir()?.path(), 10_000)?);
        let (blob_writer, _) = make_blob_writer();
        let blob_writer = Arc::new(blob_writer);
        let hasher = Arc::new(make_hasher());
        let (thumbnail_repo, _, thumbnail_generator, _, clock) = default_thumbnail_deps();

        let (tx, rx) = mpsc::channel(4);
        let worker = BackgroundBlobWorker::new(
            rx,
            cache,
            spool,
            repo.clone(),
            blob_writer,
            hasher,
            thumbnail_repo,
            thumbnail_generator,
            clock,
            3,
            Duration::from_millis(1),
        );

        let handle = tokio::spawn(worker.run());
        tx.send(rep_id.clone()).await?;
        drop(tx);
        handle.await?;

        let updated = read_representation(&rep_store, &rep_id);
        let updated = updated.expect("representation missing");
        assert_eq!(updated.payload_state(), PayloadAvailability::Staged);
        assert_eq!(
            updated.last_error.as_deref(),
            Some("cache/spool miss: bytes not available")
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_worker_retries_on_transient_error() -> Result<()> {
        let rep_id = RepresentationId::new();
        let rep = create_representation(&rep_id);

        let mut reps = HashMap::new();
        reps.insert(rep_id.clone(), rep);

        let (repo, rep_store) = make_representation_repo(reps);
        let repo = Arc::new(repo);
        let cache = Arc::new(RepresentationCache::new(10, 10_000));
        cache.put(&rep_id, vec![7, 7, 7]).await;
        let spool = Arc::new(SpoolManager::new(tempfile::tempdir()?.path(), 10_000)?);
        let blob_writer = Arc::new(make_flaky_blob_writer());
        let hasher = Arc::new(make_hasher());
        let (thumbnail_repo, _, thumbnail_generator, _, clock) = default_thumbnail_deps();

        let (tx, rx) = mpsc::channel(4);
        let worker = BackgroundBlobWorker::new(
            rx,
            cache,
            spool,
            repo.clone(),
            blob_writer,
            hasher,
            thumbnail_repo,
            thumbnail_generator,
            clock,
            2,
            Duration::from_millis(1),
        );

        let handle = tokio::spawn(worker.run());
        tx.send(rep_id.clone()).await?;
        drop(tx);
        handle.await?;

        let updated = read_representation(&rep_store, &rep_id);
        let updated = updated.expect("representation missing");
        assert_eq!(updated.payload_state(), PayloadAvailability::BlobReady);
        Ok(())
    }

    // --- Tests for should_convert_to_png and convert_image_to_png ---

    #[test]
    fn test_should_convert_to_png_tiff() {
        let mime = MimeType("image/tiff".to_string());
        assert!(should_convert_to_png(Some(&mime)));
    }

    #[test]
    fn test_should_convert_to_png_false_for_png() {
        let mime = MimeType("image/png".to_string());
        assert!(!should_convert_to_png(Some(&mime)));
    }

    #[test]
    fn test_should_convert_to_png_false_for_text() {
        let mime = MimeType("text/plain".to_string());
        assert!(!should_convert_to_png(Some(&mime)));
    }

    #[test]
    fn test_should_convert_to_png_false_for_webp() {
        let mime = MimeType("image/webp".to_string());
        assert!(!should_convert_to_png(Some(&mime)));
    }

    #[test]
    fn test_should_convert_to_png_false_for_none() {
        assert!(!should_convert_to_png(None));
    }

    #[test]
    fn test_convert_image_to_png_with_valid_png_bytes() {
        // Create a minimal valid PNG (1x1 red pixel)
        let img = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]));
        let mut png_input = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut png_input),
            image::ImageFormat::Png,
        )
        .unwrap();

        let result = convert_image_to_png(&png_input);
        assert!(result.is_ok());
        let converted = result.unwrap();
        // PNG magic bytes: 0x89 0x50 0x4E 0x47
        assert_eq!(&converted.png_bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
        assert_eq!(converted.width, 1);
        assert_eq!(converted.height, 1);
        assert_eq!(converted.rgba_bytes.len(), 4); // 1x1 RGBA
    }

    #[test]
    fn test_convert_image_to_png_with_valid_tiff_bytes() {
        // Create a minimal TIFF image in memory
        let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 128, 255, 255]));
        let mut tiff_bytes = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut tiff_bytes),
            image::ImageFormat::Tiff,
        )
        .unwrap();

        let result = convert_image_to_png(&tiff_bytes);
        assert!(result.is_ok());
        let converted = result.unwrap();
        // Verify PNG magic bytes
        assert_eq!(&converted.png_bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
        assert_eq!(converted.width, 2);
        assert_eq!(converted.height, 2);
    }

    #[test]
    fn test_convert_image_to_png_with_invalid_bytes() {
        let result = convert_image_to_png(&[0x00, 0x01, 0x02, 0x03]);
        assert!(result.is_err());
    }
}
