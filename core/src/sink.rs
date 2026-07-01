use async_trait::async_trait;

use crate::error::Result;
use crate::types::{Asset, AssetOutcome, TranscodedAsset};

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

    /// Verify the new asset is accessible and intact.
    async fn verify(&self, new_id: &str) -> Result<bool>;

    /// Delete (trash) the original asset after successful transcode.
    async fn delete_original(&self, asset: &Asset) -> Result<()>;

    /// Process a completed asset through the entire sink lifecycle:
    /// store → copy_metadata → verify → delete_original.
    async fn process(
        &self,
        original: &Asset,
        transcoded: &mut TranscodedAsset,
    ) -> Result<AssetOutcome> {
        let new_id = self.store(original, transcoded).await?;

        if let Err(e) = self.copy_metadata(original, &new_id).await {
            return Ok(AssetOutcome::PartialSuccess {
                asset_id: original.id.clone(),
                new_id,
                stats: transcoded.stats.clone(),
                warning: format!("metadata copy failed: {e}"),
            });
        }

        if !self.verify(&new_id).await.unwrap_or(false) {
            return Ok(AssetOutcome::PartialSuccess {
                asset_id: original.id.clone(),
                new_id,
                stats: transcoded.stats.clone(),
                warning: "new asset verification failed".into(),
            });
        }

        if let Err(e) = self.delete_original(original).await {
            return Ok(AssetOutcome::PartialSuccess {
                asset_id: original.id.clone(),
                new_id,
                stats: transcoded.stats.clone(),
                warning: format!("original deletion failed: {e}"),
            });
        }

        Ok(AssetOutcome::Success {
            asset_id: original.id.clone(),
            new_id,
            stats: transcoded.stats.clone(),
        })
    }
}
