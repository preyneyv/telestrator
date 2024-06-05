use std::{
    collections::HashMap,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use bytes::Bytes;
use tokio::sync::{broadcast, mpsc};

use crate::timing_stats::TimingStats;

use super::{
    encoders::{
        self, EncoderFrameFlags, FeedEncoder, FeedEncoderConfig, FeedEncoderConfigImpl,
        FeedEncoderImpl, RateParameters,
    },
    frame::Resolution,
    sources::{self, FeedSource, FeedSourceConfig, FeedSourceConfigImpl, FeedSourceImpl},
};

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

    /// The encoding pipeline will not exceed this FPS limit.
    max_fps: Option<f32>,

    /// If specified, frames will be resized to this if they are larger.
    resolution: Option<Resolution>,
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

    pub fn fps(mut self, max_fps: f32) -> Self {
        self.max_fps = Some(max_fps);
        self
    }

    pub fn resolution(mut self, resolution: Resolution) -> Self {
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
            // None => encoders::FeedEncoderConfig::OpenH264(Default::default()),
            None => encoders::FeedEncoderConfig::Nvenc(Default::default()),
        };

        let min_bitrate = self.min_bitrate.unwrap_or(1000);
        let start_bitrate = self.start_bitrate.unwrap_or(2000);
        let max_bitrate = self.max_bitrate.unwrap_or(0);

        let max_fps = self.max_fps.unwrap_or(60.);
        let resolution = self.resolution;

        Ok(FeedConfig {
            source,
            encoder,

            min_bitrate,
            start_bitrate,
            max_bitrate,

            max_fps,

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

    max_fps: f32,

    resolution: Option<Resolution>,
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

    source: FeedSource,
    encoder: FeedEncoder,

    feed_control_rx: mpsc::Receiver<FeedControlMessage>,
    feed_result_tx: broadcast::Sender<FeedResultMessage>,

    force_keyframe: bool,

    client_bitrates: HashMap<String, u32>,
    target_bitrate: u32,
    max_fps: f32,

    last_frame_time: Instant,
}

impl FeedManager {
    pub fn new(
        config: FeedConfig,
        feed_control_rx: mpsc::Receiver<FeedControlMessage>,
        feed_result_tx: broadcast::Sender<FeedResultMessage>,
    ) -> Result<Self> {
        let max_fps = config.max_fps;
        let target_bitrate = config.start_bitrate;

        let source = config.source.build()?;
        let encoder = config.encoder.build(RateParameters {
            max_fps,
            target_bitrate,
        })?;

        Ok(Self {
            config,

            source,
            encoder,

            feed_control_rx,
            feed_result_tx,

            force_keyframe: false,

            client_bitrates: HashMap::new(),
            target_bitrate,
            max_fps,

            last_frame_time: Instant::now(),
        })
    }

    pub fn run_forever(&mut self) -> Result<()> {
        let mut stats = TimingStats::new("feed".into());

        loop {
            self.process_queued_control_messages()?;

            if self.client_bitrates.len() == 0 {
                // There's nothing to do, since we don't have any clients.
                // Just block until the next message and try again.
                self.block_until_next_message()?;
                continue;
            }

            // TODO: get_frame() could be block while a keyframe request comes
            // in. If so, the keyframe gets deferred for a really long time.
            let Some(frame) = self.source.get_frame()? else {
                continue;
            };

            stats.tick();

            let force_keyframe = std::mem::replace(&mut self.force_keyframe, false);
            stats.start("encode");
            let data = self
                .encoder
                .encode(&frame, EncoderFrameFlags { force_keyframe })
                .context("failed to encode frame")?;
            stats.end("encode");

            stats.track("bitrate", (8 * data.len()) as _, " bits/frame");

            self.rate_limit();

            self.feed_result_tx
                .send(FeedResultMessage::EncodedBitstream(data))
                .ok();
        }
    }

    /// Sleep to meet the target FPS.
    fn rate_limit(&mut self) {
        let frame_length = Duration::from_secs_f32(1. / self.max_fps);
        let expected_deadline = self.last_frame_time + frame_length;
        thread::sleep(expected_deadline.duration_since(Instant::now()));
        self.last_frame_time = Instant::now();
    }

    /// Parse all messages in the feed control queue.
    fn process_queued_control_messages(&mut self) -> Result<()> {
        while let Ok(message) = self.feed_control_rx.try_recv() {
            self.process_control_message(message)?;
        }
        Ok(())
    }

    /// Block until a message is received on the feed control queue.
    fn block_until_next_message(&mut self) -> Result<()> {
        if let Some(message) = self.feed_control_rx.blocking_recv() {
            self.process_control_message(message)?;
        }
        Ok(())
    }

    fn process_control_message(&mut self, message: FeedControlMessage) -> Result<()> {
        match message {
            FeedControlMessage::ClientJoined { client_id } => {
                self.client_bitrates.insert(
                    client_id,
                    std::cmp::min(self.target_bitrate, self.config.start_bitrate),
                );
                self.update_target_bitrate()?;
            }
            FeedControlMessage::BandwidthEstimate { client_id, bitrate } => {
                self.client_bitrates.insert(client_id, bitrate);
                self.update_target_bitrate()?;
            }
            FeedControlMessage::ClientLeft { client_id } => {
                self.client_bitrates.remove(&client_id);
                self.update_target_bitrate()?;
            }
            FeedControlMessage::RequestKeyframe => {
                self.force_keyframe = true;
            }
        }

        Ok(())
    }

    /// Compute the target bitrate as the minimum of all connected client
    /// bitrates. If there are no clients, the target bitrate is set to
    /// `config.start_bitrate`.
    fn update_target_bitrate(&mut self) -> Result<()> {
        let min_client_bitrate = self
            .client_bitrates
            .values()
            .min()
            .unwrap_or(&self.config.start_bitrate)
            .to_owned();

        self.target_bitrate = match (self.config.min_bitrate, self.config.max_bitrate) {
            (0, 0) => min_client_bitrate,
            (0, max) => min_client_bitrate.min(max),
            (min, 0) => min_client_bitrate.max(min),
            (min, max) => min_client_bitrate.clamp(min, max),
        };

        let rate = RateParameters {
            target_bitrate: self.target_bitrate,
            max_fps: self.max_fps,
        };

        // println!("bitrate {:?}", rate);

        self.encoder
            .set_rate(rate)
            .context("unable to update rate parameters")?;
        Ok(())
    }
}
