use async_trait::async_trait;
use std::io::{Cursor, Read};

use jxl_encoder::{LosslessConfig, LossyConfig, PixelLayout};
use rs_immicher_oxide_core::error::{PipelineError, Result};
use rs_immicher_oxide_core::transcoder::Transcoder;
use rs_immicher_oxide_core::types::{
    Asset, MediaCodec, MediaKind, TranscodeStats, TranscodedAsset,
};

/// JPEG XL transcoder — pure Rust, no subprocess.
pub struct JxlTranscoder {
    /// Butteraugli distance (0 = lossless, >0 = lossy).
    distance: f32,
    /// Encoder effort (1-10, higher = slower/better).
    effort: u8,
}

impl JxlTranscoder {
    /// Create a new JXL transcoder.
    ///
    /// - `distance = 0.0`: lossless (JPEG input uses coefficient-domain transcode)
    /// - `distance = 1.0`: visually lossless (default)
    pub fn new(distance: f32) -> Self {
        Self {
            distance,
            effort: 7,
        }
    }

    /// Set encoder effort (1-10). Default 7.
    pub fn with_effort(mut self, effort: u8) -> Self {
        self.effort = effort.clamp(1, 10);
        self
    }

    fn label_str(&self) -> &'static str {
        if self.distance == 0.0 {
            "jxl-lossless"
        } else {
            "jxl-visual-lossless"
        }
    }
}

/// Decode image bytes into raw RGB pixels using the `image` crate.
fn decode_rgb8(data: &[u8]) -> std::result::Result<(Vec<u8>, u32, u32), String> {
    let img = image::load_from_memory(data).map_err(|e| format!("image decode failed: {e}"))?;
    let w = img.width();
    let h = img.height();
    let rgb = img.into_rgb8();
    let pixels = rgb.into_raw();
    Ok((pixels, w, h))
}

#[async_trait]
impl Transcoder for JxlTranscoder {
    fn label(&self) -> &'static str {
        self.label_str()
    }

    fn kind(&self) -> MediaKind {
        MediaKind::Image
    }

    fn input_codecs(&self) -> &'static [MediaCodec] {
        &[
            MediaCodec::Jpeg,
            MediaCodec::Png,
            MediaCodec::WebP,
            MediaCodec::Heic,
            MediaCodec::Avif,
            MediaCodec::Tiff,
            MediaCodec::Gif,
            MediaCodec::Bmp,
        ]
    }

    fn output_codec(&self) -> MediaCodec {
        MediaCodec::Jxl
    }

    async fn transcode(
        &self,
        input: Box<dyn Read + Send + Unpin + 'static>,
        asset: &Asset,
    ) -> Result<TranscodedAsset> {
        let start = std::time::Instant::now();

        let mut data = Vec::with_capacity(asset.size_bytes.unwrap_or(8192) as usize);
        let mut reader = input;
        reader.read_to_end(&mut data).map_err(PipelineError::Io)?;
        let input_bytes = data.len() as u64;

        let jxl_bytes = self.encode_pixels(&data)?;

        let output_bytes = jxl_bytes.len() as u64;
        let elapsed = start.elapsed();

        Ok(TranscodedAsset {
            original_id: asset.id.clone(),
            codec: MediaCodec::Jxl,
            kind: MediaKind::Image,
            stream: Box::new(Cursor::new(jxl_bytes)),
            byte_count: output_bytes,
            original_checksum: asset.checksum.clone(),
            stats: TranscodeStats {
                decode_seconds: 0.0,
                encode_seconds: elapsed.as_secs_f64(),
                total_seconds: elapsed.as_secs_f64(),
                input_bytes,
                output_bytes,
                compression_ratio: if input_bytes > 0 {
                    output_bytes as f64 / input_bytes as f64
                } else {
                    1.0
                },
                encoder: self.label().to_string(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageEncoder;
    use jxl_encoder::LossyConfig;

    use serde_json;

    /// JXL magic bytes: FF 0A (ISO media format, file type box "JXL ")
    const JXL_MAGIC: &[u8] = &[0xFF, 0x0A];

    /// Generate a 4×4 RGB test pattern.
    fn test_rgb_pixels() -> (Vec<u8>, u32, u32) {
        let w = 4u32;
        let h = 4u32;
        // 4x4 pixels: red, green, blue, white repeated
        let mut pixels = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                let r = if x % 2 == 0 { 255 } else { 0 };
                let g = if y % 2 == 0 { 255 } else { 0 };
                let b = if (x + y) % 2 == 0 { 255 } else { 0 };
                pixels.push(r);
                pixels.push(g);
                pixels.push(b);
            }
        }
        (pixels, w, h)
    }

    /// Create an in-memory PNG from raw RGB pixels.
    fn rgb_to_png_bytes(pixels: &[u8], w: u32, h: u32) -> Vec<u8> {
        use std::io::Cursor;
        let mut buf = Cursor::new(Vec::new());
        let encoder = image::codecs::png::PngEncoder::new(&mut buf);
        encoder
            .write_image(pixels, w, h, image::ExtendedColorType::Rgb8)
            .unwrap();
        buf.into_inner()
    }

    #[test]
    fn encode_lossy_jxl_has_correct_magic() {
        let (pixels, w, h) = test_rgb_pixels();
        let jxl = LossyConfig::new(1.0)
            .with_effort(1)
            .encode(&pixels, w, h, PixelLayout::Rgb8)
            .expect("JXL lossy encode should succeed");
        assert!(!jxl.is_empty(), "JXL output should not be empty");
        assert_eq!(&jxl[..2], JXL_MAGIC, "JXL should start with magic bytes");
    }

    #[test]
    fn encode_lossless_jxl_has_correct_magic() {
        let (pixels, w, h) = test_rgb_pixels();
        let jxl = LosslessConfig::new()
            .with_effort(1)
            .encode(&pixels, w, h, PixelLayout::Rgb8)
            .expect("JXL lossless encode should succeed");
        assert!(!jxl.is_empty(), "JXL output should not be empty");
        assert_eq!(&jxl[..2], JXL_MAGIC, "JXL should start with magic bytes");
    }

    #[test]
    fn decode_rgb8_roundtrips_png() {
        let (orig_pixels, w, h) = test_rgb_pixels();
        let png_bytes = rgb_to_png_bytes(&orig_pixels, w, h);

        let (decoded_pixels, dw, dh) =
            decode_rgb8(&png_bytes).expect("PNG decode should succeed");

        assert_eq!(dw, w, "decoded width should match");
        assert_eq!(dh, h, "decoded height should match");
        assert_eq!(decoded_pixels.len(), orig_pixels.len(), "pixel count should match");
        assert_eq!(decoded_pixels, orig_pixels, "pixel data should be identical");
    }

    #[test]
    fn jxl_transcoder_lossy_produces_valid_output() {
        let (pixels, w, h) = test_rgb_pixels();
        let png_bytes = rgb_to_png_bytes(&pixels, w, h);

        let asset = Asset {
            id: "test".into(),
            filename: "test.png".into(),
            kind: MediaKind::Image,
            codec: MediaCodec::Png,
            mime_type: Some("image/png".into()),
            size_bytes: Some(png_bytes.len() as u64),
            created_at: None,
            checksum: None,
            metadata: serde_json::json!({}),
        };

        let transcoder = JxlTranscoder::new(1.0).with_effort(1);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(async {
            transcoder
                .transcode(Box::new(Cursor::new(png_bytes)), &asset)
                .await
        });

        assert!(result.is_ok(), "transcode should succeed");
        let ta = result.unwrap();
        assert_eq!(ta.codec, MediaCodec::Jxl);
        assert!(ta.byte_count > 0, "output should have bytes");
        assert_eq!(&ta.stats.encoder, "jxl-visual-lossless");
    }

    #[test]
    fn jxl_transcoder_lossless_produces_valid_output() {
        let (pixels, w, h) = test_rgb_pixels();
        let png_bytes = rgb_to_png_bytes(&pixels, w, h);

        let asset = Asset {
            id: "test".into(),
            filename: "test.png".into(),
            kind: MediaKind::Image,
            codec: MediaCodec::Png,
            mime_type: Some("image/png".into()),
            size_bytes: Some(png_bytes.len() as u64),
            created_at: None,
            checksum: None,
            metadata: serde_json::json!({}),
        };

        let transcoder = JxlTranscoder::new(0.0).with_effort(1);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(async {
            transcoder
                .transcode(Box::new(Cursor::new(png_bytes)), &asset)
                .await
        });

        assert!(result.is_ok(), "lossless transcode should succeed");
        let ta = result.unwrap();
        assert_eq!(ta.codec, MediaCodec::Jxl);
        assert!(ta.byte_count > 0);
    }

    #[test]
    fn can_handle_checks_codec() {
        let transcoder = JxlTranscoder::new(1.0);
        let jpeg_asset = Asset {
            id: "j".into(),
            filename: "a.jpg".into(),
            kind: MediaKind::Image,
            codec: MediaCodec::Jpeg,
            mime_type: None,
            size_bytes: None,
            created_at: None,
            checksum: None,
            metadata: serde_json::json!({}),
        };
        assert!(transcoder.can_handle(&jpeg_asset), "should handle JPEG");

        let video_asset = Asset {
            id: "v".into(),
            filename: "a.mp4".into(),
            kind: MediaKind::Video,
            codec: MediaCodec::H264,
            mime_type: None,
            size_bytes: None,
            created_at: None,
            checksum: None,
            metadata: serde_json::json!({}),
        };
        assert!(!transcoder.can_handle(&video_asset), "should not handle video");
    }

    #[test]
    fn label_changes_with_distance() {
        let lossy = JxlTranscoder::new(1.0);
        assert_eq!(lossy.label_str(), "jxl-visual-lossless");

        let lossless = JxlTranscoder::new(0.0);
        assert_eq!(lossless.label_str(), "jxl-lossless");
    }
}

impl JxlTranscoder {
    fn encode_pixels(&self, data: &[u8]) -> Result<Vec<u8>> {
        let (pixels, w, h) = decode_rgb8(data)
            .map_err(|e| PipelineError::Transcoder(Box::new(std::io::Error::other(e))))?;

        if self.distance == 0.0 {
            LosslessConfig::new()
                .with_effort(self.effort)
                .encode(&pixels, w, h, PixelLayout::Rgb8)
                .map_err(|e| {
                    PipelineError::Transcoder(Box::new(std::io::Error::other(format!(
                        "JXL lossless encode failed: {e}"
                    ))))
                })
        } else {
            LossyConfig::new(self.distance)
                .with_effort(self.effort)
                .encode(&pixels, w, h, PixelLayout::Rgb8)
                .map_err(|e| {
                    PipelineError::Transcoder(Box::new(std::io::Error::other(format!(
                        "JXL lossy encode failed: {e}"
                    ))))
                })
        }
    }
}
