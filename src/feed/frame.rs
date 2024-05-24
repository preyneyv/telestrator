use anyhow::Result;
use bytes::Bytes;

pub type Resolution = (u32, u32);

#[derive(Debug, Clone, PartialEq)]
pub enum VideoFramePixelFormat {
    I420,
    UYVY,
}

#[derive(Debug, Clone)]
pub struct VideoFramerate {
    pub num: u32,
    pub den: u32,
}

impl VideoFramerate {
    pub fn new(num: u32, den: u32) -> Self {
        return Self { num, den };
    }

    /// Convert the provided framerate into a single value
    pub fn ratio(&self) -> f32 {
        return self.num as f32 / self.den as f32;
    }
}

/// Represent the timecode of a video frame. Internally stores timecodes as
/// microseconds (higher resolutions are lost).
#[derive(Clone)]
pub struct VideoTimestamp(u64);
impl VideoTimestamp {
    pub fn from_micros(micros: u64) -> Self {
        Self(micros)
    }
    pub fn from_millis(millis: u64) -> Self {
        Self(millis * 1000)
    }
    pub fn to_micros(&self) -> u64 {
        self.0
    }
    pub fn to_millis(&self) -> u64 {
        self.0 / 1000
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ColorConversionError {
    #[error("ipp color conversion threw an error: {0}")]
    IPPError(i32),
}

#[derive(Clone)]
pub struct VideoFrameBuffer {
    pub pix_fmt: VideoFramePixelFormat,
    pub width: usize,
    pub height: usize,

    /// timestamp in milliseconds
    pub timestamp: VideoTimestamp,
    pub framerate: VideoFramerate,
    pub line_stride: usize,
    pub data: Bytes,
}
impl VideoFrameBuffer {
    pub fn resolution(&self) -> Resolution {
        (self.width as u32, self.height as u32)
    }

    /// Convert the provided frame to I420.
    pub fn to_i420(&self) -> Result<VideoFrameBuffer> {
        use VideoFramePixelFormat::*;

        match self.pix_fmt {
            I420 => Ok(self.clone()),
            UYVY => self.uyvy_to_i420(),
        }
    }

    fn uyvy_to_i420(&self) -> Result<VideoFrameBuffer> {
        assert!(self.pix_fmt == VideoFramePixelFormat::UYVY);
        let width = self.width;
        let height = self.height;
        let line_stride = self.line_stride;
        let data = &self.data;

        // Reserve a buffer for I420
        let mut yuv = vec![0u8; 3 * (width * height) / 2];
        let w: i32 = width as _;
        let dest_steps = [w, w / 2, w / 2];
        let dim = width * height;

        unsafe {
            let y = yuv.as_mut_ptr();
            let u = y.add(dim);
            let v = u.add(dim >> 2);
            let dest_slices = [y, u, v];

            let rv = ippi_sys::ippiCbYCr422ToYCbCr420_8u_C2P3R(
                data.as_ptr(),
                line_stride as _,
                dest_slices.as_ptr() as _,
                dest_steps.as_ptr() as _,
                ippi_sys::IppiSize {
                    width: width as _,
                    height: height as _,
                },
            );

            if rv != ippi_sys::ippStsNoErr as i32 {
                Err(ColorConversionError::IPPError(rv))?;
            }
        }

        let new_data = Bytes::from(yuv);
        return Ok(Self {
            pix_fmt: VideoFramePixelFormat::I420,
            data: new_data,
            ..self.clone()
        });
    }

    // TODO: This is ugly. Make separate structs for each color type.
    pub fn yuv_slices(&self) -> (&[u8], &[u8], &[u8]) {
        debug_assert!(self.pix_fmt == VideoFramePixelFormat::I420);
        let width = self.width;
        let height = self.height;
        let dim = width * height;

        let y = &self.data[0..dim];
        let u = &self.data[dim..dim + dim / 4];
        let v = &self.data[dim + dim / 4..];

        (y, u, v)
    }
}
