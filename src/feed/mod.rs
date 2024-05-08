pub mod encoders;
pub mod sources;

use std::{thread::sleep, time::Duration};

use anyhow::{Context, Result};
use bytes::Bytes;
use tokio::sync::{broadcast, mpsc};

use crate::{
    feed::{encoders::FeedEncoderImpl, sources::FeedSourceImpl},
    BoxedBitstream,
};

#[derive(Debug)]
pub enum FeedColorType {
    I420,
    UYVY,
    RGBA,
    BGRA,
}

#[derive(Debug)]
pub struct FeedFrame {
    pub color: FeedColorType,
    pub width: usize,
    pub height: usize,
    pub data: Box<[u8]>,
    pub pts: i64,
}

#[derive(Debug)]
pub enum FeedControlMessage {
    RequestKeyframe,
}

fn main_loop(
    source_config: sources::FeedSourceConfig,
    frame_ready_tx: broadcast::Sender<BoxedBitstream>,
    mut feed_control_rx: mpsc::Receiver<FeedControlMessage>,
) -> Result<()> {
    let mut source = source_config.make_source()?;
    let mut encoder = encoders::OpenH264FeedEncoder::new()?;
    let mut force_keyframe = true;

    loop {
        // Parse feed control messages before rendering next frame
        while let Ok(message) = feed_control_rx.try_recv() {
            println!(">> FeedControl {:?}", message);
            match message {
                FeedControlMessage::RequestKeyframe => force_keyframe = true,
            }
        }
        if frame_ready_tx.receiver_count() == 0 {
            sleep(Duration::from_millis(100));
            // println!("No available receivers. Sleeping.");
            continue;
        }

        let Some(frame) = source.get_frame(1000)? else {
            continue;
        };

        let data = encoder.encode(&frame, force_keyframe)?.into_boxed_slice();
        // force_keyframe = false;

        frame_ready_tx.send(Bytes::from(data)).ok();
    }
}

pub async fn main(
    frame_ready_tx: broadcast::Sender<BoxedBitstream>,
    feed_control_rx: mpsc::Receiver<FeedControlMessage>,
) -> Result<()> {
    // let source = source::NDIFeedSource::build_interactive().context("Failed to build source")?;
    let source_config = sources::ndi::NDIFeedSourceConfig::build_interactive()
        .context("Failed to build source config")?;
    tokio::task::spawn_blocking(move || main_loop(source_config, frame_ready_tx, feed_control_rx))
        .await??;

    Ok(())
}
