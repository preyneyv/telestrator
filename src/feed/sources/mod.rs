pub mod ndi;

use anyhow::Result;

use self::ndi::NDIFeedSource;

use super::frame::VideoFrameBuffer;

#[enum_delegate::register]
pub trait FeedSourceImpl {
    fn get_frame(&mut self) -> Result<Option<VideoFrameBuffer>>;
}

#[enum_delegate::implement(FeedSourceImpl)]
pub enum FeedSource {
    NDI(NDIFeedSource),
}

#[enum_delegate::register]
pub trait FeedSourceConfigImpl {
    fn build(&self) -> Result<FeedSource>;
}

#[enum_delegate::implement(FeedSourceConfigImpl)]
pub enum FeedSourceConfig {
    NDI(ndi::NDIFeedSourceConfig),
}
