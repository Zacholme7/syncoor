use alloy_rpc_types::Filter;
use alloy_sol_types::SolEvent;
use evm_abi::factories::UniswapV2Factory;
use syncoor::{Result, SyncMessage, SyncoorBuilder};

#[tokio::main]
async fn main() -> Result<()> {
    // Configure URLs
    let http_url = "http://localhost:8545";
    let ws_url = "ws://localhost:8546";

    // Construct your desired filter. Match against addresses, events, etc...
    let filter = Filter::new().event_signature(UniswapV2Factory::PairCreated::SIGNATURE_HASH);

    // Build Syncoor with URLs and optional configuration
    let (mut syncoor, mut receiver) = SyncoorBuilder::new(filter, http_url, ws_url)
        .batch_size(5_000) // Optional: default is 1000
        .from_block(22_800_000) // Optional: default is 0
        .build()
        .await?;

    // Start the sync process in the background
    tokio::spawn(async move {
        if let Err(e) = syncoor.start().await {
            println!("Error: {e:?}");
        }
    });

    // Receive all of the sync messages
    while let Some(msg) = receiver.recv().await {
        match msg {
            SyncMessage::Logs {
                logs,
                mode,
                block_range,
            } => {
                println!(
                    "Received {} logs from {:?} mode, blocks {:?}",
                    logs.len(),
                    mode,
                    block_range
                );
            }
            SyncMessage::ModeTransition { from, to } => {
                println!("Transitioned from {:?} to {:?}", from, to);
            }
            SyncMessage::Progress {
                current_block,
                latest_block,
                is_live,
            } => {
                println!(
                    "Progress: {}/{} (live: {})",
                    current_block, latest_block, is_live
                );
            }
            SyncMessage::Error(e) => {
                eprintln!("Sync error: {}", e);
            }
        }
    }

    Ok(())
}
