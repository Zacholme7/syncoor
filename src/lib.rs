use alloy_primitives::Log;
use alloy_provider::{DynProvider, Provider};
use alloy_rpc_types::Filter;
use futures_util::stream::{self, BoxStream};
use futures_util::StreamExt;
use std::cmp::min;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{error, info};

pub use builder::SyncoorBuilder;
pub use error::{Result, SyncoorError};
mod builder;
mod error;

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
    /// Max retries for failed historical sync batches
    max_retries: u32,
    /// Max number of concurrent historical batch requests
    concurrent_batches: usize,
    /// Preserve block order when processing historical batches
    preserve_block_order: bool,
}

impl Syncoor {
    /// Start the sync process in the background. Consumes the coordinator and returns immediately
    /// after the task is spawned.
    pub async fn start(self) -> Result<()> {
        tokio::spawn(async move {
            if let Err(e) = self.run().await {
                error!(error = ?e, "Sync loop exited with error");
            }
        });

        Ok(())
    }

    /// Full sync flow. Intended to run inside the background task.
    async fn run(self) -> Result<()> {
        info!("Getting latest block number...");
        // Get the latest block number
        let latest_block = self
            .http_provider
            .get_block_number()
            .await
            .map_err(SyncoorError::GetLatestBlock)?;

        info!(latest_block, "Fetched latest block");

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
                return Err(SyncoorError::SendModeTransition(e));
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
            let current_tip = self
                .http_provider
                .get_block_number()
                .await
                .map_err(SyncoorError::GetLatestBlockHistorical)?;

            // If we've caught up to the tip, we're done with historical sync
            if state.current_block >= current_tip {
                state.target_block = current_tip;
                break;
            }

            // Update target to current tip
            state.target_block = current_tip;

            // Build batch ranges for the current window
            let mut ranges = Vec::new();
            let mut batch_start = state.current_block;
            while batch_start <= state.target_block {
                let batch_end = min(batch_start + self.batch_size - 1, state.target_block);
                ranges.push((batch_start, batch_end));
                batch_start = batch_end + 1;
            }

            let fetches = stream::iter(ranges).map(|(start, end)| async move {
                let logs = self.fetch_batch_logs(start, end).await?;
                Ok((start, end, logs))
            });

            let mut fetches: BoxStream<'_, Result<(u64, u64, Vec<Log>)>> =
                if self.preserve_block_order {
                    fetches.buffered(self.concurrent_batches).boxed()
                } else {
                    fetches.buffer_unordered(self.concurrent_batches).boxed()
                };

            while let Some(result) = fetches.next().await {
                match result {
                    Ok((batch_start, batch_end, logs)) => {
                        // Send logs if any found
                        if !logs.is_empty()
                            && let Err(e) = self.sender.send(SyncMessage::Logs {
                                logs,
                                mode: SyncMode::Historical,
                                block_range: (batch_start, batch_end),
                            })
                        {
                            return Err(SyncoorError::SendLogs(e));
                        }

                        // Send progress update
                        let _ = self.sender.send(SyncMessage::Progress {
                            current_block: batch_end,
                            latest_block: current_tip,
                            is_live: false,
                        });
                    }
                    Err(error) => {
                        let _ = self.sender.send(SyncMessage::Error(error.to_string()));
                        return Err(error);
                    }
                }
            }

            // All batches in the current window completed
            state.current_block = state.target_block + 1;
        }

        Ok(())
    }

    async fn fetch_batch_logs(&self, start: u64, end: u64) -> Result<Vec<Log>> {
        let mut batch_filter = self.filter.clone();
        batch_filter = batch_filter.from_block(start).to_block(end);

        let mut attempt = 0;
        loop {
            match self.http_provider.get_logs(&batch_filter).await {
                Ok(logs) => {
                    let logs = logs.into_iter().map(|log| log.inner).collect();
                    return Ok(logs);
                }
                Err(source) => {
                    attempt += 1;
                    if attempt >= self.max_retries {
                        return Err(SyncoorError::GetLogs { start, end, source });
                    }
                }
            }
        }
    }

    /// Long running live sync task that uses log subscription
    async fn live_sync(&self, mut state: SyncState) -> Result<()> {
        // Subscribe to logs matching the filter
        let subscription = self
            .ws_provider
            .subscribe_logs(&self.filter)
            .await
            .map_err(SyncoorError::SubscribeLogs)?;

        let mut stream = subscription.into_stream();

        // Process each log as it arrives
        while let Some(log) = stream.next().await {
            // Extract block number from the log
            let block_number = log
                .block_number
                .ok_or_else(|| SyncoorError::MissingBlockNumber)?;

            // Send the log
            if let Err(e) = self.sender.send(SyncMessage::Logs {
                logs: vec![log.inner],
                mode: SyncMode::Live,
                block_range: (block_number, block_number),
            }) {
                return Err(SyncoorError::SendLiveLog(e));
            }

            // Update state and send progress
            if block_number >= state.current_block {
                state.current_block = block_number + 1;

                if let Err(e) = self.sender.send(SyncMessage::Progress {
                    current_block: block_number,
                    latest_block: block_number,
                    is_live: true,
                }) {
                    return Err(SyncoorError::SendProgress(e));
                }
            }
        }

        // If we reach here, the stream has ended
        Err(SyncoorError::StreamEnded)
    }
}
