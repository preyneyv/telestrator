use std::{collections::HashMap, time::Duration};

use anyhow::{Context, Result};
use bytes::Bytes;
use tokio::sync::{broadcast, mpsc};

use crate::timing_stats::TimingStats;

use super::{
    encoders::{self, FeedEncoderConfig, FeedEncoderConfigImpl, FeedEncoderImpl},
    sources::{self, FeedSourceConfig, FeedSourceConfigImpl, FeedSourceImpl},
};

#[derive(Debug)]
pub struct FeedResizeResolution {
    width: u32,
    height: u32,
}

#[derive(Default)]
pub struct FeedConfigBuilder {
    /// Frame source configuration
    source: Option<FeedSourceConfig>,

    /// Frame encoder configuration
    encoder: Option<FeedEncoderConfig>,

    /// Minimum bitrate due to bandwidth-related adjustments. (kbps)
    min_bitrate: Option<u32>,
    /// Initial bitrate pre bandwidth-related adjustments. (kbps)
    start_bitrate: Option<u32>,
    /// Maximum bitrate due to bandwidth-related adjustments. (kbps)
    max_bitrate: Option<u32>,

    /// The encoding pipeline is allowed to dip below this for performance
    /// reasons. However, bandwidth-related adjustments will not go
    /// below min_fps.
    min_fps: Option<u32>,
    /// The encoding pipeline will not exceed this FPS limit.
    max_fps: Option<u32>,

    /// If specified, frames will be resized to this if they are larger.
    resolution: Option<FeedResizeResolution>,
}

impl FeedConfigBuilder {
    pub fn new() -> Self {
        Self {
            ..Default::default()
        }
    }

    pub fn source(mut self, source: FeedSourceConfig) -> Self {
        self.source = Some(source);
        self
    }

    pub fn encoder(mut self, encoder: FeedEncoderConfig) -> Self {
        self.encoder = Some(encoder);
        self
    }

    pub fn bitrate(mut self, min: u32, start: u32, max: u32) -> Self {
        self.min_bitrate = Some(min);
        self.start_bitrate = Some(start);
        self.max_bitrate = Some(max);
        self
    }

    pub fn fps(mut self, min_fps: u32, max_fps: u32) -> Self {
        self.min_fps = Some(min_fps);
        self.max_fps = Some(max_fps);
        self
    }

    pub fn resolution(mut self, resolution: FeedResizeResolution) -> Self {
        self.resolution = Some(resolution);
        self
    }

    pub fn build_interactive(self) -> Result<FeedConfig> {
        let source = match self.source {
            Some(source) => source,
            None => sources::ndi::NDIFeedSourceConfig::build_interactive()
                .context("Failed to build source config")?,
        };

        let encoder = match self.encoder {
            Some(encoder) => encoder,
            None => encoders::FeedEncoderConfig::OpenH264(Default::default()),
        };

        let min_bitrate = self.min_bitrate.unwrap_or(1000);
        let start_bitrate = self.start_bitrate.unwrap_or(6000);
        let max_bitrate = self.max_bitrate.unwrap_or(20_000);

        let min_fps = self.min_fps.unwrap_or(1000);
        let max_fps = self.max_fps.unwrap_or(6000);
        let resolution = self.resolution;

        Ok(FeedConfig {
            source,
            encoder,

            min_bitrate,
            start_bitrate,
            max_bitrate,

            max_fps,
            min_fps,

            resolution,
        })
    }
}

pub struct FeedConfig {
    source: FeedSourceConfig,
    encoder: FeedEncoderConfig,

    min_bitrate: u32,
    start_bitrate: u32,
    max_bitrate: u32,

    min_fps: u32,
    max_fps: u32,

    resolution: Option<FeedResizeResolution>,
}

#[derive(Debug)]
pub enum FeedControlMessage {
    ClientJoined { client_id: String },
    ClientLeft { client_id: String },
    RequestKeyframe,
    BandwidthEstimate { client_id: String, bitrate: u32 },
}

#[derive(Debug, Clone)]
pub enum FeedResultMessage {
    EncodedBitstream(Bytes),
}

pub struct FeedManager {
    config: FeedConfig,

    feed_control_rx: mpsc::Receiver<FeedControlMessage>,
    feed_result_tx: broadcast::Sender<FeedResultMessage>,

    client_bitrates: HashMap<String, u32>,
    target_bitrate: u32,
    force_keyframe: bool,
}

impl FeedManager {
    pub fn new(
        config: FeedConfig,
        feed_control_rx: mpsc::Receiver<FeedControlMessage>,
        feed_result_tx: broadcast::Sender<FeedResultMessage>,
    ) -> Self {
        Self {
            config,

            feed_control_rx,
            feed_result_tx,

            client_bitrates: HashMap::new(),
            target_bitrate: 0,
            force_keyframe: false,
        }
    }

    pub fn run_forever(&mut self) -> Result<()> {
        let mut stats = TimingStats::new("feed".into());

        let mut source = self.config.source.build()?;
        let mut encoder = self.config.encoder.build()?;

        loop {
            self.process_queued_control_messages();

            if self.client_bitrates.len() == 0 {
                // There's nothing to do, since we don't have any clients.
                // Just block until the next message and try again.
                self.block_until_next_message();
                continue;
            }

            // TODO: get_frame() could be block while a keyframe request comes
            // in. If so, the keyframe gets deferred for a really long time.
            let Some(frame) = source.get_frame()? else {
                continue;
            };

            stats.tick();

            let force_keyframe = std::mem::replace(&mut self.force_keyframe, false);
            let data = encoder
                .encode(&frame, force_keyframe)
                .context("failed to encode frame")?;

            self.feed_result_tx
                .send(FeedResultMessage::EncodedBitstream(data))
                .ok();
        }
    }

    /// Parse all messages in the feed control queue.
    fn process_queued_control_messages(&mut self) {
        while let Ok(message) = self.feed_control_rx.try_recv() {
            self.process_control_message(message);
        }
    }

    /// Block until a message is received on the feed control queue.
    fn block_until_next_message(&mut self) {
        if let Some(message) = self.feed_control_rx.blocking_recv() {
            self.process_control_message(message);
        }
    }

    fn process_control_message(&mut self, message: FeedControlMessage) {
        println!(">> FeedControl {:?}", message);
        match message {
            FeedControlMessage::ClientJoined { client_id } => {
                self.client_bitrates.insert(
                    client_id,
                    std::cmp::min(self.target_bitrate, self.config.start_bitrate),
                );
                // send a keyframe for the new client
                self.force_keyframe = true
            }
            FeedControlMessage::BandwidthEstimate { client_id, bitrate } => {
                self.client_bitrates.insert(client_id, bitrate);
                self.compute_target_bitrate();
            }
            FeedControlMessage::ClientLeft { client_id } => {
                self.client_bitrates.remove(&client_id);
                self.compute_target_bitrate();
            }
            FeedControlMessage::RequestKeyframe => {
                self.force_keyframe = true;
            }
        }
    }

    /// Compute the target bitrate as the minimum of all connected client
    /// bitrates. If there are no clients, the target bitrate is set to
    /// `config.start_bitrate`.
    fn compute_target_bitrate(&mut self) {
        let min_client_bitrate = self
            .client_bitrates
            .values()
            .min()
            .unwrap_or(&self.config.start_bitrate)
            .to_owned();

        let clamped = min_client_bitrate.clamp(self.config.min_bitrate, self.config.max_bitrate);
        self.target_bitrate = clamped;
    }
}
