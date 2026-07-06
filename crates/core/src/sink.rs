use async_trait::async_trait;

use crate::error::Result;
use crate::types::{Asset, AssetOutcome, MediaCodec, TranscodedAsset, WriteMode};

/// A destination for transcoded assets.
///
/// Implementations store the transcoded result, copy metadata from the
/// original, and optionally delete the original.
#[async_trait]
pub trait Sink: Send + Sync {
    /// A human-readable label for this sink.
    fn label(&self) -> &'static str;

    /// Store a transcoded asset.
    ///
    /// Returns the new stable identifier assigned by the sink system.
    async fn store(&self, original: &Asset, transcoded: &mut TranscodedAsset) -> Result<String>;

    /// Copy metadata (favorite, album membership, etc.) from the original to the new asset.
    async fn copy_metadata(&self, original: &Asset, new_id: &str) -> Result<()>;

    /// Verify the new asset is accessible and matches the expected output.
    async fn verify(
        &self,
        original: &Asset,
        output_codec: MediaCodec,
        byte_count: u64,
        new_id: &str,
    ) -> Result<()>;

    /// Delete (trash) the original asset after successful transcode.
    async fn delete_original(&self, asset: &Asset) -> Result<()>;

    /// Process a completed asset through the selected sink lifecycle.
    async fn process(
        &self,
        original: &Asset,
        transcoded: &mut TranscodedAsset,
        write_mode: WriteMode,
    ) -> Result<AssetOutcome> {
        let new_id = self.store(original, transcoded).await?;

        if let Err(e) = self.copy_metadata(original, &new_id).await {
            return Ok(AssetOutcome::PartialSuccess {
                asset_id: original.id.clone(),
                new_id,
                original_codec: original.codec,
                output_codec: transcoded.codec,
                original_checksum: transcoded.original_checksum.clone(),
                stats: transcoded.stats.clone(),
                warning: format!("metadata copy failed: {e}"),
            });
        }

        if let Err(e) = self
            .verify(original, transcoded.codec, transcoded.byte_count, &new_id)
            .await
        {
            return Ok(AssetOutcome::PartialSuccess {
                asset_id: original.id.clone(),
                new_id,
                original_codec: original.codec,
                output_codec: transcoded.codec,
                original_checksum: transcoded.original_checksum.clone(),
                stats: transcoded.stats.clone(),
                warning: format!("new asset verification failed: {e}"),
            });
        }

        if matches!(write_mode, WriteMode::TrashOriginal) {
            if let Err(e) = self.delete_original(original).await {
                return Ok(AssetOutcome::PartialSuccess {
                    asset_id: original.id.clone(),
                    new_id,
                    original_codec: original.codec,
                    output_codec: transcoded.codec,
                    original_checksum: transcoded.original_checksum.clone(),
                    stats: transcoded.stats.clone(),
                    warning: format!("original deletion failed: {e}"),
                });
            }
        }

        Ok(AssetOutcome::Success {
            asset_id: original.id.clone(),
            new_id,
            original_codec: original.codec,
            output_codec: transcoded.codec,
            original_checksum: transcoded.original_checksum.clone(),
            stats: transcoded.stats.clone(),
        })
    }
}
