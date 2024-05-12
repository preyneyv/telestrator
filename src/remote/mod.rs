mod wrtc;

use anyhow::Result;
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

use crate::feed::manager::{FeedControlMessage, FeedResultMessage};

pub async fn main(
    feed_control_tx: mpsc::Sender<FeedControlMessage>,
    feed_result_tx: broadcast::Sender<FeedResultMessage>,
) -> Result<()> {
    wrtc::run_webrtc_tasks(feed_control_tx, feed_result_tx).await?;
    // let client_id = Uuid::new_v4().to_string();
    // feed_control_tx
    //     .send(FeedControlMessage::ClientJoined {
    //         client_id: client_id.clone(),
    //     })
    //     .await?;
    Ok(())
}
