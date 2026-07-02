use rs_immicher_oxide_core::error::Result;

use crate::client::ImmichApiClient;
use crate::ImmichConfig;

/// Watches for new Immich assets by periodically polling the search API.
pub struct ImmichWatcher {
    #[allow(dead_code)]
    config: ImmichConfig,
    client: ImmichApiClient,
    /// Timestamp of the last poll (ISO 8601).
    last_poll: chrono::DateTime<chrono::Utc>,
    /// Poll interval.
    interval: std::time::Duration,
}

impl ImmichWatcher {
    /// Create a new watcher.
    ///
    /// `interval` controls how often to poll for new assets (e.g. 5 minutes).
    /// `lookback` controls how far back the initial poll looks (e.g. 1 hour).
    pub fn new(config: ImmichConfig, interval: std::time::Duration) -> Self {
        let client = ImmichApiClient::new(&config);
        // Initial lookback: start with whatever is recent
        let last_poll = chrono::Utc::now();
        Self {
            config,
            client,
            last_poll,
            interval,
        }
    }

    /// Return the current interval.
    pub fn interval(&self) -> std::time::Duration {
        self.interval
    }

    /// Poll for assets created after `last_poll`, update the timestamp.
    pub async fn poll_new_assets(&mut self) -> Result<Vec<crate::client::AssetResponseDto>> {
        let since = self.last_poll;
        self.last_poll = chrono::Utc::now();

        let assets = self
            .client
            .search_all_assets(
                None,
                Some(since.format("%Y-%m-%dT%H:%M:%S.000Z").to_string()),
            )
            .await
            .map_err(|e| rs_immicher_oxide_core::error::PipelineError::Source(Box::new(e)))?;

        Ok(assets)
    }
}
