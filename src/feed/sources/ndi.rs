use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use bytes::Bytes;
use ndi::VideoData;

use crate::feed::frame::{VideoFrameBuffer, VideoFramePixelFormat, VideoFramerate, VideoTimestamp};

use super::{FeedSource, FeedSourceConfig, FeedSourceConfigImpl, FeedSourceImpl};

pub struct NDIFeedSourceConfig {
    source: ndi::Source,
    recv_timeout: u64,
}

impl FeedSourceConfigImpl for NDIFeedSourceConfig {
    fn build(&self) -> Result<FeedSource> {
        let source = NDIFeedSource::new(self)?;
        Ok(FeedSource::NDI(source))
    }
}

impl NDIFeedSourceConfig {
    pub fn build_interactive() -> Result<FeedSourceConfig> {
        ndi::initialize()?;
        let find = ndi::Find::new()?;
        let sources = find
            .current_sources(2000)
            .context("No NDI sources were found within the timeout")?;

        println!("Available sources:");
        sources
            .iter()
            .enumerate()
            .for_each(|(i, s)| println!("{i}) {}", s.get_name()));

        let stdin = std::io::stdin();
        let mut buf = String::new();
        stdin.read_line(&mut buf)?;
        let i = buf.trim_end().parse::<usize>()?;

        let source = sources[i].clone();

        return Ok(FeedSourceConfig::NDI(Self {
            source,
            recv_timeout: 1000,
        }));
    }
}

pub struct NDIFeedSource {
    recv: ndi::Recv,
    ndi_video_data: Option<VideoData>,
    recv_timeout: Duration,
}

impl NDIFeedSource {
    /// Construct an NDIFeedSource for the provided source
    pub fn new(config: &NDIFeedSourceConfig) -> Result<Self> {
        Ok(NDIFeedSource {
            recv: ndi::RecvBuilder::new()
                .allow_video_fields(false)
                .bandwidth(ndi::RecvBandwidth::Highest)
                .color_format(ndi::RecvColorFormat::UYVY_RGBA)
                .ndi_recv_name("Telestrator".into())
                .source_to_connect_to(config.source.clone())
                .build()
                .context("Unable to build NDI receiver")?,
            ndi_video_data: None,
            recv_timeout: Duration::from_millis(config.recv_timeout),
        })
    }
}

impl FeedSourceImpl for NDIFeedSource {
    /// Read one frame from the source.
    fn get_frame(&mut self) -> Result<Option<VideoFrameBuffer>> {
        let start = Instant::now();
        while start.elapsed() < self.recv_timeout {
            let response = self.recv.capture_video(&mut self.ndi_video_data, 100);

            match response {
                ndi::FrameType::Video => {}
                ndi::FrameType::ErrorFrame => bail!("NDI error frame received."),
                _ => continue,
            }

            let video_data = std::mem::replace(&mut self.ndi_video_data, None)
                .context("Failed to get video data from capture")?;

            let width = video_data.width() as usize;
            let height = video_data.height() as usize;

            let timestamp =
                VideoTimestamp::from_micros((video_data.timestamp().unwrap_or(0) / 10) as u64);

            let line_stride = video_data.line_stride_in_bytes().unwrap() as usize;

            // TODO: This copy sucks but we get memory errors if this copy is allowed to happen.
            let raw_frame =
                unsafe { std::slice::from_raw_parts(video_data.p_data(), height * line_stride) }
                    .to_vec();

            let pix_fmt = VideoFramePixelFormat::try_from(video_data.four_cc())
                .context("Unsupported color format {}")?;
            let data = Bytes::from(raw_frame);

            return Ok(Some(VideoFrameBuffer {
                pix_fmt,
                width,
                height,
                data,
                timestamp,
                framerate: VideoFramerate::new(
                    video_data.frame_rate_n(),
                    video_data.frame_rate_d(),
                ),
                line_stride,
            }));
        }
        return Ok(None);
    }
}

impl TryFrom<ndi::FourCCVideoType> for VideoFramePixelFormat {
    type Error = anyhow::Error;

    fn try_from(value: ndi::FourCCVideoType) -> Result<Self> {
        match value {
            // ndi::FourCCVideoType::BGRA | ndi::FourCCVideoType::BGRX => Ok(Self::BGRA),
            // ndi::FourCCVideoType::RGBA | ndi::FourCCVideoType::RGBX => Ok(Self::RGBA),
            ndi::FourCCVideoType::UYVY => Ok(Self::UYVY),
            ndi::FourCCVideoType::I420 => Ok(Self::I420),
            format => bail!("Unsupported video format {:?}", format),
        }
    }
}
