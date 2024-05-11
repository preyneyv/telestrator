use anyhow::{bail, Context, Result};
use openh264::{formats::YUVSource, Timestamp};

use crate::feed::{FeedColorType, FeedFrame};

use super::{FeedEncoder, FeedEncoderImpl};

pub struct OpenH264FeedEncoder {
    encoder: openh264::encoder::Encoder,
    frame_count: u16,
}

impl OpenH264FeedEncoder {
    pub fn new() -> Result<FeedEncoder> {
        let config = openh264::encoder::EncoderConfig::new();
        // .enable_skip_frame(true)
        // .rate_control_mode(openh264::encoder::RateControlMode::Timestamp);

        let api = openh264::OpenH264API::from_source();

        let encoder = openh264::encoder::Encoder::with_api_config(api, config)?;

        Ok(FeedEncoder::OpenH264(Self {
            encoder,
            frame_count: 0,
        }))
    }
}

impl YUVSource for &FeedFrame {
    fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    fn strides(&self) -> (usize, usize, usize) {
        (self.width, self.width >> 1, self.width >> 1)
    }

    fn y(&self) -> &[u8] {
        &self.data[0..self.width * self.height]
    }

    fn u(&self) -> &[u8] {
        let base_u = self.width * self.height;
        &self.data[base_u..base_u + base_u / 4]
    }

    fn v(&self) -> &[u8] {
        let base_u = self.width * self.height;
        let base_v = base_u + base_u / 4;
        &self.data[base_v..]
    }
}

impl FeedEncoderImpl for OpenH264FeedEncoder {
    fn encode(&mut self, frame: &FeedFrame, force_keyframe: bool) -> Result<Vec<u8>> {
        let yuv_source = match frame.color {
            FeedColorType::I420 => frame,
            // TODO: add support for UYVY, I420, etc.
            _ => bail!("Unsupported encoder color format {:?}", frame.color),
        };

        if force_keyframe {
            unsafe { self.encoder.raw_api().force_intra_frame(true) };
        } else if self.frame_count % 240 == 0 {
            unsafe { self.encoder.raw_api().force_intra_frame(false) };
            self.frame_count = 0;
        }
        self.frame_count += 1;
        let bitstream = self
            .encoder
            .encode_at(&yuv_source, Timestamp::from_millis((frame.pts / 100) as _))
            .context("Failed to encode frame")?;

        let bytes = bitstream.to_vec();

        Ok(bytes)
    }
}
