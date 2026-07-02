use reqwest::multipart;
use serde::{Deserialize, Serialize};

use super::ImmichConfig;

/// Immich-specific error.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("HTTP error: {status} {body}")]
    Http {
        status: reqwest::StatusCode,
        body: String,
    },
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("API returned error status: {0}")]
    Status(String),
}

type ApiResult<T> = std::result::Result<T, ApiError>;

// ── DTOs ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetResponseDto {
    pub id: String,
    pub checksum: Option<String>,
    #[serde(rename = "type")]
    pub asset_type: AssetType,
    pub original_file_name: String,
    pub original_mime_type: Option<String>,
    pub file_created_at: Option<String>,
    pub file_modified_at: Option<String>,
    pub is_favorite: Option<bool>,
    pub is_archived: Option<bool>,
    pub is_trashed: Option<bool>,
    pub exif_info: Option<ExifResponseDto>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExifResponseDto {
    pub file_size_in_byte: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum AssetType {
    Image,
    Video,
    Audio,
    Other,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetadataSearchDto {
    pub page: i32,
    pub size: i32,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub asset_type: Option<AssetType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taken_after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taken_before: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub with_deleted: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchResponseDto {
    pub assets: SearchAssetResponseDto,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchAssetResponseDto {
    pub items: Vec<AssetResponseDto>,
    pub next_page: Option<String>,
    pub total: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetMediaResponseDto {
    pub id: String,
    pub status: AssetMediaStatus,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssetMediaStatus {
    Created,
    Duplicate,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssetBulkUpdateDto {
    pub ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_favorite: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_archived: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BulkIdsDto {
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BulkIdResponseDto {
    pub id: String,
    pub success: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumResponseDto {
    pub id: String,
    pub album_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssetBulkDeleteDto {
    pub ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force: Option<bool>,
}

// ── Client ────────────────────────────────────────────────────────

/// Typed Immich REST API client.
///
/// Covers the subset of the Immich API needed for the transpiler:
/// search, download, upload, metadata copy, album management, and delete.
pub struct ImmichApiClient {
    base_url: String,
    client: reqwest::Client,
}

impl ImmichApiClient {
    pub fn new(config: &ImmichConfig) -> Self {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::HeaderName::from_static("x-api-key"),
            reqwest::header::HeaderValue::from_str(&config.api_key).unwrap(),
        );

        let client = reqwest::Client::builder()
            .user_agent("rs-immicher-oxide/0.1")
            .default_headers(headers)
            .build()
            .expect("failed to build reqwest client");

        Self {
            base_url: config.server_url.trim_end_matches('/').to_string(),
            client,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    async fn check_response(&self, resp: reqwest::Response) -> ApiResult<reqwest::Response> {
        let status = resp.status();
        if status.is_success() {
            Ok(resp)
        } else {
            let body = resp.text().await.unwrap_or_default();
            Err(ApiError::Http { status, body })
        }
    }

    // ── Asset discovery ───────────────────────────────────────────

    /// POST /search/metadata with paginated results.
    pub async fn search_assets(&self, dto: &MetadataSearchDto) -> ApiResult<SearchResponseDto> {
        let resp = self
            .client
            .post(self.url("/search/metadata"))
            .json(dto)
            .send()
            .await?;
        let resp = self.check_response(resp).await?;
        Ok(resp.json().await?)
    }

    /// Discover all assets of a given type, page by page.
    pub async fn search_all_assets(
        &self,
        asset_type: Option<AssetType>,
        taken_after: Option<String>,
    ) -> ApiResult<Vec<AssetResponseDto>> {
        let mut all = Vec::new();
        let mut page = 1;
        let size = 500;

        loop {
            let dto = MetadataSearchDto {
                page,
                size,
                asset_type: asset_type.clone(),
                taken_after: taken_after.clone(),
                taken_before: None,
                created_after: None,
                with_deleted: Some(false),
            };

            let result = self.search_assets(&dto).await?;
            all.extend(result.assets.items);

            match result.assets.next_page {
                Some(ref token) if !token.is_empty() => {
                    page = token.parse().unwrap_or(page + 1);
                }
                _ => break,
            }

            if result.assets.total.is_some_and(|t| all.len() >= t as usize) {
                break;
            }
        }

        Ok(all)
    }

    // ── Download ──────────────────────────────────────────────────

    /// GET /assets/{id}/original — returns a streaming response.
    pub async fn download_original(&self, asset_id: &str) -> ApiResult<reqwest::Response> {
        let resp = self
            .client
            .get(self.url(&format!("/assets/{asset_id}/original")))
            .send()
            .await?;
        self.check_response(resp).await
    }

    // ── Asset info ────────────────────────────────────────────────

    /// GET /assets/{id} — get full asset metadata.
    pub async fn get_asset_info(&self, asset_id: &str) -> ApiResult<AssetResponseDto> {
        let resp = self
            .client
            .get(self.url(&format!("/assets/{asset_id}")))
            .send()
            .await?;
        let resp = self.check_response(resp).await?;
        Ok(resp.json().await?)
    }

    // ── Upload ────────────────────────────────────────────────────

    /// POST /assets — upload a transcoded asset.
    pub async fn upload_asset(
        &self,
        asset_data: Vec<u8>,
        file_created_at: &str,
        file_modified_at: &str,
        filename: &str,
        is_favorite: bool,
    ) -> ApiResult<AssetMediaResponseDto> {
        let mime = if filename.ends_with(".mp4") || filename.ends_with(".jxl") {
            // We don't know exact MIME; let Immich detect from extension
            "application/octet-stream"
        } else {
            "application/octet-stream"
        };

        let part = multipart::Part::bytes(asset_data)
            .file_name(filename.to_string())
            .mime_str(mime)
            .map_err(|e| ApiError::Status(e.to_string()))?;

        let form = multipart::Form::new()
            .part("assetData", part)
            .text("fileCreatedAt", file_created_at.to_string())
            .text("fileModifiedAt", file_modified_at.to_string())
            .text("isFavorite", if is_favorite { "true" } else { "false" });

        let resp = self
            .client
            .post(self.url("/assets"))
            .multipart(form)
            .send()
            .await?;

        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();

        if status.is_success() {
            Ok(serde_json::from_str(&body)
                .map_err(|e| ApiError::Status(format!("parse upload response: {e}: {body}")))?)
        } else {
            Err(ApiError::Http { status, body })
        }
    }

    // ── Bulk update ───────────────────────────────────────────────

    /// PUT /assets — bulk update asset metadata.
    pub async fn bulk_update_assets(&self, dto: &AssetBulkUpdateDto) -> ApiResult<()> {
        let resp = self
            .client
            .put(self.url("/assets"))
            .json(dto)
            .send()
            .await?;
        self.check_response(resp).await?;
        Ok(())
    }

    // ── Albums ────────────────────────────────────────────────────

    /// GET /albums?assetId={id} — albums containing the given asset.
    pub async fn get_albums_for_asset(&self, asset_id: &str) -> ApiResult<Vec<AlbumResponseDto>> {
        let resp = self
            .client
            .get(self.url(&format!("/albums?assetId={asset_id}")))
            .send()
            .await?;
        let resp = self.check_response(resp).await?;
        Ok(resp.json().await?)
    }

    /// PUT /albums/{id}/assets — add assets to album.
    pub async fn add_assets_to_album(
        &self,
        album_id: &str,
        asset_ids: &[String],
    ) -> ApiResult<Vec<BulkIdResponseDto>> {
        let resp = self
            .client
            .put(self.url(&format!("/albums/{album_id}/assets")))
            .json(&BulkIdsDto {
                ids: asset_ids.to_vec(),
            })
            .send()
            .await?;
        let resp = self.check_response(resp).await?;
        Ok(resp.json().await?)
    }

    /// PUT /albums/assets — add assets to multiple albums at once.
    pub async fn add_assets_to_albums(
        &self,
        album_ids: &[String],
        asset_ids: &[String],
    ) -> ApiResult<()> {
        #[derive(Serialize)]
        struct AlbumsAddAssetsDto {
            album_ids: Vec<String>,
            asset_ids: Vec<String>,
        }

        let resp = self
            .client
            .put(self.url("/albums/assets"))
            .json(&AlbumsAddAssetsDto {
                album_ids: album_ids.to_vec(),
                asset_ids: asset_ids.to_vec(),
            })
            .send()
            .await?;
        self.check_response(resp).await?;
        Ok(())
    }

    // ── Delete ────────────────────────────────────────────────────

    /// DELETE /assets — trash assets.
    pub async fn delete_assets(&self, ids: &[String], force: bool) -> ApiResult<()> {
        let resp = self
            .client
            .delete(self.url("/assets"))
            .json(&AssetBulkDeleteDto {
                ids: ids.to_vec(),
                force: Some(force),
            })
            .send()
            .await?;
        self.check_response(resp).await?;
        Ok(())
    }
}
