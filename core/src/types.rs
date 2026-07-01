use serde::{Deserialize, Serialize};

/// The kind of media asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaKind {
    Image,
    Video,
}

/// A media codec identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaCodec {
    /// JPEG
    Jpeg,
    /// PNG
    Png,
    /// WebP
    WebP,
    /// HEIC / HEIF
    Heic,
    /// AVIF
    Avif,
    /// TIFF
    Tiff,
    /// GIF
    Gif,
    /// BMP
    Bmp,
    /// JPEG XL
    Jxl,
    /// H.264 / AVC
    H264,
    /// H.265 / HEVC
    H265,
    /// AV1
    Av1,
    /// VP9
    Vp9,
    /// Unknown
    Unknown,
}

/// The quality / compression target for encoding.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Quality {
    /// Lossless — bit-perfect preservation
    Lossless,
    /// Visually lossless — subjective quality indistinguishable from original
    VisualLossless,
    /// Balanced — good quality with reasonable compression
    Balanced,
    /// Maximum compression — smallest file size, may have visible artifacts
    MaxCompression,
}

/// A media asset discovered from a source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    /// Unique identifier within the source system.
    pub id: String,
    /// Original filename or display name.
    pub filename: String,
    /// The media kind.
    pub kind: MediaKind,
    /// Source-detected or inferred codec.
    pub codec: MediaCodec,
    /// MIME type from the source, if available.
    pub mime_type: Option<String>,
    /// Size in bytes.
    pub size_bytes: Option<u64>,
    /// Asset creation timestamp (from the source).
    pub created_at: Option<String>,
    /// Source checksum, if available (e.g. Immich SHA1 base64).
    pub checksum: Option<String>,
    /// Arbitrary source metadata to forward to the sink.
    pub metadata: serde_json::Value,
}

/// A transcoded asset ready for storage.
pub struct TranscodedAsset {
    /// The original asset this was transcoded from.
    pub original_id: String,
    /// The output codec.
    pub codec: MediaCodec,
    /// The output media kind.
    pub kind: MediaKind,
    /// Streaming byte reader for the encoded output.
    pub stream: Box<dyn Read + Send + Unpin + 'static>,
    /// Total byte count of the output.
    pub byte_count: u64,
    /// Original checksum for verification.
    pub original_checksum: Option<String>,
    /// Transcode timing and statistics.
    pub stats: TranscodeStats,
}

/// Statistics from a single transcode operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscodeStats {
    /// Time spent decoding the input (wall clock, seconds).
    pub decode_seconds: f64,
    /// Time spent encoding (wall clock, seconds).
    pub encode_seconds: f64,
    /// Total wall clock time (seconds).
    pub total_seconds: f64,
    /// Input byte count.
    pub input_bytes: u64,
    /// Output byte count.
    pub output_bytes: u64,
    /// Compression ratio (output / input).
    pub compression_ratio: f64,
    /// Encoder label.
    pub encoder: String,
}

/// Outcome of processing a single asset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AssetOutcome {
    /// Successfully transcoded and stored.
    Success {
        asset_id: String,
        new_id: String,
        stats: TranscodeStats,
    },
    /// Partially succeeded (original deleted, but some metadata copy failed).
    PartialSuccess {
        asset_id: String,
        new_id: String,
        stats: TranscodeStats,
        warning: String,
    },
    /// Skipped (already in target format, too small, or excluded by policy).
    Skipped { asset_id: String, reason: String },
    /// Failed with error.
    Failed { asset_id: String, error: String },
}

use std::io::Read;
