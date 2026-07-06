use async_trait::async_trait;
use std::io::Read;

use rs_immicher_oxide_core::error::{PipelineError, Result};
use rs_immicher_oxide_core::sink::Sink;
use rs_immicher_oxide_core::types::{
    Asset, MediaCodec, MediaKind, TranscodedAsset, TranscodedPayload,
};

use crate::client::{AssetBulkUpdateDto, AssetMediaStatus, AssetType, ImmichApiClient};
use crate::ImmichConfig;

/// Sink that uploads transcoded assets to Immich, copies metadata,
/// and optionally deletes the original.
pub struct ImmichSink {
    #[allow(dead_code)]
    config: ImmichConfig,
    client: ImmichApiClient,
    /// Whether to force-delete originals (skip trash, permanent).
    force_delete: bool,
}

impl ImmichSink {
    pub fn new(config: ImmichConfig) -> Self {
        let client = ImmichApiClient::new(&config);
        Self {
            config,
            client,
            force_delete: false,
        }
    }

    /// Enable force-delete (bypasses Immich trash, permanent deletion).
    pub fn with_force_delete(mut self, force: bool) -> Self {
        self.force_delete = force;
        self
    }

    /// Build the output filename from the original by changing the extension.
    fn output_filename(original: &Asset) -> String {
        let base = match original.filename.rfind('.') {
            Some(pos) => &original.filename[..pos],
            None => &original.filename,
        };
        let ext = match original.kind {
            rs_immicher_oxide_core::types::MediaKind::Image => "jxl",
            rs_immicher_oxide_core::types::MediaKind::Video => "mp4",
        };
        format!("{base}.{ext}")
    }

    /// Determine if the original had favorite/archived state from metadata.
    fn extract_metadata(original: &Asset) -> (Option<bool>, Option<bool>) {
        let is_favorite = original
            .metadata
            .get("is_favorite")
            .and_then(|v| v.as_bool());
        let is_archived = original
            .metadata
            .get("is_archived")
            .and_then(|v| v.as_bool());
        (is_favorite, is_archived)
    }
}

#[async_trait]
impl Sink for ImmichSink {
    fn label(&self) -> &'static str {
        "immich-api"
    }

    async fn store(&self, original: &Asset, transcoded: &mut TranscodedAsset) -> Result<String> {
        let filename = Self::output_filename(original);

        // Use original timestamps for the upload
        let file_created_at = original
            .created_at
            .as_deref()
            .unwrap_or("2024-01-01T00:00:00.000Z");
        let (is_favorite, _) = Self::extract_metadata(original);
        let file_modified_at = original
            .metadata
            .get("file_modified_at")
            .and_then(|v| v.as_str())
            .unwrap_or(file_created_at);

        let resp = match &mut transcoded.payload {
            TranscodedPayload::Reader(stream) => {
                let mut data = Vec::with_capacity(transcoded.byte_count as usize);
                stream.read_to_end(&mut data).map_err(PipelineError::Io)?;
                self.client
                    .upload_asset(
                        data,
                        file_created_at,
                        file_modified_at,
                        &filename,
                        is_favorite.unwrap_or(false),
                    )
                    .await
            }
            TranscodedPayload::File { path, .. } => {
                self.client
                    .upload_asset_file(
                        path,
                        transcoded.byte_count,
                        file_created_at,
                        file_modified_at,
                        &filename,
                        is_favorite.unwrap_or(false),
                    )
                    .await
            }
        }
        .map_err(|e| PipelineError::Sink(Box::new(e)))?;

        if resp.status == AssetMediaStatus::Duplicate {
            return Err(PipelineError::Sink(
                format!("Immich reported duplicate upload for replacement asset {filename}").into(),
            ));
        }

        Ok(resp.id)
    }

    async fn copy_metadata(&self, original: &Asset, new_id: &str) -> Result<()> {
        let (is_favorite, is_archived) = Self::extract_metadata(original);

        // 1. Bulk update: favorite + archive state
        if is_favorite.is_some() || is_archived.is_some() {
            self.client
                .bulk_update_assets(&AssetBulkUpdateDto {
                    ids: vec![new_id.to_string()],
                    is_favorite,
                    is_archived,
                    rating: None,
                })
                .await
                .map_err(|e| PipelineError::Sink(Box::new(e)))?;
        }

        // 2. Album membership: find albums containing the original, add new asset
        let albums = self
            .client
            .get_albums_for_asset(&original.id)
            .await
            .map_err(|e| PipelineError::Sink(Box::new(e)))?;

        if !albums.is_empty() {
            let album_ids: Vec<String> = albums.into_iter().map(|a| a.id).collect();
            self.client
                .add_assets_to_albums(&album_ids, &[new_id.to_string()])
                .await
                .map_err(|e| PipelineError::Sink(Box::new(e)))?;
        }

        Ok(())
    }

    async fn verify(
        &self,
        original: &Asset,
        output_codec: MediaCodec,
        byte_count: u64,
        new_id: &str,
    ) -> Result<()> {
        let info = self
            .client
            .get_asset_info(new_id)
            .await
            .map_err(|e| PipelineError::Sink(Box::new(e)))?;

        if info.id != new_id {
            return Err(PipelineError::Sink(
                format!("new asset id mismatch: expected {new_id}, got {}", info.id).into(),
            ));
        }

        let expected_type = match original.kind {
            MediaKind::Image => AssetType::Image,
            MediaKind::Video => AssetType::Video,
        };
        if info.asset_type != expected_type {
            return Err(PipelineError::Sink(
                format!(
                    "new asset type mismatch: expected {:?}, got {:?}",
                    expected_type, info.asset_type
                )
                .into(),
            ));
        }

        let expected_filename = Self::output_filename(original);
        if info.original_file_name != expected_filename {
            return Err(PipelineError::Sink(
                format!(
                    "new asset filename mismatch: expected {expected_filename}, got {}",
                    info.original_file_name
                )
                .into(),
            ));
        }

        if info.is_trashed.unwrap_or(false) {
            return Err(PipelineError::Sink("new asset is already trashed".into()));
        }

        if output_codec != transcoded_codec_for(original.kind) {
            return Err(PipelineError::Sink(
                format!(
                    "new asset codec mismatch: expected {:?}, got {:?}",
                    transcoded_codec_for(original.kind),
                    output_codec
                )
                .into(),
            ));
        }

        if byte_count == 0 {
            return Err(PipelineError::Sink("transcoded output is empty".into()));
        }

        if let Some(size) = info.exif_info.as_ref().and_then(|e| e.file_size_in_byte) {
            if size <= 0 {
                return Err(PipelineError::Sink(
                    format!("new asset reports non-positive size {size}").into(),
                ));
            }
        }

        let resp = self
            .client
            .download_original(new_id)
            .await
            .map_err(|e| PipelineError::Sink(Box::new(e)))?;
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| PipelineError::Sink(Box::new(e)))?;
        if bytes.is_empty() {
            return Err(PipelineError::Sink("new asset download is empty".into()));
        }

        Ok(())
    }

    async fn delete_original(&self, asset: &Asset) -> Result<()> {
        self.client
            .delete_assets(std::slice::from_ref(&asset.id), self.force_delete)
            .await
            .map_err(|e| PipelineError::Sink(Box::new(e)))?;
        Ok(())
    }
}

fn transcoded_codec_for(kind: MediaKind) -> MediaCodec {
    match kind {
        MediaKind::Image => MediaCodec::Jxl,
        MediaKind::Video => MediaCodec::Av1,
    }
}
