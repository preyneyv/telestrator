use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

use crate::{
    feed::{FeedColorType, FeedFrame},
    ffi,
};

use super::{FeedSource, FeedSourceConfig, FeedSourceConfigImpl, FeedSourceImpl};

pub struct NDIFeedSourceConfig {
    source: ndi::Source,
}

impl FeedSourceConfigImpl for NDIFeedSourceConfig {
    fn make_source(&self) -> Result<FeedSource> {
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

        return Ok(Box::new(Self { source }));
    }
}

pub struct NDIFeedSource {
    recv: ndi::Recv,
    // window: minifb::Window,
}

impl NDIFeedSource {
    /// Construct an NDIFeedSource for the provided source
    pub fn new(config: &NDIFeedSourceConfig) -> Result<Self> {
        // let window = minifb::Window::new("ndi dbg", 1280, 720, Default::default())
        //     .context("unable to create dbg window")?;
        Ok(NDIFeedSource {
            recv: ndi::RecvBuilder::new()
                .allow_video_fields(false)
                .bandwidth(ndi::RecvBandwidth::Highest)
                .color_format(ndi::RecvColorFormat::UYVY_RGBA)
                .ndi_recv_name("Telestrator".into())
                .source_to_connect_to(config.source.clone())
                .build()
                .context("Unable to build NDI receiver")?,
            // window,
        })
    }
}

impl FeedSourceImpl for NDIFeedSource {
    /// Read one frame from the source.
    fn get_frame(&mut self, timeout: u64) -> Result<Option<FeedFrame>> {
        let timeout = Duration::from_millis(timeout);
        let start = Instant::now();
        while start.elapsed() < timeout {
            let mut data = None;
            let response = self.recv.capture_video(&mut data, 1000);

            match response {
                ndi::FrameType::Video => {}
                ndi::FrameType::ErrorFrame => bail!("NDI error frame received."),
                _ => continue,
            }

            let data = data.context("Failed to get video data from capture")?;
            let width = data.width() as usize;
            let height = data.height() as usize;
            let pts = data.timecode();

            let line_stride = data.line_stride_in_bytes().unwrap() as usize;
            let raw_frame: Box<[u8]> =
                unsafe { std::slice::from_raw_parts(data.p_data(), height * line_stride) }.into();

            let mut color =
                FeedColorType::try_from(data.four_cc()).context("Unsupported color format {}")?;

            let data = match color {
                // FeedColorType::RGBA => raw_frame,
                FeedColorType::UYVY => {
                    let mut yuv = vec![0u8; 3 * (width * height) / 2].into_boxed_slice();
                    let w: i32 = width as _;
                    let dest_steps = [w, w / 2, w / 2];
                    let dim = width * height;
                    unsafe {
                        let y = yuv.as_mut_ptr();
                        let u = y.add(dim);
                        let v = u.add(dim >> 2);
                        let dest_slices = [y, u, v];

                        let rv = ffi::ippi::ippiCbYCr422ToYCbCr420_8u_C2P3R(
                            raw_frame.as_ptr(),
                            line_stride as _,
                            dest_slices.as_ptr() as _,
                            dest_steps.as_ptr() as _,
                            ffi::ippi::IppiSize {
                                width: width as _,
                                height: height as _,
                            },
                        );

                        if rv != ffi::ippi::ippStsNoErr as i32 {
                            bail!("Received error from IPP: {}", rv);
                        }
                    }
                    color = FeedColorType::I420;
                    yuv
                }
                _ => bail!("Unsupported source color format {color:?}"),
            };

            return Ok(Some(FeedFrame {
                color,
                width,
                height,
                data,
                pts,
            }));
        }
        return Ok(None);
    }
}

impl TryFrom<ndi::FourCCVideoType> for FeedColorType {
    type Error = anyhow::Error;

    fn try_from(value: ndi::FourCCVideoType) -> Result<Self> {
        match value {
            ndi::FourCCVideoType::BGRA | ndi::FourCCVideoType::BGRX => Ok(Self::BGRA),
            ndi::FourCCVideoType::RGBA | ndi::FourCCVideoType::RGBX => Ok(Self::RGBA),
            ndi::FourCCVideoType::UYVY => Ok(Self::UYVY),
            ndi::FourCCVideoType::I420 => Ok(Self::I420),
            format => bail!("Unsupported video format {:?}", format),
        }
    }
}
