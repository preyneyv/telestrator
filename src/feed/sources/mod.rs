pub mod ndi;

use anyhow::Result;

use self::ndi::NDIFeedSource;
use super::FeedFrame;

pub trait FeedSourceImpl {
    fn get_frame(&mut self, timeout: u64) -> Result<Option<FeedFrame>>;
}

pub trait FeedSourceConfigImpl {
    fn make_source(&self) -> Result<FeedSource>;
}

pub enum FeedSource {
    NDI(NDIFeedSource),
}
pub type FeedSourceConfig = Box<dyn FeedSourceConfigImpl + Send>;

impl FeedSourceImpl for FeedSource {
    fn get_frame(&mut self, timeout: u64) -> Result<Option<FeedFrame>> {
        match self {
            Self::NDI(src) => src.get_frame(timeout),
        }
    }
}
