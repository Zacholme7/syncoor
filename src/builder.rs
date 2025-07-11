use crate::{SyncMessage, Syncoor};
use alloy_provider::Provider;
use alloy_rpc_types::Filter;
use std::sync::Arc;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

/// Builder for Syncoor
pub struct SyncoorBuilder<P> {
    filter: Filter,
    http_provider: P,
    ws_provider: P,
    batch_size: Option<u64>,
    from_block: Option<u64>,
}

impl<P> SyncoorBuilder<P>
where
    P: Provider + Send + Sync + 'static,
{
    /// Create a new SyncoorBuilder with both providers
    pub fn new(filter: Filter, http_provider: P, ws_provider: P) -> Self {
        Self {
            filter,
            http_provider,
            ws_provider,
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

    /// Build the Syncoor
    pub fn build(self) -> (Syncoor<P>, UnboundedReceiver<SyncMessage>) {
        let (sender, receiver) = unbounded_channel::<SyncMessage>();

        let syncoor = Syncoor {
            sender,
            filter: self.filter,
            http_provider: Arc::new(self.http_provider),
            ws_provider: Arc::new(self.ws_provider),
            batch_size: self.batch_size.unwrap_or(1000),
            from_block: self.from_block.unwrap_or(0),
        };

        (syncoor, receiver)
    }
}
