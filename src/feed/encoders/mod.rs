pub mod nvenc;
pub mod openh264;

use anyhow::Result;
use bytes::Bytes;

use self::nvenc::{NvencFeedEncoder, NvencFeedEncoderConfig};
use self::openh264::{OpenH264FeedEncoder, OpenH264FeedEncoderConfig};
use crate::feed::frame::VideoFrameBuffer;

#[enum_delegate::register]
pub trait FeedEncoderImpl {
    fn encode(&mut self, frame: &VideoFrameBuffer, flags: EncoderFrameFlags) -> Result<Bytes>;
    fn set_rate(&mut self, rate: RateParameters) -> Result<()>;
}

#[enum_delegate::implement(FeedEncoderImpl)]
pub enum FeedEncoder {
    OpenH264(OpenH264FeedEncoder),
    Nvenc(NvencFeedEncoder),
}

#[enum_delegate::register]
pub trait FeedEncoderConfigImpl {
    fn build(&self, rate: RateParameters) -> Result<FeedEncoder>;
}

#[enum_delegate::implement(FeedEncoderConfigImpl)]
pub enum FeedEncoderConfig {
    OpenH264(OpenH264FeedEncoderConfig),
    Nvenc(NvencFeedEncoderConfig),
}

#[derive(Default, Debug)]
pub struct EncoderFrameFlags {
    pub force_keyframe: bool,
}

#[derive(Debug)]
pub struct RateParameters {
    pub target_bitrate: u32,
    pub max_fps: f32,
}
