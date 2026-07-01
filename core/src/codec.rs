use crate::types::{MediaCodec, MediaKind};

/// Mapping from file extension to codec information.
pub fn codec_from_extension(ext: &str) -> (MediaKind, MediaCodec) {
    match ext.to_lowercase().as_str() {
        "jpg" | "jpeg" => (MediaKind::Image, MediaCodec::Jpeg),
        "png" => (MediaKind::Image, MediaCodec::Png),
        "webp" => (MediaKind::Image, MediaCodec::WebP),
        "heic" | "heif" | "hif" => (MediaKind::Image, MediaCodec::Heic),
        "avif" => (MediaKind::Image, MediaCodec::Avif),
        "tiff" | "tif" => (MediaKind::Image, MediaCodec::Tiff),
        "gif" => (MediaKind::Image, MediaCodec::Gif),
        "bmp" => (MediaKind::Image, MediaCodec::Bmp),
        "jxl" => (MediaKind::Image, MediaCodec::Jxl),
        "mp4" | "m4v" => (MediaKind::Video, MediaCodec::H264),
        "mov" => (MediaKind::Video, MediaCodec::H264),
        "mkv" => (MediaKind::Video, MediaCodec::H265),
        "webm" => (MediaKind::Video, MediaCodec::Vp9),
        "avi" => (MediaKind::Video, MediaCodec::Unknown),
        _ => (MediaKind::Image, MediaCodec::Unknown),
    }
}

/// Mapping from MIME type to codec information.
pub fn codec_from_mime(mime: &str) -> (MediaKind, MediaCodec) {
    match mime {
        "image/jpeg" => (MediaKind::Image, MediaCodec::Jpeg),
        "image/png" => (MediaKind::Image, MediaCodec::Png),
        "image/webp" => (MediaKind::Image, MediaCodec::WebP),
        "image/heic" | "image/heif" => (MediaKind::Image, MediaCodec::Heic),
        "image/avif" => (MediaKind::Image, MediaCodec::Avif),
        "image/tiff" => (MediaKind::Image, MediaCodec::Tiff),
        "image/gif" => (MediaKind::Image, MediaCodec::Gif),
        "image/bmp" => (MediaKind::Image, MediaCodec::Bmp),
        "image/jxl" => (MediaKind::Image, MediaCodec::Jxl),
        "video/mp4" => (MediaKind::Video, MediaCodec::H264),
        "video/quicktime" => (MediaKind::Video, MediaCodec::H264),
        "video/x-matroska" => (MediaKind::Video, MediaCodec::H265),
        "video/webm" => (MediaKind::Video, MediaCodec::Vp9),
        "video/x-msvideo" => (MediaKind::Video, MediaCodec::Unknown),
        _ => (MediaKind::Image, MediaCodec::Unknown),
    }
}

/// Common MIME types for supported image formats.
pub const IMAGE_MIME_TYPES: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/webp",
    "image/heic",
    "image/heif",
    "image/avif",
    "image/tiff",
    "image/gif",
    "image/bmp",
];

/// Common MIME types for supported video formats.
pub const VIDEO_MIME_TYPES: &[&str] = &[
    "video/mp4",
    "video/quicktime",
    "video/x-matroska",
    "video/webm",
    "video/x-msvideo",
];
