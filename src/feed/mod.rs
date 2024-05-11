pub mod encoders;
pub mod frame;
pub mod manager;
pub mod sources;

use anyhow::Result;
use tokio::sync::{broadcast, mpsc};

use self::manager::{FeedConfig, FeedControlMessage, FeedManager, FeedResultMessage};

pub async fn main(
    config: FeedConfig,
    feed_control_rx: mpsc::Receiver<FeedControlMessage>,
    feed_result_tx: broadcast::Sender<FeedResultMessage>,
) -> Result<()> {
    tokio::task::spawn_blocking(move || {
        let mut manager = FeedManager::new(config, feed_control_rx, feed_result_tx)?;
        manager.run_forever()
    })
    .await??;

    Ok(())
}
