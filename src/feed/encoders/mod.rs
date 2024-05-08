pub mod openh264;
use anyhow::Result;

pub use self::openh264::OpenH264FeedEncoder;

use super::FeedFrame;

#[enum_delegate::register]
pub trait FeedEncoderImpl {
    fn encode(&mut self, frame: &FeedFrame, force_keyframe: bool) -> Result<Vec<u8>>;
}

#[enum_delegate::implement(FeedEncoderImpl)]
pub enum FeedEncoder {
    OpenH264(OpenH264FeedEncoder),
}
