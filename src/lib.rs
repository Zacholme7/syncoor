pub mod error;

use crate::error::{Result, SyncoorError};
use alloy_primitives::Log;
use alloy_provider::{DynProvider, Provider};
use alloy_rpc_types::Filter;
use futures_util::StreamExt;
use std::cmp::min;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use tracing::error;

pub use builder::SyncoorBuilder;
mod builder;

/// Sync mode to distinguish between historical catchup and live tracking
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SyncMode {
    Historical,
    Live,
}

/// Sync Message that carries logs and context information
#[derive(Debug)]
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
    Error(SyncoorError),
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
    pub async fn start(self) -> Result<()> {
        // Get the latest block number up front so we can fail early if needed
        let latest_block = self
            .http_provider
            .get_block_number()
            .await
            .map_err(|e| SyncoorError::LatestBlockFetch(e.to_string()))?;

        let mut state = SyncState {
            current_block: self.from_block,
            target_block: latest_block,
            is_live: false,
        };

        tokio::spawn(async move {
            let result: Result<()> = async {
                // Run historical sync first - this will loop until we catch up to the tip
                if state.current_block < state.target_block {
                    self.historical_sync(&mut state).await?;

                    // Signal transition to live mode; log instead of failing if the channel is gone
                    if let Err(e) = self.sender.send(SyncMessage::ModeTransition {
                        from: SyncMode::Historical,
                        to: SyncMode::Live,
                    }) {
                        error!("Failed to send mode transition: {}", e);
                    }
                }

                // Mark as live and continue
                state.is_live = true;

                // Start live sync with streaming
                self.live_sync(state).await
            }
            .await;

            if let Err(e) = result {
                error!("Sync task exited with error: {}", e);
                let _ = self.sender.send(SyncMessage::Error(e));
            }
        });

        Ok(())
    }

    /// Perform the historical sync - loops until caught up to the tip
    async fn historical_sync(&self, state: &mut SyncState) -> Result<()> {
        loop {
            // Check the current tip of the chain
            let current_tip = self
                .http_provider
                .get_block_number()
                .await
                .map_err(|e| SyncoorError::HistoricalLatestBlock(e.to_string()))?;

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

            // Fetch and emit logs for the batch
            match self.fetch_logs_range(batch_start, batch_end).await {
                Ok(logs) => {
                    self.emit_logs(logs, SyncMode::Historical, (batch_start, batch_end))?;
                }
                Err(reason) => {
                    let error = SyncoorError::HistoricalLogs {
                        start: batch_start,
                        end: batch_end,
                        reason,
                    };
                    let _ = self.sender.send(SyncMessage::Error(error.clone()));
                    return Err(error);
                }
            }

            // Send progress update
            let _ = self.emit_progress(batch_end, current_tip, false);

            // Update state
            state.current_block = batch_end + 1;
        }

        Ok(())
    }

    /// Long running live sync task that subscribes to new blocks and fetches logs per block
    async fn live_sync(&self, mut state: SyncState) -> Result<()> {
        // Subscribe to new blocks
        let subscription = self
            .ws_provider
            .subscribe_blocks()
            .await
            .map_err(|e| SyncoorError::Subscribe(e.to_string()))?;

        let mut stream = subscription.into_stream();

        // For each new block, fetch logs in that block using the HTTP provider
        while let Some(header) = stream.next().await {
            let block_number = header.number;

            match self.fetch_logs_range(block_number, block_number).await {
                Ok(logs) => {
                    self.emit_logs(logs, SyncMode::Live, (block_number, block_number))?;

                    // Update state and send progress
                    if block_number >= state.current_block {
                        state.current_block = block_number + 1;

                        let _ = self.emit_progress(block_number, block_number, true);
                    }
                }
                Err(reason) => {
                    let error = SyncoorError::LiveLogs {
                        block: block_number,
                        reason,
                    };
                    let _ = self.sender.send(SyncMessage::Error(error.clone()));
                    return Err(error);
                }
            }
        }

        // If we reach here, the stream has ended
        Err(SyncoorError::StreamEnded)
    }

    /// Build a filter for a specific block range
    fn range_filter(&self, start: u64, end: u64) -> Filter {
        self.filter.clone().from_block(start).to_block(end)
    }

    /// Fetch logs for a block range via the HTTP provider
    async fn fetch_logs_range(
        &self,
        start: u64,
        end: u64,
    ) -> std::result::Result<Vec<Log>, String> {
        let filter = self.range_filter(start, end);
        self.http_provider
            .get_logs(&filter)
            .await
            .map(|logs| logs.into_iter().map(|log| log.inner).collect())
            .map_err(|e| e.to_string())
    }

    /// Emit logs to the consumer
    fn emit_logs(&self, logs: Vec<Log>, mode: SyncMode, block_range: (u64, u64)) -> Result<()> {
        if logs.is_empty() {
            return Ok(());
        }

        self.sender
            .send(SyncMessage::Logs {
                logs,
                mode,
                block_range,
            })
            .map_err(|e| match mode {
                SyncMode::Historical => SyncoorError::SendLogs(e.to_string()),
                SyncMode::Live => SyncoorError::SendLiveLog(e.to_string()),
            })
    }

    /// Emit progress to the consumer
    fn emit_progress(&self, current_block: u64, latest_block: u64, is_live: bool) -> Result<()> {
        self.sender
            .send(SyncMessage::Progress {
                current_block,
                latest_block,
                is_live,
            })
            .map_err(|e| SyncoorError::SendProgress(e.to_string()))
    }
}
