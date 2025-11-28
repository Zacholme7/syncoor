use alloy_rpc_types::Filter;
use alloy_sol_types::SolEvent;
use anyhow::Result;
use evm_abi::factories::UniswapV2Factory;
use syncoor::{SyncMessage, SyncoorBuilder};
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();

    // Configure URLs
    let http_url = "https://eth.merkle.io";
    let ws_url = "wss://eth.merkle.io";

    // Construct your desired filter. Match against addresses, events, etc...
    let filter = Filter::new().event_signature(UniswapV2Factory::PairCreated::SIGNATURE_HASH);

    // Build Syncoor with URLs and optional configuration
    let (syncoor, mut receiver) = SyncoorBuilder::new(filter, http_url, ws_url)
        .batch_size(1_000) // Optional: default is 1000
        //.from_block(22_800_000) // Optional: default is 0
        .from_block(23891117)
        .build()
        .await?;
    syncoor.start().await?;

    // Receive all of the sync messages
    while let Some(msg) = receiver.recv().await {
        match msg {
            SyncMessage::Logs {
                logs,
                mode,
                block_range,
            } => {
                info!(
                    "Received {} logs from {:?} mode, blocks {:?}",
                    logs.len(),
                    mode,
                    block_range
                );
            }
            SyncMessage::ModeTransition { from, to } => {
                info!("Transitioned from {:?} to {:?}", from, to);
            }
            SyncMessage::Progress {
                current_block,
                latest_block,
                is_live,
            } => {
                info!(
                    "Progress: {}/{} (live: {})",
                    current_block, latest_block, is_live
                );
            }
            SyncMessage::Error(e) => {
                error!("Sync error: {}", e);
            }
        }
    }

    Ok(())
}
