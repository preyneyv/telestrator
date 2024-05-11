use anyhow::Result;
use bytes::Bytes;

use crate::feed::frame::VideoFrameBuffer;

use super::{FeedEncoder, FeedEncoderConfigImpl, FeedEncoderImpl};

pub struct OpenH264FeedEncoderConfig {
    keyframe_interval: u32,
}

impl Default for OpenH264FeedEncoderConfig {
    fn default() -> Self {
        Self {
            keyframe_interval: 250,
        }
    }
}

impl FeedEncoderConfigImpl for OpenH264FeedEncoderConfig {
    fn build(&self) -> Result<super::FeedEncoder> {
        let source = OpenH264FeedEncoder::new(self)?;
        return Ok(FeedEncoder::OpenH264(source));
    }
}

pub struct OpenH264FeedEncoder {}

impl OpenH264FeedEncoder {
    pub fn new(config: &OpenH264FeedEncoderConfig) -> Result<Self> {
        Ok(Self {})
    }
}

impl FeedEncoderImpl for OpenH264FeedEncoder {
    fn encode(&mut self, frame: &VideoFrameBuffer, force_keyframe: bool) -> Result<Bytes> {
        Ok(Bytes::from(Vec::<u8>::new()))
    }
}
