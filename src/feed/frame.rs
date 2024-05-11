use crate::ffi;
use anyhow::Result;
use bytes::Bytes;

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

#[derive(Clone)]
pub struct VideoFrameBuffer {
    pub pix_fmt: VideoFramePixelFormat,
    pub width: usize,
    pub height: usize,
    pub timecode: i64,
    pub framerate: VideoFramerate,
    pub line_stride: usize,
    pub data: Bytes,
}

#[derive(thiserror::Error, Debug)]
pub enum ColorConversionError {
    #[error("ipp color conversion threw an error: {0}")]
    IPPError(i32),
}

impl VideoFrameBuffer {
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

            let rv = ffi::ippi::ippiCbYCr422ToYCbCr420_8u_C2P3R(
                data.as_ptr(),
                line_stride as _,
                dest_slices.as_ptr() as _,
                dest_steps.as_ptr() as _,
                ffi::ippi::IppiSize {
                    width: width as _,
                    height: height as _,
                },
            );

            if rv != ffi::ippi::ippStsNoErr as i32 {
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
}
