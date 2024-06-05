use std::{ffi::c_void, ptr, rc::Rc, sync::Arc};

use bytes::Bytes;
use nvidia_sys::nvencodeapi::{self as sys};

use crate::feed::{
    encoders::RateParameters,
    frame::{Resolution, VideoFrameBuffer, VideoFramePixelFormat},
};

use super::cuda;

pub use nvidia_sys::guids::{NV_ENC_CODEC as Codec, NV_ENC_PRESET as Preset};

/// Max resolution for NVENC H264.
const MAX_DIM: u32 = 4096;

trait NvencErrorCode {
    fn ok(self) -> std::result::Result<(), NvencError>;
}
#[derive(thiserror::Error, Debug)]
#[error("NVENC ERROR {code:?}")]
pub struct NvencError {
    code: sys::NVENCSTATUS,
}
impl NvencErrorCode for sys::NVENCSTATUS {
    fn ok(self) -> std::result::Result<(), NvencError> {
        match self {
            sys::NVENCSTATUS::NV_ENC_SUCCESS => Ok(()),
            code => {
                return Err(NvencError { code });
            }
        }
    }
}

type Result<T> = std::result::Result<T, NvencError>;
type NvencAPI = Rc<sys::NV_ENCODE_API_FUNCTION_LIST>;

// impl TryInto<sys::NV_ENC_BUFFER_FORMAT> for VideoFramePixelFormat {
//     type Error = anyhow::Error;
//     // fn try_into(self) -> sys::NV_ENC_BUFFER_FORMAT {
//     //     use sys::NV_ENC_BUFFER_FORMAT::*;
//     //     use VideoFramePixelFormat::*;
//     //     match self {
//     //         I420 => NV_ENC_BUFFER_FORMAT_IYUV,
//     //     }
//     // }
//     fn try_into(self) -> std::result::Result<sys::NV_ENC_BUFFER_FORMAT, Self::Error> {
//         use sys::NV_ENC_BUFFER_FORMAT::*;
//         use VideoFramePixelFormat::*;
//         Ok(match self {
//             I420 => NV_ENC_BUFFER_FORMAT_IYUV,
//             UYVY
//         })
//     }
// }

/// Throws NV_ENC_ERR_INVALID_PARAM if functions are missing from API instance.
fn make_encode_api() -> Result<sys::NV_ENCODE_API_FUNCTION_LIST> {
    let mut api = sys::NV_ENCODE_API_FUNCTION_LIST {
        version: sys::NV_ENCODE_API_FUNCTION_LIST_VER,
        ..Default::default()
    };
    unsafe {
        sys::NvEncodeAPICreateInstance(&mut api).ok()?;
    }

    (|| -> Option<()> {
        api.nvEncOpenEncodeSession?;
        api.nvEncGetEncodeGUIDCount?;
        api.nvEncGetEncodeProfileGUIDCount?;
        api.nvEncGetEncodeProfileGUIDs?;
        api.nvEncGetEncodeGUIDs?;
        api.nvEncGetInputFormatCount?;
        api.nvEncGetInputFormats?;
        api.nvEncGetEncodeCaps?;
        api.nvEncGetEncodePresetCount?;
        api.nvEncGetEncodePresetGUIDs?;
        api.nvEncGetEncodePresetConfig?;
        api.nvEncInitializeEncoder?;
        api.nvEncCreateInputBuffer?;
        api.nvEncDestroyInputBuffer?;
        api.nvEncCreateBitstreamBuffer?;
        api.nvEncDestroyBitstreamBuffer?;
        api.nvEncEncodePicture?;
        api.nvEncLockBitstream?;
        api.nvEncUnlockBitstream?;
        api.nvEncLockInputBuffer?;
        api.nvEncUnlockInputBuffer?;
        api.nvEncGetEncodeStats?;
        api.nvEncGetSequenceParams?;
        api.nvEncRegisterAsyncEvent?;
        api.nvEncUnregisterAsyncEvent?;
        api.nvEncMapInputResource?;
        api.nvEncUnmapInputResource?;
        api.nvEncDestroyEncoder?;
        api.nvEncInvalidateRefFrames?;
        api.nvEncOpenEncodeSessionEx?;
        api.nvEncRegisterResource?;
        api.nvEncUnregisterResource?;
        api.nvEncReconfigureEncoder?;
        Some(())
    })()
    .ok_or(NvencError {
        code: sys::_NVENCSTATUS::NV_ENC_ERR_INVALID_PTR,
    })?;

    Ok(api)
}

// pub struct InputBuffer {
//     api: NvencAPI,
// }
// impl InputBuffer {
//     pub fn new(api: &NvencAPI) -> Result<Self> {
//         let api = api.clone();
//         unsafe {
//             api.nvEncCreateInputBuffer.unwrap_unchecked()()
//         }
//         Ok(Self { api: api.clone() })
//     }
// }

pub struct EncodeSession {
    // tie ctx lifetime to encode session
    _ctx: Rc<cuda::Context>,

    api: NvencAPI,
    encoder_ptr: *mut c_void,
    input_buffer: Option<sys::NV_ENC_INPUT_PTR>,
    output_bitstream: Option<sys::NV_ENC_OUTPUT_PTR>,
}

impl EncodeSession {
    pub fn new(ctx: &Rc<cuda::Context>) -> anyhow::Result<Self> {
        let ctx = ctx.clone();
        let api = Rc::new(make_encode_api()?);

        let mut params = sys::NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS {
            version: sys::NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER,
            apiVersion: sys::NVENCAPI_VERSION,
            deviceType: sys::_NV_ENC_DEVICE_TYPE::NV_ENC_DEVICE_TYPE_CUDA,
            device: ctx.as_ptr().cast(),
            ..Default::default()
        };

        let encoder_ptr = unsafe {
            let mut enc: *mut c_void = ptr::null_mut();
            api.nvEncOpenEncodeSessionEx.unwrap_unchecked()(&mut params, &mut enc).ok()?;
            enc
        };
        println!("acquired encoder! {:?}", encoder_ptr);
        Ok(Self {
            _ctx: ctx,

            api,
            encoder_ptr,
            input_buffer: None,
            output_bitstream: None,
        })
    }

    pub fn get_codec_guids(&self) -> Result<Vec<sys::GUID>> {
        let mut count = 0;
        unsafe {
            self.api.nvEncGetEncodeGUIDCount.unwrap_unchecked()(self.encoder_ptr, &mut count)
                .ok()?;
        };

        let mut guids = Vec::with_capacity(count as usize);
        let mut out_count = 0;
        unsafe {
            self.api.nvEncGetEncodeGUIDs.unwrap_unchecked()(
                self.encoder_ptr,
                guids.as_mut_ptr(),
                count,
                &mut out_count,
            )
            .ok()?;
            guids.set_len(out_count as _);
        }

        Ok(guids)
    }

    pub fn supports_codec(&self, &codec_guid: &sys::GUID) -> Result<bool> {
        Ok(self.get_codec_guids()?.contains(&codec_guid))
    }

    /// Get vec of supported input formats
    pub fn get_input_formats(
        &self,
        &codec_guid: &sys::GUID,
    ) -> Result<Vec<sys::NV_ENC_BUFFER_FORMAT>> {
        let mut count = 0;
        unsafe {
            self.api.nvEncGetInputFormatCount.unwrap_unchecked()(
                self.encoder_ptr,
                codec_guid,
                &mut count,
            )
            .ok()?;
        };

        let mut formats = Vec::with_capacity(count as usize);
        let mut out_count = 0;
        unsafe {
            self.api.nvEncGetInputFormats.unwrap_unchecked()(
                self.encoder_ptr,
                codec_guid,
                formats.as_mut_ptr(),
                count,
                &mut out_count,
            )
            .ok()?;
            formats.set_len(out_count as _);
        }

        Ok(formats)
    }

    fn initialize_input_buffer(&mut self, resolution: &Resolution) -> Result<()> {
        assert_eq!(self.input_buffer, None, "input buffer already initialized");
        let mut params = sys::NV_ENC_CREATE_INPUT_BUFFER {
            version: sys::NV_ENC_CREATE_INPUT_BUFFER_VER,
            // width: MAX_DIM,
            // height: MAX_DIM,
            width: resolution.0,
            height: resolution.1,
            bufferFmt: sys::NV_ENC_BUFFER_FORMAT::NV_ENC_BUFFER_FORMAT_IYUV,

            ..Default::default()
        };
        unsafe {
            self.api.nvEncCreateInputBuffer.unwrap_unchecked()(self.encoder_ptr, &mut params)
                .ok()?;
        }
        self.input_buffer = Some(params.inputBuffer);
        Ok(())
    }

    fn initialize_output_bitstream(&mut self) -> Result<()> {
        assert_eq!(
            self.output_bitstream, None,
            "output bitstream already initialized"
        );
        let mut params = sys::NV_ENC_CREATE_BITSTREAM_BUFFER {
            version: sys::NV_ENC_CREATE_BITSTREAM_BUFFER_VER,
            ..Default::default()
        };
        unsafe {
            self.api.nvEncCreateBitstreamBuffer.unwrap_unchecked()(self.encoder_ptr, &mut params)
                .ok()?;
        }
        self.output_bitstream = Some(params.bitstreamBuffer);
        Ok(())
    }

    pub fn initialize(&mut self, resolution: &Resolution, rate: &RateParameters) -> Result<()> {
        let encode_guid = Codec::H264;
        let preset_guid = Preset::P5;
        // let mut encode_config = unsafe {
        //     let mut conf = sys::NV_ENC_PRESET_CONFIG {
        //         version: sys::NV_ENC_PRESET_CONFIG_VER,
        //         ..Default::default()
        //     };
        //     self.api.nvEncGetEncodePresetConfig.unwrap_unchecked()(
        //         self.encoder_ptr,
        //         encode_guid,
        //         preset_guid,
        //         &mut conf,
        //     )
        //     .ok()?;
        //     conf.presetCfg
        // };

        // encode_config.gopLength = sys::NVENC_INFINITE_GOPLENGTH;
        // encode_config.frameIntervalP = 1;
        // encode_config.frameFieldMode = sys::NV_ENC_PARAMS_FRAME_FIELD_MODE::NV_ENC_PARAMS_FRAME_FIELD_MODE_FRAME;
        // encode_config.rcParams.

        let mut params = sys::NV_ENC_INITIALIZE_PARAMS {
            version: sys::NV_ENC_INITIALIZE_PARAMS_VER,
            tuningInfo: sys::NV_ENC_TUNING_INFO::NV_ENC_TUNING_INFO_LOW_LATENCY,
            maxEncodeHeight: MAX_DIM,
            maxEncodeWidth: MAX_DIM,
            // encodeConfig: &mut encode_config,
            enableEncodeAsync: 0,
            enablePTD: 1,

            encodeGUID: encode_guid,
            presetGUID: preset_guid,

            darWidth: resolution.0,
            darHeight: resolution.1,
            encodeWidth: resolution.0,
            encodeHeight: resolution.1,

            // TODO: this is ugly. should store num/den separately.
            frameRateNum: rate.max_fps.round() as _,
            frameRateDen: 1,

            ..Default::default()
        };
        unsafe {
            self.api.nvEncInitializeEncoder.unwrap_unchecked()(self.encoder_ptr, &mut params)
                .ok()?
        };

        self.initialize_input_buffer(&resolution)?;
        self.initialize_output_bitstream()?;

        Ok(())
    }

    fn prep_frame_data(&self, frame: &VideoFrameBuffer) -> anyhow::Result<()> {
        assert_ne!(self.input_buffer, None, "no input buffer present");
        let input_buffer = self.input_buffer.unwrap();

        let mut params = sys::NV_ENC_LOCK_INPUT_BUFFER {
            version: sys::NV_ENC_LOCK_INPUT_BUFFER_VER,
            inputBuffer: input_buffer,
            ..Default::default()
        };
        unsafe {
            self.api.nvEncLockInputBuffer.unwrap_unchecked()(self.encoder_ptr, &mut params).ok()?
        };

        let ptr = params.bufferDataPtr;
        // println!("buffer data: {:?} p{:?}", ptr, params.pitch);

        unsafe {
            let src = frame.data.as_ptr();
            let dst = params.bufferDataPtr as *mut u8;
            let src_pitch = frame.width;
            let dst_pitch = params.pitch as usize;
            let stride_count = frame.data.len() / src_pitch;
            let dim = frame.height;
            let slices = frame.yuv_slices();

            // slice Y
            let target = slices.0.as_ptr();
            for i in 0..dim {
                std::ptr::copy::<u8>(target.add(i * src_pitch), dst.add(i * dst_pitch), src_pitch);
            }

            let dst = dst.add(dim * dst_pitch);
            let target = slices.1.as_ptr();
            for i in 0..dim / 4 {
                std::ptr::write_bytes(dst.add(i * dst_pitch), 255u8, dst_pitch / 4);
                // std::ptr::copy::<u8>(target.add(i * src_pitch), dst.add(i * dst_pitch), src_pitch);
            }
            // std::ptr::write_bytes(dst, 0u8, dst_pitch * dim / 4);

            // slice V
            // let dst = dst.add(dim / 4 * dst_pitch);
            // std::ptr::write_bytes(dst, 255u8, dst_pitch * dim / 4);

            // slice U
            // let target = slices.1.as_ptr();
            // for i in 0..dim / 4 {
            //     std::ptr::copy::<u8>(
            //         target.add(i * src_pitch / 2),
            //         dst.add((dim * 2 + i) * dst_pitch),
            //         src_pitch / 2,
            //     );
            // }

            // // slice V
            // let target = slices.2.as_ptr();
            // for i in 0..dim / 4 {
            //     std::ptr::copy::<u8>(
            //         target.add(i * src_pitch / 2),
            //         dst.add((dim * 2 + i) * dst_pitch / 2),
            //         src_pitch / 2,
            //     );
            // }

            // for i in 0..frame.height {
            //     std::ptr::copy::<u8>(src.add(i * src_pitch), dst.add(i * dst_pitch), src_pitch);
            // }
            // for i in frame.height..frame.height +
            // std::ptr::copy::<u8>(frame.data.as_ptr(), ptr.cast(), frame.data.len());

            self.api.nvEncUnlockInputBuffer.unwrap_unchecked()(self.encoder_ptr, input_buffer)
                .ok()?;
        };

        Ok(())
    }

    fn read_output_data(&self) -> anyhow::Result<Bytes> {
        assert_ne!(self.output_bitstream, None, "no output bitstream present");
        let output_bitstream = self.output_bitstream.unwrap();
        let mut params = sys::NV_ENC_LOCK_BITSTREAM {
            version: sys::NV_ENC_LOCK_BITSTREAM_VER,
            outputBitstream: output_bitstream,
            ..Default::default()
        };
        unsafe {
            self.api.nvEncLockBitstream.unwrap_unchecked()(self.encoder_ptr, &mut params).ok()?;
        }

        let ptr = params.bitstreamBufferPtr;
        let size = params.bitstreamSizeInBytes;
        let slice = unsafe { std::slice::from_raw_parts(ptr as *const u8, size as _) };
        let bytes = Bytes::copy_from_slice(slice);
        Ok(bytes)
    }

    pub fn encode_picture(
        &mut self,
        frame: &VideoFrameBuffer,
        force_keyframe: bool,
    ) -> anyhow::Result<Bytes> {
        let frame = frame.to_i420()?;
        self.prep_frame_data(&frame)?;

        let mut params = sys::NV_ENC_PIC_PARAMS {
            version: sys::NV_ENC_PIC_PARAMS_VER,
            inputWidth: frame.width as _,
            inputHeight: frame.height as _,
            inputPitch: frame.width as _,
            encodePicFlags: if force_keyframe {
                sys::NV_ENC_PIC_FLAGS::NV_ENC_PIC_FLAG_FORCEIDR as u32
                    | sys::NV_ENC_PIC_FLAGS::NV_ENC_PIC_FLAG_OUTPUT_SPSPPS as u32
            } else {
                Default::default()
            },
            inputBuffer: self.input_buffer.unwrap(),
            outputBitstream: self.output_bitstream.unwrap(),
            bufferFmt: sys::NV_ENC_BUFFER_FORMAT::NV_ENC_BUFFER_FORMAT_IYUV,
            pictureStruct: sys::NV_ENC_PIC_STRUCT::NV_ENC_PIC_STRUCT_FRAME,
            // inputTimeStamp: frame.timestamp.to_micros(),
            ..Default::default()
        };

        unsafe {
            self.api.nvEncEncodePicture.unwrap_unchecked()(self.encoder_ptr, &mut params).ok()?
        }

        self.read_output_data()
    }
}

impl Drop for EncodeSession {
    fn drop(&mut self) {
        unsafe {
            if let Some(ptr) = self.input_buffer {
                self.api.nvEncDestroyInputBuffer.unwrap_unchecked()(self.encoder_ptr, ptr);
            }
            if let Some(ptr) = self.output_bitstream {
                self.api.nvEncDestroyBitstreamBuffer.unwrap_unchecked()(self.encoder_ptr, ptr);
            }
            self.api.nvEncDestroyEncoder.unwrap_unchecked()(self.encoder_ptr);
        };
    }
}
