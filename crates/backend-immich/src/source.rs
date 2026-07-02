use async_trait::async_trait;
use futures::stream::{BoxStream, StreamExt};
use std::io::Read;

use rs_immicher_oxide_core::codec as codec_helpers;
use rs_immicher_oxide_core::error::{PipelineError, Result};
use rs_immicher_oxide_core::source::Source;
use rs_immicher_oxide_core::types::{Asset, MediaCodec, MediaKind};

use crate::client::{AssetType, ImmichApiClient};
use crate::ImmichConfig;

/// Source that discovers and streams existing Immich library assets via the REST API.
pub struct ImmichSource {
    #[allow(dead_code)]
    config: ImmichConfig,
    client: ImmichApiClient,
    /// Filter to only IMAGE or VIDEO assets.
    asset_type_filter: Option<AssetType>,
    /// Optional filter: only assets taken after this ISO timestamp.
    taken_after: Option<String>,
    /// Target codec — assets already in this format are skipped.
    target_codec: MediaCodec,
}

impl ImmichSource {
    pub fn new(config: ImmichConfig, target_codec: MediaCodec) -> Self {
        let client = ImmichApiClient::new(&config);
        Self {
            config,
            client,
            asset_type_filter: None,
            taken_after: None,
            target_codec,
        }
    }

    /// Filter to only image assets.
    pub fn with_images_only(mut self) -> Self {
        self.asset_type_filter = Some(AssetType::Image);
        self
    }

    /// Filter to only video assets.
    pub fn with_videos_only(mut self) -> Self {
        self.asset_type_filter = Some(AssetType::Video);
        self
    }

    /// Only process assets taken after this ISO 8601 timestamp.
    pub fn with_taken_after(mut self, timestamp: &str) -> Self {
        self.taken_after = Some(timestamp.to_string());
        self
    }

    /// Map an Immich `AssetResponseDto` to our `Asset` type.
    fn map_asset(&self, dto: crate::client::AssetResponseDto) -> Asset {
        let mime = dto.original_mime_type.as_deref().unwrap_or("");
        let (kind, codec) = codec_helpers::codec_from_mime(mime);

        // If MIME didn't match, try extension
        let (kind, codec) = if codec == MediaCodec::Unknown {
            let ext = dto.original_file_name.rsplit('.').next().unwrap_or("");
            codec_helpers::codec_from_extension(ext)
        } else {
            (kind, codec)
        };

        // Override kind from the Immich type
        let kind = match dto.asset_type {
            AssetType::Image => MediaKind::Image,
            AssetType::Video => MediaKind::Video,
            _ => kind,
        };

        let size = dto
            .exif_info
            .as_ref()
            .and_then(|e| e.file_size_in_byte)
            .map(|s| s as u64);

        Asset {
            id: dto.id,
            filename: dto.original_file_name,
            kind,
            codec,
            mime_type: dto.original_mime_type,
            size_bytes: size,
            created_at: dto.file_created_at,
            checksum: dto.checksum,
            metadata: serde_json::json!({
                "is_favorite": dto.is_favorite,
                "is_archived": dto.is_archived,
                "file_modified_at": dto.file_modified_at,
            }),
        }
    }
}

#[async_trait]
impl Source for ImmichSource {
    fn label(&self) -> &'static str {
        "immich-api"
    }

    async fn discover(&self) -> Result<BoxStream<'_, Result<Asset>>> {
        // Fetch all assets with pagination
        let dtos = self
            .client
            .search_all_assets(self.asset_type_filter.clone(), self.taken_after.clone())
            .await
            .map_err(|e| PipelineError::Source(Box::new(e)))?;

        let target_codec = self.target_codec;

        // Convert to stream, filtering out assets already in the target codec
        let assets: Vec<_> = dtos
            .into_iter()
            .map(|dto| self.map_asset(dto.clone()))
            .filter(|a| a.codec != target_codec && target_codec != MediaCodec::Unknown)
            .map(Ok)
            .collect();

        let stream = futures::stream::iter(assets);

        Ok(stream.boxed())
    }

    async fn open_original(&self, asset: &Asset) -> Result<Box<dyn Read + Send + Unpin + 'static>> {
        let resp = self
            .client
            .download_original(&asset.id)
            .await
            .map_err(|e| PipelineError::Source(Box::new(e)))?;

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| PipelineError::Source(Box::new(e)))?
            .to_vec();

        Ok(Box::new(std::io::Cursor::new(bytes)))
    }
}
