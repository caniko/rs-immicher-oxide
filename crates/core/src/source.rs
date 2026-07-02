use async_trait::async_trait;
use futures::stream::BoxStream;

use crate::error::Result;
use crate::types::Asset;

/// A source of media assets to process.
///
/// Implementations discover assets (existing library scans, filesystem watches,
/// webhooks) and provide their original byte streams.
#[async_trait]
pub trait Source: Send + Sync {
    /// A human-readable label for this source.
    fn label(&self) -> &'static str;

    /// Discover and stream assets.
    ///
    /// The stream must yield available assets in batches. Each asset
    /// represents a single media file to be transcoded.
    async fn discover(&self) -> Result<BoxStream<'_, Result<Asset>>>;

    /// Open the original bytes of an asset as a readable stream.
    async fn open_original(&self, asset: &Asset) -> Result<Box<dyn Read + Send + Unpin + 'static>>;
}

use std::io::Read;
