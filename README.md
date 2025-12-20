# Syncoor

It is extremely common to have to query the entire chain from genesis and live sync along with the tip to index certain event logs. `Syncoor` abstracts away this process and makes it very simple. I got sick of wriring this same variations of this code for nearly every project I started, hence `syncoor` was born. 

Note: `syncoor` will not track sync progress. If you are syncing from tip, you are probably storing the events in a database as well and you should also store a `last processed block` from the `Progress` message. You must set this in `SyncoorBuilder` upon each restart. 

# Example

```rust
use alloy_rpc_types::Filter;
use alloy_sol_types::SolEvent;
use evm_abi::factories::UniswapV2Factory;
use syncoor::{Result, SyncMessage, SyncoorBuilder};
use tracing::{Level, error, info};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    // Configure URLs
    let http_url = "http://localhost:8545";
    let ws_url = "ws://localhost:8546";

    // Construct your desired filter. Match against addresses, events, etc...
    let filter = Filter::new().event_signature(UniswapV2Factory::PairCreated::SIGNATURE_HASH);

    // Build Syncoor with URLs and optional configuration, then start it in the background
    let mut receiver = SyncoorBuilder::new(filter, http_url, ws_url)
        .batch_size(5_000) // Optional: default is 1000
        .from_block(22_800_000) // Optional: default is 0
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

```


## TODO - PR Welcome :)
- Make historical fetching parallel. It is extremely important to be able to optionally preserve block order for log processing
- Dynamic batching. Clients typically limit the range of blocks you can fetch in one query or the number of logs each request can hold. Often, logs are not very dense towards genesis and significantly ramp up towards the tip. This leads to underutlization of batch size early on as you must pick a size that is compatible with fetches near the tip. Dynamic batching will start a batch size as large as we can and reduce it if there is a size error closer to the tip. 
