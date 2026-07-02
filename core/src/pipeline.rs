use async_trait::async_trait;
use futures::StreamExt;

use crate::error::Result;
use crate::sink::Sink;
use crate::source::Source;
use crate::transcoder::Transcoder;
use crate::types::AssetOutcome;

/// State persistence for resumable pipeline runs.
#[async_trait]
pub trait StateStore: Send + Sync {
    /// Mark an asset as completed with the given outcome.
    async fn record(&self, asset_id: &str, outcome: &AssetOutcome) -> Result<()>;

    /// Check if an asset has already been completed.
    async fn is_completed(&self, asset_id: &str) -> Result<bool>;

    /// Return all completed asset IDs.
    async fn completed_ids(&self) -> Result<Vec<String>>;
}

/// The main pipeline orchestrator.
///
/// Wires a `Source`, `Transcoder`, and `Sink` together with optional
/// state tracking for resumability.
pub struct Pipeline {
    source: Box<dyn Source>,
    transcoder: Box<dyn Transcoder>,
    sink: Box<dyn Sink>,
    state: Option<Box<dyn StateStore>>,
    dry_run: bool,
}

impl Pipeline {
    /// Create a new pipeline.
    pub fn new(
        source: Box<dyn Source>,
        transcoder: Box<dyn Transcoder>,
        sink: Box<dyn Sink>,
    ) -> Self {
        Self {
            source,
            transcoder,
            sink,
            state: None,
            dry_run: false,
        }
    }

    /// Attach a state store for resumability.
    pub fn with_state(mut self, state: Box<dyn StateStore>) -> Self {
        self.state = Some(state);
        self
    }

    /// Enable dry-run mode (discover + log only, no transcode or upload).
    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    /// Run the pipeline: discover, transcode, store.
    pub async fn run(
        &mut self,
        mut on_outcome: impl FnMut(AssetOutcome),
    ) -> Result<PipelineSummary> {
        let mut summary = PipelineSummary::default();
        let mut stream = self.source.discover().await?;

        while let Some(asset_result) = stream.next().await {
            let asset = asset_result?;

            // Skip if already completed (stateful resume)
            if let Some(ref state) = self.state {
                if state.is_completed(&asset.id).await.unwrap_or(false) {
                    summary.skipped += 1;
                    continue;
                }
            }

            // Check if the transcoder can handle this asset
            if !self.transcoder.can_handle(&asset) {
                let outcome = AssetOutcome::Skipped {
                    asset_id: asset.id.clone(),
                    reason: format!(
                        "codec {:?} not supported by {}",
                        asset.codec,
                        self.transcoder.label()
                    ),
                };
                summary.skipped += 1;
                on_outcome(outcome.clone());

                if let Some(ref state) = self.state {
                    let _ = state.record(&asset.id, &outcome).await;
                }
                continue;
            }

            if self.dry_run {
                let outcome = AssetOutcome::Skipped {
                    asset_id: asset.id.clone(),
                    reason: "dry run".into(),
                };
                summary.skipped += 1;
                on_outcome(outcome.clone());

                if let Some(ref state) = self.state {
                    let _ = state.record(&asset.id, &outcome).await;
                }
                continue;
            }

            // Open original bytes
            let input = match self.source.open_original(&asset).await {
                Ok(stream) => stream,
                Err(e) => {
                    let outcome = AssetOutcome::Failed {
                        asset_id: asset.id.clone(),
                        error: format!("failed to open original: {e}"),
                    };
                    summary.failed += 1;
                    on_outcome(outcome.clone());

                    if let Some(ref state) = self.state {
                        let _ = state.record(&asset.id, &outcome).await;
                    }
                    continue;
                }
            };

            // Transcode
            let mut transcoded = match self.transcoder.transcode(input, &asset).await {
                Ok(t) => t,
                Err(e) => {
                    let outcome = AssetOutcome::Failed {
                        asset_id: asset.id.clone(),
                        error: format!("transcode failed: {e}"),
                    };
                    summary.failed += 1;
                    on_outcome(outcome.clone());

                    if let Some(ref state) = self.state {
                        let _ = state.record(&asset.id, &outcome).await;
                    }
                    continue;
                }
            };

            // Store (via Sink::process which handles store + metadata + verify + delete)
            let outcome = self
                .sink
                .process(&asset, &mut transcoded)
                .await
                .unwrap_or_else(|e| AssetOutcome::Failed {
                    asset_id: asset.id.clone(),
                    error: format!("sink processing failed: {e}"),
                });

            match &outcome {
                AssetOutcome::Success { .. } => summary.succeeded += 1,
                AssetOutcome::PartialSuccess { .. } => summary.partial += 1,
                AssetOutcome::Skipped { .. } => summary.skipped += 1,
                AssetOutcome::Failed { .. } => summary.failed += 1,
            }

            on_outcome(outcome.clone());

            if let Some(ref state) = self.state {
                let _ = state.record(&asset.id, &outcome).await;
            }
        }

        Ok(summary)
    }
}

/// Summary of a pipeline run.
#[derive(Debug, Clone, Default)]
pub struct PipelineSummary {
    pub succeeded: u64,
    pub partial: u64,
    pub skipped: u64,
    pub failed: u64,
}

impl PipelineSummary {
    pub fn total(&self) -> u64 {
        self.succeeded + self.partial + self.skipped + self.failed
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use async_trait::async_trait;
    use futures::stream::BoxStream;

    use super::*;
    use crate::error::Result;
    use crate::sink::Sink;
    use crate::source::Source;
    use crate::transcoder::Transcoder;
    use crate::types::{
        Asset, MediaCodec, MediaKind, TranscodeStats, TranscodedAsset,
    };

    // ── Mock Source ──────────────────────────────────────────────

    struct MockSource {
        assets: Vec<Asset>,
        open_ok: bool,
    }

    impl MockSource {
        fn new(assets: Vec<Asset>) -> Self {
            Self {
                assets,
                open_ok: true,
            }
        }

        fn with_failures(assets: Vec<Asset>, open_ok: bool) -> Self {
            Self { assets, open_ok }
        }
    }

    #[async_trait]
    impl Source for MockSource {
        fn label(&self) -> &'static str {
            "mock"
        }

        async fn discover(&self) -> Result<BoxStream<'_, Result<Asset>>> {
            let items: Vec<Result<Asset>> = self
                .assets
                .iter()
                .map(|a| Ok(a.clone()))
                .collect();
            Ok(futures::stream::iter(items).boxed())
        }

        async fn open_original(
            &self,
            _asset: &Asset,
        ) -> Result<Box<dyn std::io::Read + Send + Unpin + 'static>> {
            if self.open_ok {
                Ok(Box::new(Cursor::new(b"mock-pixels".to_vec())))
            } else {
                Err(crate::error::PipelineError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "open failed",
                )))
            }
        }
    }

    // ── Mock Transcoder ──────────────────────────────────────────

    struct MockTranscoder {
        supported: &'static [MediaCodec],
        output_codec: MediaCodec,
        kind: MediaKind,
        should_fail: bool,
    }

    impl MockTranscoder {
        fn new(supported: &'static [MediaCodec], output: MediaCodec, kind: MediaKind) -> Self {
            Self {
                supported,
                output_codec: output,
                kind,
                should_fail: false,
            }
        }

        fn with_failure(mut self) -> Self {
            self.should_fail = true;
            self
        }
    }

    #[async_trait]
    impl Transcoder for MockTranscoder {
        fn label(&self) -> &'static str {
            "mock-transcoder"
        }

        fn kind(&self) -> MediaKind {
            self.kind
        }

        fn input_codecs(&self) -> &'static [MediaCodec] {
            self.supported
        }

        fn output_codec(&self) -> MediaCodec {
            self.output_codec
        }

        async fn transcode(
            &self,
            _input: Box<dyn std::io::Read + Send + Unpin + 'static>,
            asset: &Asset,
        ) -> Result<TranscodedAsset> {
            if self.should_fail {
                return Err(crate::error::PipelineError::Transcoder(
                    "mock error".into(),
                ));
            }
            Ok(TranscodedAsset {
                original_id: asset.id.clone(),
                codec: self.output_codec,
                kind: self.kind,
                stream: Box::new(Cursor::new(b"transcoded")),
                byte_count: 10,
                original_checksum: None,
                stats: TranscodeStats {
                    decode_seconds: 0.0,
                    encode_seconds: 0.1,
                    total_seconds: 0.1,
                    input_bytes: 10,
                    output_bytes: 10,
                    compression_ratio: 1.0,
                    encoder: "mock".into(),
                },
            })
        }
    }

    // ── Mock Sink ────────────────────────────────────────────────

    struct MockSink {
        store_fail: bool,
        outcomes: std::sync::Mutex<Vec<AssetOutcome>>,
    }

    impl MockSink {
        fn new() -> Self {
            Self {
                store_fail: false,
                outcomes: std::sync::Mutex::new(Vec::new()),
            }
        }

        #[allow(dead_code)]
        fn outcomes(&self) -> Vec<AssetOutcome> {
            self.outcomes.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl Sink for MockSink {
        fn label(&self) -> &'static str {
            "mock"
        }

        async fn store(
            &self,
            _original: &Asset,
            _transcoded: &mut TranscodedAsset,
        ) -> Result<String> {
            if self.store_fail {
                Err(crate::error::PipelineError::Sink(
                    "store failed".into(),
                ))
            } else {
                Ok("new-id".into())
            }
        }

        async fn copy_metadata(&self, _original: &Asset, _new_id: &str) -> Result<()> {
            Ok(())
        }

        async fn verify(&self, _new_id: &str) -> Result<bool> {
            Ok(true)
        }

        async fn delete_original(&self, _asset: &Asset) -> Result<()> {
            Ok(())
        }
    }

    // ── Mock State Store ─────────────────────────────────────────

    struct MockState {
        completed: std::sync::Mutex<Vec<String>>,
    }

    impl MockState {
        fn new() -> Self {
            Self {
                completed: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl StateStore for MockState {
        async fn record(&self, asset_id: &str, _outcome: &AssetOutcome) -> Result<()> {
            self.completed.lock().unwrap().push(asset_id.to_string());
            Ok(())
        }

        async fn is_completed(&self, asset_id: &str) -> Result<bool> {
            Ok(self.completed.lock().unwrap().contains(&asset_id.to_string()))
        }

        async fn completed_ids(&self) -> Result<Vec<String>> {
            Ok(self.completed.lock().unwrap().clone())
        }
    }

    fn make_asset(id: &str, codec: MediaCodec, kind: MediaKind) -> Asset {
        Asset {
            id: id.into(),
            filename: format!("{id}.bin"),
            kind,
            codec,
            mime_type: None,
            size_bytes: Some(10),
            created_at: None,
            checksum: None,
            metadata: serde_json::json!({}),
        }
    }

    // ── Tests ────────────────────────────────────────────────────

    #[test]
    fn summary_default_is_all_zeros() {
        let s = PipelineSummary::default();
        assert_eq!(s.total(), 0);
        assert_eq!((s.succeeded, s.partial, s.skipped, s.failed), (0, 0, 0, 0));
    }

    #[test]
    fn summary_total_sums_all_categories() {
        let s = PipelineSummary {
            succeeded: 5,
            partial: 2,
            skipped: 3,
            failed: 1,
        };
        assert_eq!(s.total(), 11);
    }

    #[tokio::test]
    async fn pipeline_skips_unsupported_codec() {
        let asset = make_asset("a1", MediaCodec::Jpeg, MediaKind::Image);
        let source = MockSource::new(vec![asset]);
        let transcoder = MockTranscoder::new(
            &[MediaCodec::Png],
            MediaCodec::Jxl,
            MediaKind::Image,
        );
        let sink = MockSink::new();
        let mut pipeline = Pipeline::new(
            Box::new(source),
            Box::new(transcoder),
            Box::new(sink),
        );

        let outcomes = std::sync::Mutex::new(Vec::new());
        let summary = pipeline
            .run(|o| {
                outcomes.lock().unwrap().push(o);
            })
            .await
            .unwrap();

        assert_eq!(summary.skipped, 1);
        assert_eq!(summary.total(), 1);
    }

    #[tokio::test]
    async fn dry_run_skips_all_assets() {
        let asset = make_asset("a1", MediaCodec::Jpeg, MediaKind::Image);
        let source = MockSource::new(vec![asset]);
        let transcoder = MockTranscoder::new(
            &[MediaCodec::Jpeg],
            MediaCodec::Jxl,
            MediaKind::Image,
        );
        let sink = MockSink::new();
        let mut pipeline = Pipeline::new(
            Box::new(source),
            Box::new(transcoder),
            Box::new(sink),
        )
        .with_dry_run(true);

        let summary = pipeline.run(|_| {}).await.unwrap();
        assert_eq!(summary.skipped, 1);
        assert_eq!(summary.succeeded, 0);
    }

    #[tokio::test]
    async fn pipeline_happy_path() {
        let asset = make_asset("a1", MediaCodec::Jpeg, MediaKind::Image);
        let source = MockSource::new(vec![asset]);
        let transcoder = MockTranscoder::new(
            &[MediaCodec::Jpeg],
            MediaCodec::Jxl,
            MediaKind::Image,
        );
        let sink = MockSink::new();
        let mut pipeline = Pipeline::new(
            Box::new(source),
            Box::new(transcoder),
            Box::new(sink),
        );

        let summary = pipeline.run(|_| {}).await.unwrap();
        assert_eq!(summary.succeeded, 1);
        assert_eq!(summary.failed, 0);
    }

    #[tokio::test]
    async fn pipeline_transcode_failure() {
        let asset = make_asset("a1", MediaCodec::Jpeg, MediaKind::Image);
        let source = MockSource::new(vec![asset]);
        let transcoder = MockTranscoder::new(
            &[MediaCodec::Jpeg],
            MediaCodec::Jxl,
            MediaKind::Image,
        )
        .with_failure();
        let sink = MockSink::new();
        let mut pipeline = Pipeline::new(
            Box::new(source),
            Box::new(transcoder),
            Box::new(sink),
        );

        let summary = pipeline.run(|_| {}).await.unwrap();
        assert_eq!(summary.failed, 1);
    }

    #[tokio::test]
    async fn pipeline_source_open_failure() {
        let asset = make_asset("a1", MediaCodec::Jpeg, MediaKind::Image);
        let source = MockSource::with_failures(vec![asset], false);
        let transcoder = MockTranscoder::new(
            &[MediaCodec::Jpeg],
            MediaCodec::Jxl,
            MediaKind::Image,
        );
        let sink = MockSink::new();
        let mut pipeline = Pipeline::new(
            Box::new(source),
            Box::new(transcoder),
            Box::new(sink),
        );

        let summary = pipeline.run(|_| {}).await.unwrap();
        assert_eq!(summary.failed, 1);
    }

    #[tokio::test]
    async fn state_store_prevents_reprocessing() {
        let assets = vec![
            make_asset("a1", MediaCodec::Jpeg, MediaKind::Image),
            make_asset("a2", MediaCodec::Jpeg, MediaKind::Image),
        ];
        let source = MockSource::new(assets);
        let transcoder = MockTranscoder::new(
            &[MediaCodec::Jpeg],
            MediaCodec::Jxl,
            MediaKind::Image,
        );
        let sink = MockSink::new();
        let state = MockState::new();

        // Mark a1 as already completed
        state
            .record(
                "a1",
                &AssetOutcome::Skipped {
                    asset_id: "a1".into(),
                    reason: "done".into(),
                },
            )
            .await
            .unwrap();

        let mut pipeline = Pipeline::new(
            Box::new(source),
            Box::new(transcoder),
            Box::new(sink),
        )
        .with_state(Box::new(state));

        let summary = pipeline.run(|_| {}).await.unwrap();
        // a1 skipped (already done), a2 succeeds
        assert_eq!(summary.skipped, 1);
        assert_eq!(summary.succeeded, 1);
    }

    #[tokio::test]
    async fn multiple_assets_all_succeed() {
        let assets = vec![
            make_asset("a1", MediaCodec::Jpeg, MediaKind::Image),
            make_asset("a2", MediaCodec::Jpeg, MediaKind::Image),
            make_asset("a3", MediaCodec::Jpeg, MediaKind::Image),
        ];
        let source = MockSource::new(assets);
        let transcoder = MockTranscoder::new(
            &[MediaCodec::Jpeg],
            MediaCodec::Jxl,
            MediaKind::Image,
        );
        let sink = MockSink::new();
        let mut pipeline = Pipeline::new(
            Box::new(source),
            Box::new(transcoder),
            Box::new(sink),
        );

        let summary = pipeline.run(|_| {}).await.unwrap();
        assert_eq!(summary.succeeded, 3);
        assert_eq!(summary.total(), 3);
    }
}
