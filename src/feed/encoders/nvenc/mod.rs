pub mod cuda;
mod nvenc;

use std::{rc::Rc, sync::Arc};

use anyhow::{bail, Context, Result};
use bytes::Bytes;
use nvidia_sys as nv;

use crate::feed::frame::Resolution;

use super::{FeedEncoder, FeedEncoderConfigImpl, FeedEncoderImpl, RateParameters};

#[derive(Clone, Default)]
pub struct NvencFeedEncoderConfig {
    keyframe_interval: u32,
    cuda_idx: i32,
}

impl FeedEncoderConfigImpl for NvencFeedEncoderConfig {
    fn build(&self, rate: RateParameters) -> Result<super::FeedEncoder> {
        if self.cuda_idx >= cuda::Device::count()? {
            bail!("non existent CUDA device");
        }

        let source = NvencFeedEncoder::new(self, rate)?;
        return Ok(FeedEncoder::Nvenc(source));
    }
}

pub struct NvencFeedEncoder {
    config: NvencFeedEncoderConfig,
    encoder: nvenc::EncodeSession,
    previous_resolution: Option<Resolution>,
    previous_rate: RateParameters,
}

impl NvencFeedEncoder {
    pub fn new(config: &NvencFeedEncoderConfig, rate: RateParameters) -> Result<Self> {
        let device = Rc::from(cuda::Device::new(config.cuda_idx)?);
        println!("Selected GPU: {}", device.name()?);

        // Ensure that NVENC is supported.
        let device_caps = device.compute_capability()?;
        if ((device_caps.0 << 4) + device_caps.1) < 0x30 {
            bail!("Selected GPU doesn't support NVENC.");
        }

        let ctx = Rc::from(cuda::Context::new(
            nv::cuda::CUctx_flags_enum::CU_CTX_SCHED_BLOCKING_SYNC,
            &device,
        )?);
        println!("CUDA CTX API Version {}", ctx.api_version()?);

        let encoder = nvenc::EncodeSession::new(&ctx)?;
        if !encoder.supports_codec(&nvenc::Codec::H264)? {
            bail!("H264 is not supported by the selected encoder.");
        }

        Ok(Self {
            config: config.to_owned(),
            encoder,
            previous_rate: rate,
            previous_resolution: None,
        })
    }
}

impl FeedEncoderImpl for NvencFeedEncoder {
    fn encode(
        &mut self,
        frame: &crate::feed::frame::VideoFrameBuffer,
        mut flags: super::EncoderFrameFlags,
    ) -> Result<bytes::Bytes> {
        let resolution = frame.resolution();
        if self.previous_resolution != Some(resolution) {
            if self.previous_resolution.is_none() {
                // First initialization
                self.encoder
                    .initialize(&resolution, &self.previous_rate)
                    .context("couldn't initialize encoder.")?;
            } else {
                self.encoder
                    .update_rate(&resolution, &self.previous_rate, true)
                    .context("couldn't update rates")?;
            }

            self.previous_resolution = Some(resolution);
            flags.force_keyframe = true;
        }

        let bytes = self
            .encoder
            .encode_picture(&frame, flags.force_keyframe)
            .context("couldn't encode frame")?;

        Ok(bytes)
    }

    fn set_rate(&mut self, rate: RateParameters) -> Result<()> {
        if self.previous_resolution.is_none() {
            self.previous_rate = rate;
            return Ok(());
        }

        self.encoder
            .update_rate(&self.previous_resolution.as_ref().unwrap(), &rate, false)
            .context("unable to update rates")?;

        self.previous_rate = rate;
        Ok(())
    }
}
