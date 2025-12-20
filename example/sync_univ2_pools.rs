use alloy_provider::{Provider, ProviderBuilder, WsConnect};
use alloy_rpc_types::Filter;
use alloy_sol_types::SolEvent;
use evm_abi::factories::UniswapV2Factory;
use syncoor::{Result, SyncMessage, SyncoorBuilder, SyncoorError};
use tracing::{Level, error, info};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    // Configure URLs
    let http_url = "http://localhost:8545";
    let ws_url = "ws://localhost:8546";

    // Construct your desired filter. Match against addresses, events, etc...
    let filter = Filter::new().event_signature(UniswapV2Factory::PairCreated::SIGNATURE_HASH);

    // Build providers
    let http_provider = ProviderBuilder::new()
        .connect_http(http_url.parse()?)
        .erased();

    let ws_connect = WsConnect::new(ws_url);
    let ws_provider = ProviderBuilder::new()
        .connect_ws(ws_connect)
        .await
        .map_err(SyncoorError::Provider)?
        .erased();

    // Build Syncoor with providers and optional configuration
    let mut receiver = SyncoorBuilder::new(filter, http_provider, ws_provider)
        .batch_size(5_000) // Optional: default is 1000
        .from_block(24_000_000) // Optional: default is 0
        .build_and_start()
        .await?;

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
