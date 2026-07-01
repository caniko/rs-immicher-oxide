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
