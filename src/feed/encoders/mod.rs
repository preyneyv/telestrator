pub mod nvenc;
pub mod openh264;

use anyhow::Result;
use bytes::Bytes;

use self::nvenc::{NvencFeedEncoder, NvencFeedEncoderConfig};
use self::openh264::{OpenH264FeedEncoder, OpenH264FeedEncoderConfig};
use crate::feed::frame::VideoFrameBuffer;

pub trait FeedEncoderImpl {
    fn encode(&mut self, frame: &VideoFrameBuffer, flags: EncoderFrameFlags) -> Result<Bytes>;
    fn set_rate(&mut self, rate: RateParameters) -> Result<()>;
}

pub enum FeedEncoder {
    OpenH264(OpenH264FeedEncoder),
    Nvenc(NvencFeedEncoder),
}
impl FeedEncoderImpl for FeedEncoder {
    fn encode(&mut self, frame: &VideoFrameBuffer, flags: EncoderFrameFlags) -> Result<Bytes> {
        match self {
            Self::OpenH264(enc) => enc.encode(frame, flags),
            Self::Nvenc(enc) => enc.encode(frame, flags),
        }
    }
    fn set_rate(&mut self, rate: RateParameters) -> Result<()> {
        match self {
            Self::OpenH264(enc) => enc.set_rate(rate),
            Self::Nvenc(enc) => enc.set_rate(rate),
        }
    }
}

pub trait FeedEncoderConfigImpl {
    fn build(&self, rate: RateParameters) -> Result<FeedEncoder>;
}

pub enum FeedEncoderConfig {
    OpenH264(OpenH264FeedEncoderConfig),
    Nvenc(NvencFeedEncoderConfig),
}
impl FeedEncoderConfigImpl for FeedEncoderConfig {
    fn build(&self, rate: RateParameters) -> Result<FeedEncoder> {
        match self {
            Self::Nvenc(cfg) => cfg.build(rate),
            Self::OpenH264(cfg) => cfg.build(rate),
        }
    }
}

#[derive(Default, Debug)]
pub struct EncoderFrameFlags {
    pub force_keyframe: bool,
}

#[derive(Debug, PartialEq)]
pub struct RateParameters {
    pub target_bitrate: u32,
    pub max_fps: f32,
}
