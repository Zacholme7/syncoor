use alloy_primitives::Log;
use alloy_provider::{DynProvider, Provider};
use alloy_rpc_types::Filter;
use anyhow::{Result, anyhow};
use futures_util::StreamExt;
use std::cmp::min;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

pub use builder::SyncoorBuilder;
mod builder;

/// Sync mode to distinguish between historical catchup and live tracking
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SyncMode {
    Historical,
    Live,
}

/// Sync Message that carries logs and context information
#[derive(Debug, Clone)]
pub enum SyncMessage {
    /// Logs with sync mode context
    Logs {
        logs: Vec<Log>,
        mode: SyncMode,
        block_range: (u64, u64), // (start_block, end_block)
    },
    /// Transition from historical to live sync
    ModeTransition { from: SyncMode, to: SyncMode },
    /// Progress update
    Progress {
        current_block: u64,
        latest_block: u64,
        is_live: bool,
    },
    /// Error occurred during sync
    Error(String),
}

/// Sync state to track current progress
#[derive(Debug, Clone)]
struct SyncState {
    current_block: u64,
    target_block: u64,
    is_live: bool,
}

/// Sync Coordinator
pub struct Syncoor {
    /// Sends newly synced event logs
    sender: UnboundedSender<SyncMessage>,
    /// The event filter to filter events
    filter: Filter,
    /// Http provider for Historical syncing
    http_provider: Arc<DynProvider>,
    /// Websocket provider for live syncing
    ws_provider: Arc<DynProvider>,
    /// The range of blocks to sync per batch
    batch_size: u64,
    /// Starting block for historical sync (None means start from genesis)
    from_block: u64,
}

impl Syncoor {
    /// Start the sync process
    pub async fn start(&mut self) -> Result<()> {
        println!("Getting latest block number...");
        // Get the latest block number
        let latest_block = self
            .http_provider
            .get_block_number()
            .await
            .map_err(|e| anyhow!("Failed to get latest block: {}", e))?;

        println!("Latest block: {}", latest_block);

        let mut state = SyncState {
            current_block: self.from_block,
            target_block: latest_block,
            is_live: false,
        };

        // Run historical sync first - this will loop until we catch up to the tip
        if state.current_block < state.target_block {
            self.historical_sync(&mut state).await?;

            // Signal transition to live mode
            if let Err(e) = self.sender.send(SyncMessage::ModeTransition {
                from: SyncMode::Historical,
                to: SyncMode::Live,
            }) {
                return Err(anyhow!("Failed to send mode transition: {}", e));
            }
        }

        // Mark as live and continue
        state.is_live = true;

        // Start live sync with streaming
        self.live_sync(state).await?;

        Ok(())
    }

    /// Perform the historical sync - loops until caught up to the tip
    async fn historical_sync(&self, state: &mut SyncState) -> Result<()> {
        loop {
            // Check the current tip of the chain
            let current_tip =
                self.http_provider.get_block_number().await.map_err(|e| {
                    anyhow!("Failed to get latest block during historical sync: {}", e)
                })?;

            // If we've caught up to the tip, we're done with historical sync
            if state.current_block >= current_tip {
                state.target_block = current_tip;
                break;
            }

            // Update target to current tip
            state.target_block = current_tip;

            // Process a batch
            let batch_start = state.current_block;
            let batch_end = min(batch_start + self.batch_size - 1, state.target_block);

            // Create filter with block range
            let mut batch_filter = self.filter.clone();
            batch_filter = batch_filter.from_block(batch_start).to_block(batch_end);

            match self.http_provider.get_logs(&batch_filter).await {
                Ok(logs) => {
                    // Send logs if any found
                    if !logs.is_empty() {
                        if let Err(e) = self.sender.send(SyncMessage::Logs {
                            logs: logs.into_iter().map(|log| log.inner).collect(),
                            mode: SyncMode::Historical,
                            block_range: (batch_start, batch_end),
                        }) {
                            return Err(anyhow!("Failed to send logs: {}", e));
                        }
                    }

                    // Send progress update
                    let _ = self.sender.send(SyncMessage::Progress {
                        current_block: batch_end,
                        latest_block: current_tip,
                        is_live: false,
                    });

                    // Update state
                    state.current_block = batch_end + 1;
                }
                Err(e) => {
                    let error_msg = format!(
                        "Failed to get logs for blocks {}-{}: {}",
                        batch_start, batch_end, e
                    );
                    let _ = self.sender.send(SyncMessage::Error(error_msg.clone()));
                    return Err(anyhow!(error_msg));
                }
            }
        }

        Ok(())
    }

    /// Long running live sync task that uses log subscription
    async fn live_sync(&self, mut state: SyncState) -> Result<()> {
        // Subscribe to logs matching the filter
        let subscription = self
            .ws_provider
            .subscribe_logs(&self.filter)
            .await
            .map_err(|e| {
                anyhow!(
                    "Failed to subscribe to logs: {}. Make sure you're using a WebSocket provider.",
                    e
                )
            })?;

        let mut stream = subscription.into_stream();

        // Process each log as it arrives
        while let Some(log) = stream.next().await {
            // Extract block number from the log
            let block_number = log
                .block_number
                .ok_or_else(|| anyhow!("Received log without block number"))?;

            // Send the log
            if let Err(e) = self.sender.send(SyncMessage::Logs {
                logs: vec![log.inner],
                mode: SyncMode::Live,
                block_range: (block_number, block_number),
            }) {
                return Err(anyhow!("Failed to send live log: {}", e));
            }

            // Update state and send progress
            if block_number >= state.current_block {
                state.current_block = block_number + 1;

                if let Err(e) = self.sender.send(SyncMessage::Progress {
                    current_block: block_number,
                    latest_block: block_number,
                    is_live: true,
                }) {
                    return Err(anyhow!("Failed to send progress: {}", e));
                }
            }
        }

        // If we reach here, the stream has ended
        Err(anyhow!("Log subscription stream ended unexpectedly"))
    }
}
