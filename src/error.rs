use super::SyncMessage;
use alloy_provider::transport::TransportError;
use thiserror::Error;
use tokio::sync::mpsc::error::SendError;

pub type Result<T> = std::result::Result<T, SyncoorError>;

/// Syncoor error types
#[derive(Error, Debug)]
pub enum SyncoorError {
    #[error("Failed to get latest block")]
    GetLatestBlock(#[source] TransportError),

    #[error("Failed to get latest block during historical sync")]
    GetLatestBlockHistorical(#[source] TransportError),

    #[error("Failed to send mode transition")]
    SendModeTransition(#[source] SendError<SyncMessage>),

    #[error("Failed to send logs")]
    SendLogs(#[source] SendError<SyncMessage>),

    #[error("Failed to get logs for blocks {start}-{end}: {source}")]
    GetLogs {
        start: u64,
        end: u64,
        #[source]
        source: TransportError,
    },

    #[error("Failed to subscribe to logs. Make sure you're using a WebSocket provider.")]
    SubscribeLogs(#[source] TransportError),

    #[error("Received log without block number")]
    MissingBlockNumber,

    #[error("Failed to send live log")]
    SendLiveLog(#[source] SendError<SyncMessage>),

    #[error("Failed to send progress")]
    SendProgress(#[source] SendError<SyncMessage>),

    #[error("Log subscription stream ended unexpectedly")]
    StreamEnded,

    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),

    #[error(transparent)]
    UrlParse(#[from] url::ParseError),

    #[error("Provider error")]
    Provider(#[source] TransportError),
}
