use async_trait::async_trait;
use std::io::Read;

use avio::{
    AudioCodec, EncoderConfig, HwAccel, Pipeline, Preset, VideoCodec, VideoCodecOptions,
};
use rs_immicher_oxide_core::error::{PipelineError, Result};
use rs_immicher_oxide_core::transcoder::Transcoder;
use rs_immicher_oxide_core::types::{
    Asset, MediaCodec, MediaKind, TranscodeStats, TranscodedAsset,
};

/// SVT-AV1 transcoder — pure Rust via avio Pipeline, no subprocess.
///
/// Uses `avio::Pipeline` for the full transcode pipeline:
/// - VAAPI hardware decode (AMD 7900 XTX on atlas)
/// - SVT-AV1 software encode
/// - Opus audio re-encode
/// - MP4 container with faststart
pub struct SvtAv1UhqTranscoder {
    /// CRF value (0-63, lower = better quality, default 20).
    crf: u32,
}

impl SvtAv1UhqTranscoder {
    /// Create a new SVT-AV1 UHQ transcoder.
    pub fn new(crf: u32) -> Self {
        Self { crf }
    }
}

#[async_trait]
impl Transcoder for SvtAv1UhqTranscoder {
    fn label(&self) -> &'static str {
        "av1-svt-uhq"
    }

    fn kind(&self) -> MediaKind {
        MediaKind::Video
    }

    fn input_codecs(&self) -> &'static [MediaCodec] {
        &[
            MediaCodec::H264,
            MediaCodec::H265,
            MediaCodec::Av1,
            MediaCodec::Vp9,
        ]
    }

    fn output_codec(&self) -> MediaCodec {
        MediaCodec::Av1
    }

    async fn transcode(
        &self,
        mut input: Box<dyn Read + Send + Unpin + 'static>,
        asset: &Asset,
    ) -> Result<TranscodedAsset> {
        let start = std::time::Instant::now();

        // Write input to tmpfs temp file (Pipeline works with file paths)
        let tmp = tempfile::TempDir::with_prefix("rs-immich-av1-")
            .map_err(PipelineError::Io)?;
        let input_path = tmp.path().join("input");
        let output_path = tmp.path().join("output.mp4");

        let mut data = Vec::with_capacity(asset.size_bytes.unwrap_or(8_388_608) as usize);
        input.read_to_end(&mut data).map_err(PipelineError::Io)?;
        let input_bytes = data.len() as u64;
        std::fs::write(&input_path, &data).map_err(PipelineError::Io)?;
        drop(data);

        use avio::SvtAv1Options;

        // Build the encoder configuration with SVT-AV1 tune=3 (UHQ)
        let config = EncoderConfig::builder()
            .video_codec(VideoCodec::Av1Svt)
            .audio_codec(AudioCodec::Opus)
            .crf(self.crf)
            .preset(Preset::Slow)
            .codec_options(VideoCodecOptions::Av1Svt(SvtAv1Options {
                preset: 6,
                tile_rows: 1,
                tile_cols: 2,
                svtav1_params: Some("tune=3:enable-overlays=1".into()),
            }))
            .hardware(HwAccel::Vaapi)
            .build();

        // Run the Pipeline — pure Rust, no subprocess
        // Pipeline::run() is synchronous (FFmpeg-native decode→filter→encode loop).
        // It blocks the current thread, which is acceptable for long-lived
        // transcode operations running in a dedicated task.
        tracing::debug!("av1 pipeline: {} → {}", input_path.display(), output_path.display());

        Pipeline::builder()
            .input(input_path.to_str().unwrap())
            .output(output_path.to_str().unwrap(), config)
            .build()
            .map_err(|e| {
                PipelineError::Transcoder(
                    format!("avio pipeline build failed: {e}").into(),
                )
            })?
            .run()
            .map_err(|e| {
                PipelineError::Transcoder(
                    format!("avio pipeline run failed: {e}").into(),
                )
            })?;

        // Read output
        let output_bytes = std::fs::read(&output_path).map_err(PipelineError::Io)?;
        let byte_count = output_bytes.len() as u64;
        let elapsed = start.elapsed();

        // TempDir drops and cleans up tmpfs files here

        Ok(TranscodedAsset {
            original_id: asset.id.clone(),
            codec: MediaCodec::Av1,
            kind: MediaKind::Video,
            stream: Box::new(std::io::Cursor::new(output_bytes)),
            byte_count,
            original_checksum: asset.checksum.clone(),
            stats: TranscodeStats {
                decode_seconds: 0.0,
                encode_seconds: elapsed.as_secs_f64(),
                total_seconds: elapsed.as_secs_f64(),
                input_bytes,
                output_bytes: byte_count,
                compression_ratio: if input_bytes > 0 {
                    byte_count as f64 / input_bytes as f64
                } else {
                    1.0
                },
                encoder: self.label().to_string(),
            },
        })
    }
}
