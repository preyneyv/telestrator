mod wrtc;

use anyhow::Result;
use tokio::sync::{broadcast, mpsc};

use crate::{feed::FeedControlMessage, BoxedBitstream};

pub async fn main(
    frame_ready_tx: broadcast::Sender<BoxedBitstream>,
    feed_control_tx: mpsc::Sender<FeedControlMessage>,
) -> Result<()> {
    wrtc::run_webrtc_tasks(frame_ready_tx.clone(), feed_control_tx).await?;

    Ok(())
}
