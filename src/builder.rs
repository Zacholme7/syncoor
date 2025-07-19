use crate::{SyncMessage, Syncoor};
use alloy_provider::{Provider, ProviderBuilder, WsConnect};
use alloy_rpc_types::Filter;
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

/// Builder for Syncoor
pub struct SyncoorBuilder {
    filter: Filter,
    http_url: String,
    ws_url: String,
    batch_size: Option<u64>,
    from_block: Option<u64>,
}

impl SyncoorBuilder {
    /// Create a new SyncoorBuilder with HTTP and WebSocket URLs
    pub fn new(filter: Filter, http_url: impl Into<String>, ws_url: impl Into<String>) -> Self {
        Self {
            filter,
            http_url: http_url.into(),
            ws_url: ws_url.into(),
            batch_size: None,
            from_block: None,
        }
    }

    /// Set the batch size for historical sync (default: 1000)
    pub fn batch_size(mut self, size: u64) -> Self {
        self.batch_size = Some(size);
        self
    }

    /// Set the starting block for historical sync (default: 0)
    pub fn from_block(mut self, block: u64) -> Self {
        self.from_block = Some(block);
        self
    }

    /// Build the Syncoor with properly configured providers
    pub async fn build(self) -> Result<(Syncoor<impl Provider>, UnboundedReceiver<SyncMessage>)> {
        let (sender, receiver) = unbounded_channel::<SyncMessage>();

        // Create HTTP provider with connection pooling
        let http_client = reqwest::ClientBuilder::new()
            .pool_max_idle_per_host(10)
            .pool_idle_timeout(std::time::Duration::from_secs(30))
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        let http_provider = ProviderBuilder::default()
            .connect_reqwest(http_client, self.http_url.parse()?)
            .erased();

        // Create WebSocket provider
        let ws_connect = WsConnect::new(self.ws_url);
        let ws_provider = ProviderBuilder::new()
            .connect_ws(ws_connect)
            .await?
            .erased();

        let syncoor = Syncoor {
            sender,
            filter: self.filter,
            http_provider: Arc::new(http_provider),
            ws_provider: Arc::new(ws_provider),
            batch_size: self.batch_size.unwrap_or(1000),
            from_block: self.from_block.unwrap_or(0),
        };

        Ok((syncoor, receiver))
    }
}
