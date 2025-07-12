# Syncoor

It is extremely common to have to query the entire chain from genesis and live sync along with the tip to index certain event logs. `Syncoor` abstracts away this process and makes it very simple. I got sick of wriring this same variations of this code for nearly every project I started, hence `syncoor` was born. 

Note: `syncoor` will not track sync progress. If you are syncing from tip, you are probably storing the events in a database as well and you should also store a `last processed block` from the `Progress` message. You must set this in `SyncoorBuilder` upon each restart. 

# Example

```rust
#[tokio::main]
async fn main() -> Result<()> {
    // Http provider for historical sync
    let http = "http://localhost:8545";
    let http_provider = ProviderBuilder::default()
        .connect_http(http.parse()?)
        .erased();

    // Ws Provider for live sync
    let ws = WsConnect::new("ws://localhost:8546");
    let ws_provider = ProviderBuilder::new().connect_ws(ws).await?.erased();

    // Construct your desired filter. Match against addresses, events, etc...
    let filter = Filter::new().event_signature(UniswapV2Factory::PairCreated::SIGNATURE_HASH);

    // Build Syncoor with both providers and optional configuration
    let (mut syncoor, mut receiver) = SyncoorBuilder::new(filter, http_provider, ws_provider)
        .batch_size(5_000) // Optional: default is 1000
        .from_block(22_800_000) // Optional: default is 0
        .build();

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

```


## TODO - PR Welcome :)
- Make historical fetching parallel. It is extremely important to be able to optionally preserve block order for log processing
- Dynamic batching. Clients typically limit the range of blocks you can fetch in one query or the number of logs each request can hold. Often, logs are not very dense towards genesis and significantly ramp up towards the tip. This leads to underutlization of batch size early on as you must pick a size that is compatible with fetches near the tip. Dynamic batching will start a batch size as large as we can and reduce it if there is a size error closer to the tip. 
