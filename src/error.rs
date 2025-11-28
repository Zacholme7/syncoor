use thiserror::Error;

/// Error type for Syncoor operations
#[derive(Debug, Error, Clone)]
pub enum SyncoorError {
    #[error("Failed to get latest block: {0}")]
    LatestBlockFetch(String),

    #[error("Failed to get latest block during historical sync: {0}")]
    HistoricalLatestBlock(String),

    #[error("Failed to get logs for blocks {start}-{end}: {reason}")]
    HistoricalLogs {
        start: u64,
        end: u64,
        reason: String,
    },

    #[error("Failed to send logs: {0}")]
    SendLogs(String),

    #[error("Failed to fetch logs for live block {block}: {reason}")]
    LiveLogs { block: u64, reason: String },

    #[error("Failed to subscribe to blocks: {0}. Make sure you're using a WebSocket provider.")]
    Subscribe(String),

    #[error("Failed to send live log: {0}")]
    SendLiveLog(String),

    #[error("Failed to send progress: {0}")]
    SendProgress(String),

    #[error("Log subscription stream ended unexpectedly")]
    StreamEnded,

    #[error("Failed to build HTTP client: {0}")]
    HttpClientBuild(String),

    #[error("Failed to parse HTTP URL: {0}")]
    HttpUrlParse(String),

    #[error("Failed to connect HTTP provider: {0}")]
    HttpProviderConnect(String),

    #[error("Failed to connect WebSocket provider: {0}")]
    WsProviderConnect(String),
}

pub type Result<T> = std::result::Result<T, SyncoorError>;
