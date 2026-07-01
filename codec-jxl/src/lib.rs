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
