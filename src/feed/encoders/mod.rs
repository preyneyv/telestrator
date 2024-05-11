pub mod openh264;

use anyhow::Result;
use bytes::Bytes;

use self::openh264::{OpenH264FeedEncoder, OpenH264FeedEncoderConfig};
use crate::feed::frame::VideoFrameBuffer;

#[enum_delegate::register]
pub trait FeedEncoderImpl {
    fn encode(&mut self, frame: &VideoFrameBuffer, force_keyframe: bool) -> Result<Bytes>;
}

#[enum_delegate::implement(FeedEncoderImpl)]
pub enum FeedEncoder {
    OpenH264(OpenH264FeedEncoder),
}

#[enum_delegate::register]
pub trait FeedEncoderConfigImpl {
    fn build(&self) -> Result<FeedEncoder>;
}

#[enum_delegate::implement(FeedEncoderConfigImpl)]
pub enum FeedEncoderConfig {
    OpenH264(OpenH264FeedEncoderConfig),
}
