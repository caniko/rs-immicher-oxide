use std::sync::Arc;
use std::time::Duration;

use clap::{Parser, Subcommand};

use rs_immicher_oxide_backend_immich::sink::ImmichSink;
use rs_immicher_oxide_backend_immich::source::ImmichSource;
use rs_immicher_oxide_backend_immich::ImmichConfig;
use rs_immicher_oxide_codec_av1::SvtAv1UhqTranscoder;
use rs_immicher_oxide_codec_jxl::JxlTranscoder;
use rs_immicher_oxide_core::pipeline::{Pipeline, PipelineSummary};
use rs_immicher_oxide_core::sink::Sink;
use rs_immicher_oxide_core::source::Source;
use rs_immicher_oxide_core::transcoder::Transcoder;
use rs_immicher_oxide_core::types::{AssetOutcome, MediaCodec};

#[derive(Parser)]
#[command(name = "rs-immicher-oxide", about = "Immich transpiler framework")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// Shared config values used across subcommands.
#[derive(Clone)]
struct RuntimeConfig {
    immich_url: String,
    api_key: String,
    video_crf: u32,
    video_preset: u8,
    image_distance: f32,
    render_device: Option<String>,
    concurrency: usize,
    dry_run: bool,
}

impl RuntimeConfig {
    fn to_immich_config(&self) -> ImmichConfig {
        ImmichConfig::new(&self.immich_url, &self.api_key).with_concurrency(self.concurrency)
    }

    fn build_image_pipeline(&self) -> (ImmichSource, JxlTranscoder, ImmichSink) {
        let immich = self.to_immich_config();
        let source = ImmichSource::new(immich.clone(), MediaCodec::Jxl).with_images_only();
        let transcoder = JxlTranscoder::new(self.image_distance);
        let sink = ImmichSink::new(immich);
        (source, transcoder, sink)
    }

    fn build_video_pipeline(&self) -> (ImmichSource, SvtAv1UhqTranscoder, ImmichSink) {
        let immich = self.to_immich_config();
        let source = ImmichSource::new(immich.clone(), MediaCodec::Av1).with_videos_only();
        let mut transcoder = SvtAv1UhqTranscoder::new(self.video_crf, self.video_preset);
        if let Some(ref dev) = self.render_device {
            transcoder = transcoder.with_render_device(dev.clone());
        }
        let sink = ImmichSink::new(immich);
        (source, transcoder, sink)
    }
}

#[derive(Subcommand)]
enum Command {
    /// One-shot conversion: discover existing assets, transcode, upload.
    Run {
        /// Immich server URL.
        #[arg(short, long, env = "IMMICH_URL")]
        immich_url: String,

        /// Immich API key.
        #[arg(short, long, env = "IMMICH_API_KEY")]
        api_key: String,

        /// Only process videos.
        #[arg(long)]
        video_only: bool,

        /// Only process images.
        #[arg(long)]
        image_only: bool,

        /// AV1 CRF (0-63, lower = better).
        #[arg(long, default_value_t = 20)]
        video_crf: u32,

        /// SVT-AV1 preset (0-13, lower = slower/better).
        #[arg(long, default_value_t = 6)]
        video_preset: u8,

        /// JXL butteraugli distance (0 = lossless, 1.0 = visually lossless).
        #[arg(long, default_value_t = 1.0)]
        image_distance: f32,

        /// VAAPI render device path (e.g. /dev/dri/renderD128).
        #[arg(long)]
        render_device: Option<String>,

        /// Dry run (discover only, no transcode/upload).
        #[arg(long, default_value_t = true)]
        dry_run: bool,

        /// Concurrency level.
        #[arg(long, default_value_t = 2)]
        concurrency: usize,
    },

    /// Daemon mode: poll for new assets and process them.
    Watch {
        /// Immich server URL.
        #[arg(short, long, env = "IMMICH_URL")]
        immich_url: String,

        /// Immich API key.
        #[arg(short, long, env = "IMMICH_API_KEY")]
        api_key: String,

        /// AV1 CRF (0-63, lower = better).
        #[arg(long, default_value_t = 20)]
        video_crf: u32,

        /// SVT-AV1 preset (0-13, lower = slower/better).
        #[arg(long, default_value_t = 6)]
        video_preset: u8,

        /// JXL butteraugli distance.
        #[arg(long, default_value_t = 1.0)]
        image_distance: f32,

        /// VAAPI render device path.
        #[arg(long)]
        render_device: Option<String>,

        /// Poll interval.
        #[arg(short, long, default_value = "5min")]
        interval: String,

        /// Dry run (discover only, no transcode/upload).
        #[arg(long, default_value_t = true)]
        dry_run: bool,
    },

    /// Serve HTTP webhook endpoint for Immich server-side events.
    Serve {
        /// Bind address.
        #[arg(long, default_value = "0.0.0.0:8088")]
        bind: String,

        /// Immich server URL.
        #[arg(short, long, env = "IMMICH_URL")]
        immich_url: String,

        /// Immich API key.
        #[arg(short, long, env = "IMMICH_API_KEY")]
        api_key: String,

        /// AV1 CRF.
        #[arg(long, default_value_t = 20)]
        video_crf: u32,

        /// SVT-AV1 preset.
        #[arg(long, default_value_t = 6)]
        video_preset: u8,

        /// JXL distance.
        #[arg(long, default_value_t = 1.0)]
        image_distance: f32,

        /// VAAPI device.
        #[arg(long)]
        render_device: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Command::Run {
            immich_url,
            api_key,
            video_only,
            image_only,
            video_crf,
            video_preset,
            image_distance,
            render_device,
            dry_run,
            concurrency,
        } => {
            let cfg = RuntimeConfig {
                immich_url,
                api_key,
                video_crf,
                video_preset,
                image_distance,
                render_device,
                concurrency,
                dry_run,
            };

            let want_images = !video_only;
            let want_videos = !image_only;

            if want_images {
                run_pipeline(&cfg, "images", |c| {
                    let (s, t, si) = c.build_image_pipeline();
                    (
                        Box::new(s) as Box<dyn Source>,
                        Box::new(t) as Box<dyn Transcoder>,
                        Box::new(si) as Box<dyn Sink>,
                    )
                })
                .await?;
            }

            if want_videos {
                run_pipeline(&cfg, "videos", |c| {
                    let (s, t, si) = c.build_video_pipeline();
                    (Box::new(s), Box::new(t), Box::new(si))
                })
                .await?;
            }
        }

        Command::Watch {
            immich_url,
            api_key,
            video_crf,
            video_preset,
            image_distance,
            render_device,
            interval,
            dry_run,
        } => {
            let cfg = RuntimeConfig {
                immich_url,
                api_key,
                video_crf,
                video_preset,
                image_distance,
                render_device,
                concurrency: 1,
                dry_run,
            };

            let interval = parse_duration(&interval)?;
            tracing::info!("watch mode, interval {:?}", interval);

            loop {
                // Process images then videos each cycle
                run_pipeline(&cfg, "images (watch)", |c| {
                    let (s, t, si) = c.build_image_pipeline();
                    (Box::new(s), Box::new(t), Box::new(si))
                })
                .await?;

                run_pipeline(&cfg, "videos (watch)", |c| {
                    let (s, t, si) = c.build_video_pipeline();
                    (Box::new(s), Box::new(t), Box::new(si))
                })
                .await?;

                tracing::info!("sleeping for {:?}", interval);
                tokio::time::sleep(interval).await;
            }
        }

        Command::Serve {
            bind,
            immich_url,
            api_key,
            video_crf,
            video_preset,
            image_distance,
            render_device,
        } => {
            let cfg = Arc::new(RuntimeConfig {
                immich_url,
                api_key,
                video_crf,
                video_preset,
                image_distance,
                render_device,
                concurrency: 2,
                dry_run: false,
            });

            tracing::info!("webhook server starting on {}", bind);
            serve_webhook(bind, cfg).await?;
        }
    }

    Ok(())
}

async fn run_pipeline(
    cfg: &RuntimeConfig,
    label: &str,
    build: impl FnOnce(&RuntimeConfig) -> (Box<dyn Source>, Box<dyn Transcoder>, Box<dyn Sink>),
) -> anyhow::Result<PipelineSummary> {
    tracing::info!("=== {} pipeline ===", label);

    let (source, transcoder, sink) = build(cfg);
    let mut pipeline = Pipeline::new(source, transcoder, sink).with_dry_run(cfg.dry_run);

    let summary = pipeline
        .run(|outcome| match &outcome {
            AssetOutcome::Success {
                asset_id, stats, ..
            } => {
                tracing::info!(
                    "✓ {} | {:.1}% of original, {:.1}s",
                    asset_id,
                    stats.compression_ratio * 100.0,
                    stats.total_seconds,
                );
            }
            AssetOutcome::PartialSuccess {
                asset_id, warning, ..
            } => {
                tracing::warn!("⚠ {} partial: {}", asset_id, warning);
            }
            AssetOutcome::Skipped { asset_id, reason } => {
                tracing::debug!("- {} skipped: {}", asset_id, reason);
            }
            AssetOutcome::Failed { asset_id, error } => {
                tracing::error!("✗ {} failed: {}", asset_id, error);
            }
        })
        .await?;

    tracing::info!(
        "=== {} done: {} succeeded, {} partial, {} skipped, {} failed ===",
        label,
        summary.succeeded,
        summary.partial,
        summary.skipped,
        summary.failed,
    );

    Ok(summary)
}

async fn serve_webhook(bind: String, cfg: Arc<RuntimeConfig>) -> anyhow::Result<()> {
    use tokio::net::TcpListener;

    let listener = TcpListener::bind(&bind).await?;

    loop {
        let (stream, addr) = listener.accept().await?;
        let cfg = cfg.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, cfg).await {
                tracing::error!("webhook from {}: {}", addr, e);
            }
        });
    }
}

async fn handle_connection(
    stream: tokio::net::TcpStream,
    cfg: Arc<RuntimeConfig>,
) -> anyhow::Result<()> {
    use tokio::io::AsyncReadExt;

    let mut buf = vec![0u8; 4096];
    let mut reader = tokio::io::BufReader::new(stream);
    let n = reader.read(&mut buf).await?;
    let request = String::from_utf8_lossy(&buf[..n]);

    // Very simple HTTP parser — just check if it's a POST
    if !request.starts_with("POST") {
        return Ok(());
    }

    // Find Content-Length
    let body_start = request.find("\r\n\r\n").map(|i| i + 4).unwrap_or(n);
    let body = &request[body_start..n];

    // Parse the webhook event body
    if let Ok(event) = serde_json::from_str::<serde_json::Value>(body) {
        let asset_type = event
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        tracing::info!("webhook event: type={}", asset_type);

        // Process asset based on event type
        if asset_type == "asset.upload" || asset_type == "asset.update" {
            if let Some(asset_id) = event.get("id").and_then(|v| v.as_str()) {
                tracing::info!("processing new asset {}", asset_id);

                // Determine if image or video and run appropriate pipeline
                let mime = event
                    .get("originalMimeType")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if mime.starts_with("video/") {
                    let (source, transcoder, sink) = cfg.build_video_pipeline();
                    let mut pipeline =
                        Pipeline::new(Box::new(source), Box::new(transcoder), Box::new(sink))
                            .with_dry_run(cfg.dry_run);
                    let _ = pipeline.run(|_| {}).await;
                } else if mime.starts_with("image/") && mime != "image/jxl" {
                    let (source, transcoder, sink) = cfg.build_image_pipeline();
                    let mut pipeline =
                        Pipeline::new(Box::new(source), Box::new(transcoder), Box::new(sink))
                            .with_dry_run(cfg.dry_run);
                    let _ = pipeline.run(|_| {}).await;
                }
            }
        }
    }

    Ok(())
}

/// Parse a duration string like "5min", "1h", "30s".
fn parse_duration(s: &str) -> anyhow::Result<Duration> {
    let s = s.trim();
    if let Some(n) = s.strip_suffix("min") {
        let secs: u64 = n.parse()?;
        Ok(Duration::from_secs(secs * 60))
    } else if let Some(n) = s.strip_suffix('h') {
        let secs: u64 = n.parse()?;
        Ok(Duration::from_secs(secs * 3600))
    } else if let Some(n) = s.strip_suffix('s') {
        let secs: u64 = n.parse()?;
        Ok(Duration::from_secs(secs))
    } else {
        // Plain number = seconds
        let secs: u64 = s.parse()?;
        Ok(Duration::from_secs(secs))
    }
}
