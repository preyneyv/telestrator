mod feed;
mod ffi;
mod remote;
mod timing_stats;

use anyhow::{Context, Result};

use feed::manager::{FeedConfigBuilder, FeedControlMessage, FeedResultMessage};
use tokio::{
    sync::{broadcast, mpsc},
    try_join,
};

#[tokio::main]
async fn main() -> Result<()> {
    let config = FeedConfigBuilder::new()
        .build_interactive()
        .context("unable to build config")?;
    let (feed_control_tx, feed_control_rx) = mpsc::channel::<FeedControlMessage>(64);
    let feed_result_tx = broadcast::Sender::<FeedResultMessage>::new(1);

    try_join!(
        feed::main(config, feed_control_rx, feed_result_tx.clone()),
        remote::main(feed_control_tx.clone(), feed_result_tx.clone()),
    )?;

    Ok(())
}
