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
    pub async fn build(self) -> Result<(Syncoor, UnboundedReceiver<SyncMessage>)> {
        let (sender, receiver) = unbounded_channel::<SyncMessage>();

        let syncoor = Syncoor {
            sender,
            filter: self.filter,
            http_provider: self.http_provider,
            ws_provider: self.ws_provider,
            batch_size: self.batch_size.unwrap_or(1000),
            from_block: self.from_block.unwrap_or(0),
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
