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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_jpeg() {
        assert_eq!(
            codec_from_extension("jpg"),
            (MediaKind::Image, MediaCodec::Jpeg)
        );
        assert_eq!(
            codec_from_extension("jpeg"),
            (MediaKind::Image, MediaCodec::Jpeg)
        );
        assert_eq!(
            codec_from_extension("JPEG"),
            (MediaKind::Image, MediaCodec::Jpeg)
        );
    }

    #[test]
    fn extension_png() {
        assert_eq!(
            codec_from_extension("png"),
            (MediaKind::Image, MediaCodec::Png)
        );
    }

    #[test]
    fn extension_heic_variants() {
        assert_eq!(
            codec_from_extension("heic"),
            (MediaKind::Image, MediaCodec::Heic)
        );
        assert_eq!(
            codec_from_extension("heif"),
            (MediaKind::Image, MediaCodec::Heic)
        );
        assert_eq!(
            codec_from_extension("hif"),
            (MediaKind::Image, MediaCodec::Heic)
        );
    }

    #[test]
    fn extension_video_formats() {
        assert_eq!(
            codec_from_extension("mp4"),
            (MediaKind::Video, MediaCodec::H264)
        );
        assert_eq!(
            codec_from_extension("mov"),
            (MediaKind::Video, MediaCodec::H264)
        );
        assert_eq!(
            codec_from_extension("mkv"),
            (MediaKind::Video, MediaCodec::H265)
        );
        assert_eq!(
            codec_from_extension("webm"),
            (MediaKind::Video, MediaCodec::Vp9)
        );
        assert_eq!(
            codec_from_extension("avi"),
            (MediaKind::Video, MediaCodec::Unknown)
        );
    }

    #[test]
    fn extension_unknown_returns_image_unknown() {
        assert_eq!(
            codec_from_extension("txt"),
            (MediaKind::Image, MediaCodec::Unknown)
        );
        assert_eq!(
            codec_from_extension(""),
            (MediaKind::Image, MediaCodec::Unknown)
        );
    }

    #[test]
    fn mime_jpeg() {
        assert_eq!(
            codec_from_mime("image/jpeg"),
            (MediaKind::Image, MediaCodec::Jpeg)
        );
    }

    #[test]
    fn mime_video_formats() {
        assert_eq!(
            codec_from_mime("video/mp4"),
            (MediaKind::Video, MediaCodec::H264)
        );
        assert_eq!(
            codec_from_mime("video/quicktime"),
            (MediaKind::Video, MediaCodec::H264)
        );
        assert_eq!(
            codec_from_mime("video/webm"),
            (MediaKind::Video, MediaCodec::Vp9)
        );
    }

    #[test]
    fn mime_jxl() {
        assert_eq!(
            codec_from_mime("image/jxl"),
            (MediaKind::Image, MediaCodec::Jxl)
        );
    }

    #[test]
    fn mime_unknown() {
        assert_eq!(
            codec_from_mime("application/pdf"),
            (MediaKind::Image, MediaCodec::Unknown)
        );
    }

    #[test]
    fn image_mime_constants_are_comprehensive() {
        let images = [
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
        for mime in &images {
            assert!(IMAGE_MIME_TYPES.contains(mime), "missing {mime}");
        }
    }

    #[test]
    fn video_mime_constants_are_comprehensive() {
        let videos = [
            "video/mp4",
            "video/quicktime",
            "video/x-matroska",
            "video/webm",
            "video/x-msvideo",
        ];
        for mime in &videos {
            assert!(VIDEO_MIME_TYPES.contains(mime), "missing {mime}");
        }
    }

    #[test]
    fn roundtrip_mime_extension_consistency() {
        let cases = [
            ("jpg", "image/jpeg"),
            ("png", "image/png"),
            ("webp", "image/webp"),
            ("mp4", "video/mp4"),
            ("jxl", "image/jxl"),
        ];
        for (ext, mime) in &cases {
            let (k1, c1) = codec_from_extension(ext);
            let (k2, c2) = codec_from_mime(mime);
            assert_eq!(k1, k2, "kind mismatch for {ext}/{mime}");
            assert_eq!(c1, c2, "codec mismatch for {ext}/{mime}");
        }
    }
}
