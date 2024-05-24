pub mod cuda;
mod nvenc;

use std::sync::Arc;

use anyhow::{bail, Result};
use bytes::Bytes;
use nvidia_sys as nv;

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

struct NvencInnerEncoder {
    device: Arc<cuda::Device>,
    ctx: Arc<cuda::Context>,
    session: nvenc::EncodeSession,
}

impl NvencInnerEncoder {
    pub fn new(cu_idx: i32) -> Result<Self> {
        let device = Arc::from(cuda::Device::new(cu_idx)?);
        println!("Selected GPU: {}", device.name()?);

        // Ensure that NVENC is supported.
        let device_caps = device.compute_capability()?;
        if ((device_caps.0 << 4) + device_caps.1) < 0x30 {
            bail!("Selected GPU doesn't support NVENC.");
        }

        let ctx = Arc::from(cuda::Context::new(
            nv::cuda::CUctx_flags_enum::CU_CTX_SCHED_BLOCKING_SYNC,
            &device,
        )?);
        println!("CUDA CTX API Version {}", ctx.api_version()?);

        let session = nvenc::EncodeSession::new(&ctx)?;
        if !session.supports_codec(&nvenc::Codec::H264)? {
            bail!("H264 is not supported by the selected encoder.");
        }
        println!(
            "formats: {:?}",
            session.get_input_formats(&nvenc::Codec::H264)?
        );
        Ok(Self {
            device,
            ctx,
            session,
        })
    }
}

pub struct NvencFeedEncoder {
    config: NvencFeedEncoderConfig,
    encoder: NvencInnerEncoder,
}

impl NvencFeedEncoder {
    pub fn new(config: &NvencFeedEncoderConfig, rate: RateParameters) -> Result<Self> {
        let encoder = NvencInnerEncoder::new(config.cuda_idx)?;
        Ok(Self {
            config: config.to_owned(),
            encoder,
        })
    }
}

impl FeedEncoderImpl for NvencFeedEncoder {
    fn encode(
        &mut self,
        frame: &crate::feed::frame::VideoFrameBuffer,
        flags: super::EncoderFrameFlags,
    ) -> Result<bytes::Bytes> {
        Ok(Bytes::new())
    }

    fn set_rate(&mut self, rate: RateParameters) -> Result<()> {
        Ok(())
    }
}
