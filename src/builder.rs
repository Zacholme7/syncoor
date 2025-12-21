use crate::{Result, SyncMessage, Syncoor};
use alloy_provider::{DynProvider, Provider};
use alloy_rpc_types::Filter;
use std::sync::Arc;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

/// Builder for Syncoor
#[derive(Clone)]
pub struct SyncoorBuilder {
    filter: Filter,
    http_provider: Arc<DynProvider>,
    ws_provider: Arc<DynProvider>,
    batch_size: Option<u64>,
    from_block: Option<u64>,
    max_retries: Option<u32>,
    concurrent_batches: Option<usize>,
    preserve_block_order: Option<bool>,
}

impl SyncoorBuilder {
    /// Create a new SyncoorBuilder with HTTP and WebSocket providers.
    pub fn new<P>(filter: Filter, http_provider: P, ws_provider: P) -> Self
    where
        P: Provider + 'static,
    {
        Self {
            filter,
            http_provider: Arc::new(http_provider.erased()),
            ws_provider: Arc::new(ws_provider.erased()),
            batch_size: None,
            from_block: None,
            max_retries: None,
            concurrent_batches: None,
            preserve_block_order: None,
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

    /// Set the max retries for a failed historical sync batch (default: 3)
    pub fn max_retries(mut self, retries: u32) -> Self {
        self.max_retries = Some(retries);
        self
    }

    /// Set the number of concurrent historical batch requests (default: 4)
    pub fn concurrent_batches(mut self, count: usize) -> Self {
        self.concurrent_batches = Some(count);
        self
    }

    /// Preserve block order when processing historical batches (default: true)
    pub fn preserve_block_order(mut self, preserve: bool) -> Self {
        self.preserve_block_order = Some(preserve);
        self
    }

    /// Build the Syncoor with properly configured providers
    pub async fn build(self) -> Result<(Syncoor, UnboundedReceiver<SyncMessage>)> {
        let (sender, receiver) = unbounded_channel::<SyncMessage>();

        let syncoor = Syncoor {
            sender,
            filter: self.filter,
            http_provider: self.http_provider,
            ws_provider: self.ws_provider,
            batch_size: self.batch_size.unwrap_or(1000),
            from_block: self.from_block.unwrap_or(0),
            max_retries: self.max_retries.unwrap_or(3),
            concurrent_batches: self.concurrent_batches.unwrap_or(4).max(1),
            preserve_block_order: self.preserve_block_order.unwrap_or(true),
        };

        Ok((syncoor, receiver))
    }

    /// Build the Syncoor, start it in the background, and return the receiver.
    pub async fn build_and_start(self) -> Result<UnboundedReceiver<SyncMessage>> {
        let (syncoor, receiver) = self.build().await?;
        syncoor.start().await?;
        Ok(receiver)
    }
}
