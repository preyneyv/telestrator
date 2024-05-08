mod feed;
mod ffi;
mod remote;

use anyhow::Result;
use bytes::Bytes;
use feed::FeedControlMessage;
use tokio::{
    sync::{broadcast, mpsc},
    try_join,
};

type BoxedBitstream = Bytes;

#[tokio::main]
async fn main() -> Result<()> {
    let (feed_control_tx, feed_control_rx) = mpsc::channel::<FeedControlMessage>(64);
    let frame_ready_tx = broadcast::Sender::<BoxedBitstream>::new(1);

    try_join!(
        feed::main(frame_ready_tx.clone(), feed_control_rx),
        remote::main(frame_ready_tx.clone(), feed_control_tx)
    )?;

    Ok(())
}
