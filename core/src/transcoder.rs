use async_trait::async_trait;

use crate::error::Result;
use crate::types::{MediaCodec, MediaKind, TranscodedAsset};

/// A transcoder that converts media from one codec to another.
///
/// Implementations wrap encoder libraries or subprocesses. The pipeline
/// calls `transcode()` with a readable byte stream and receives a
/// streaming `TranscodedAsset` in return.
#[async_trait]
pub trait Transcoder: Send + Sync {
    /// Human-readable label (e.g. "av1-svt-uhq", "jxl-lossless").
    fn label(&self) -> &'static str;

    /// The media kind this transcoder handles.
    fn kind(&self) -> MediaKind;

    /// Input codecs this transcoder can accept.
    fn input_codecs(&self) -> &'static [MediaCodec];

    /// The output codec produced.
    fn output_codec(&self) -> MediaCodec;

    /// Whether this transcoder can accept the given asset.
    fn can_handle(&self, asset: &crate::types::Asset) -> bool {
        asset.kind == self.kind()
            && self
                .input_codecs()
                .iter()
                .any(|c| *c == asset.codec || asset.codec == MediaCodec::Unknown)
    }

    /// Transcode a source byte stream into the target format.
    ///
    /// The input must be a valid media stream decodable by this transcoder.
    /// The output stream should begin yielding encoded bytes as soon as
    /// they are available (frame-by-frame for video, atomically for images).
    async fn transcode(
        &self,
        input: Box<dyn Read + Send + Unpin + 'static>,
        asset: &crate::types::Asset,
    ) -> Result<TranscodedAsset>;
}

use std::io::Read;
