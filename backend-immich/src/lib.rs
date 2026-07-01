pub mod client;
pub mod sink;
pub mod source;
pub mod watcher;

/// Configuration for the Immich backend.
#[derive(Debug, Clone)]
pub struct ImmichConfig {
    pub server_url: String,
    pub api_key: String,
    pub concurrency: usize,
}

impl ImmichConfig {
    pub fn new(server_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            server_url: server_url.into(),
            api_key: api_key.into(),
            concurrency: 2,
        }
    }

    /// Set the number of concurrent transcode operations.
    pub fn with_concurrency(mut self, n: usize) -> Self {
        self.concurrency = n;
        self
    }
}
