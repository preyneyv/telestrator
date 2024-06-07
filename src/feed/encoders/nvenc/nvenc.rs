use std::{ffi::c_void, ops::Deref, ptr, rc::Rc};

use anyhow::Context;
use bytes::Bytes;
use nvidia_sys::nvencodeapi::{self as sys};

use crate::feed::{
    encoders::RateParameters,
    frame::{Resolution, VideoFrameBuffer},
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
type EncoderAPI = Rc<InnerEncoderAPI>;

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

struct InnerEncoderAPI {
    // tie ctx lifetime to encode session
    _ctx: Rc<cuda::Context>,

    api: sys::NV_ENCODE_API_FUNCTION_LIST,
    raw: *mut c_void,
}
impl InnerEncoderAPI {
    pub fn new(ctx: &Rc<cuda::Context>) -> Result<Self> {
        let api = make_encode_api()?;
        let mut params = sys::NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS {
            version: sys::NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER,
            apiVersion: sys::NVENCAPI_VERSION,
            deviceType: sys::_NV_ENC_DEVICE_TYPE::NV_ENC_DEVICE_TYPE_CUDA,
            device: (*ctx.as_ptr()).cast(),
            ..Default::default()
        };
        let ptr = unsafe {
            let mut enc: *mut c_void = ptr::null_mut();
            api.nvEncOpenEncodeSessionEx.unwrap_unchecked()(&mut params, &mut enc).ok()?;
            enc
        };
        Ok(Self {
            _ctx: ctx.clone(),
            api,
            raw: ptr,
        })
    }

    pub unsafe fn create_input_buffer(
        &self,
        params: &mut sys::NV_ENC_CREATE_INPUT_BUFFER,
    ) -> Result<()> {
        params.version = sys::NV_ENC_CREATE_INPUT_BUFFER_VER;
        self.api.nvEncCreateInputBuffer.unwrap_unchecked()(self.raw, params).ok()
    }

    pub unsafe fn lock_input_buffer(
        &self,
        params: &mut sys::NV_ENC_LOCK_INPUT_BUFFER,
    ) -> Result<()> {
        params.version = sys::NV_ENC_LOCK_INPUT_BUFFER_VER;
        self.api.nvEncLockInputBuffer.unwrap_unchecked()(self.raw, params).ok()
    }

    pub unsafe fn unlock_input_buffer(&self, buffer: *mut c_void) -> Result<()> {
        self.api.nvEncUnlockInputBuffer.unwrap_unchecked()(self.raw, buffer).ok()
    }

    pub unsafe fn destroy_input_buffer(&self, buffer: *mut c_void) -> Result<()> {
        self.api.nvEncDestroyInputBuffer.unwrap_unchecked()(self.raw, buffer).ok()
    }

    pub unsafe fn create_bitstream_buffer(
        &self,
        params: &mut sys::NV_ENC_CREATE_BITSTREAM_BUFFER,
    ) -> Result<()> {
        params.version = sys::NV_ENC_CREATE_BITSTREAM_BUFFER_VER;
        self.api.nvEncCreateBitstreamBuffer.unwrap_unchecked()(self.raw, params).ok()
    }

    pub unsafe fn lock_bitstream(&self, params: &mut sys::NV_ENC_LOCK_BITSTREAM) -> Result<()> {
        params.version = sys::NV_ENC_LOCK_BITSTREAM_VER;
        self.api.nvEncLockBitstream.unwrap_unchecked()(self.raw, params).ok()
    }

    pub unsafe fn unlock_bitstream(&self, buffer: *mut c_void) -> Result<()> {
        self.api.nvEncUnlockBitstream.unwrap_unchecked()(self.raw, buffer).ok()
    }

    pub unsafe fn destroy_bitstream_buffer(&self, buffer: *mut c_void) -> Result<()> {
        self.api.nvEncDestroyBitstreamBuffer.unwrap_unchecked()(self.raw, buffer).ok()
    }

    pub unsafe fn get_encode_preset_config_ex(
        &self,
        encode_guid: sys::GUID,
        preset_guid: sys::GUID,
        tuning: sys::NV_ENC_TUNING_INFO,
        params: &mut sys::NV_ENC_PRESET_CONFIG,
    ) -> Result<()> {
        params.version = sys::NV_ENC_PRESET_CONFIG_VER;
        params.presetCfg.version = sys::NV_ENC_CONFIG_VER;
        self.api.nvEncGetEncodePresetConfigEx.unwrap_unchecked()(
            self.raw,
            encode_guid,
            preset_guid,
            tuning,
            params,
        )
        .ok()
    }

    pub unsafe fn initialize_encoder(
        &self,
        params: &mut sys::NV_ENC_INITIALIZE_PARAMS,
    ) -> Result<()> {
        params.version = sys::NV_ENC_INITIALIZE_PARAMS_VER;
        self.api.nvEncInitializeEncoder.unwrap_unchecked()(self.raw, params).ok()
    }

    pub unsafe fn reconfigure_encoder(
        &self,
        params: &mut sys::NV_ENC_RECONFIGURE_PARAMS,
    ) -> Result<()> {
        params.version = sys::NV_ENC_RECONFIGURE_PARAMS_VER;
        self.api.nvEncReconfigureEncoder.unwrap_unchecked()(self.raw, params).ok()
    }

    pub unsafe fn encode_picture(&self, params: &mut sys::NV_ENC_PIC_PARAMS) -> Result<()> {
        params.version = sys::NV_ENC_PIC_PARAMS_VER;
        self.api.nvEncEncodePicture.unwrap_unchecked()(self.raw, params).ok()
    }

    pub fn get_codec_guids(&self) -> Result<Vec<sys::GUID>> {
        let mut count = 0;
        unsafe {
            self.api.nvEncGetEncodeGUIDCount.unwrap_unchecked()(self.raw, &mut count).ok()?;
        };

        let mut guids = Vec::with_capacity(count as usize);
        let mut out_count = 0;
        unsafe {
            self.api.nvEncGetEncodeGUIDs.unwrap_unchecked()(
                self.raw,
                guids.as_mut_ptr(),
                count,
                &mut out_count,
            )
            .ok()?;
            guids.set_len(out_count as _);
        }

        Ok(guids)
    }
    /// Get vec of supported input formats
    pub fn get_input_formats(
        &self,
        &codec_guid: &sys::GUID,
    ) -> Result<Vec<sys::NV_ENC_BUFFER_FORMAT>> {
        let mut count = 0;
        unsafe {
            self.api.nvEncGetInputFormatCount.unwrap_unchecked()(self.raw, codec_guid, &mut count)
                .ok()?;
        };

        let mut formats = Vec::with_capacity(count as usize);
        let mut out_count = 0;
        unsafe {
            self.api.nvEncGetInputFormats.unwrap_unchecked()(
                self.raw,
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
}

impl Deref for InnerEncoderAPI {
    type Target = *mut c_void;
    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

impl Drop for InnerEncoderAPI {
    fn drop(&mut self) {
        unsafe {
            self.api.nvEncDestroyEncoder.unwrap_unchecked()(self.raw)
                .ok()
                .unwrap_or_else(|e| eprintln!("unable to destroy encoder: {}", e))
        };
    }
}

struct InputBufferGuard {
    buf_ptr: sys::NV_ENC_INPUT_PTR,
    api: EncoderAPI,

    raw: *mut u8,
    pub pitch: u32,
}
impl InputBufferGuard {
    fn lock_buffer(buffer: &InputBuffer) -> Result<Self> {
        let api = &buffer.api;
        let mut params = sys::NV_ENC_LOCK_INPUT_BUFFER {
            inputBuffer: **buffer,
            ..Default::default()
        };
        unsafe { api.lock_input_buffer(&mut params)? };
        Ok(Self {
            api: api.clone(),
            buf_ptr: **buffer,
            raw: params.bufferDataPtr as *mut u8,
            pitch: params.pitch,
        })
    }
}
impl Deref for InputBufferGuard {
    type Target = *mut u8;
    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}
impl Drop for InputBufferGuard {
    fn drop(&mut self) {
        unsafe {
            self.api
                .unlock_input_buffer(self.buf_ptr)
                .unwrap_or_else(|e| eprintln!("unable to unlock input buffer: {}", e))
        };
    }
}

struct InputBuffer {
    api: EncoderAPI,
    raw: sys::NV_ENC_INPUT_PTR,
    pub resolution: Resolution,
}
impl InputBuffer {
    pub fn new(api: &EncoderAPI, resolution: &Resolution) -> Result<Self> {
        let api = api.clone();
        let mut params = sys::NV_ENC_CREATE_INPUT_BUFFER {
            width: resolution.0,
            height: resolution.1,
            bufferFmt: sys::NV_ENC_BUFFER_FORMAT::NV_ENC_BUFFER_FORMAT_IYUV,

            ..Default::default()
        };
        unsafe { api.create_input_buffer(&mut params)? };
        let raw = params.inputBuffer;

        Ok(Self {
            api,
            raw,
            resolution: resolution.clone(),
        })
    }

    pub fn lock(&self) -> Result<InputBufferGuard> {
        InputBufferGuard::lock_buffer(&self)
    }
}
impl Deref for InputBuffer {
    type Target = sys::NV_ENC_INPUT_PTR;
    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}
impl Drop for InputBuffer {
    fn drop(&mut self) {
        unsafe {
            self.api
                .destroy_input_buffer(self.raw)
                .unwrap_or_else(|e| eprintln!("unable to destroy input buffer: {}", e))
        };
    }
}

struct BitstreamBufferGuard {
    buf_ptr: sys::NV_ENC_OUTPUT_PTR,
    api: EncoderAPI,

    pub params: sys::NV_ENC_LOCK_BITSTREAM,
    raw: *mut u8,
    pub size: u32,
}
impl BitstreamBufferGuard {
    fn lock_buffer(buffer: &BitstreamBuffer) -> Result<Self> {
        let api = &buffer.api;
        let mut params = sys::NV_ENC_LOCK_BITSTREAM {
            outputBitstream: **buffer,
            ..Default::default()
        };
        unsafe { api.lock_bitstream(&mut params)? };
        Ok(Self {
            api: api.clone(),
            buf_ptr: **buffer,

            params,
            raw: params.bitstreamBufferPtr as *mut u8,
            size: params.bitstreamSizeInBytes,
        })
    }
    pub fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(**self, self.size as _) }
    }
}
impl Deref for BitstreamBufferGuard {
    type Target = *mut u8;
    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}
impl Drop for BitstreamBufferGuard {
    fn drop(&mut self) {
        unsafe {
            self.api
                .unlock_bitstream(self.buf_ptr)
                .unwrap_or_else(|e| eprintln!("unable to unlock bitstream: {}", e))
        };
    }
}

struct BitstreamBuffer {
    api: EncoderAPI,
    raw: sys::NV_ENC_OUTPUT_PTR,
}
impl BitstreamBuffer {
    pub fn new(api: &EncoderAPI) -> Result<Self> {
        let api = api.clone();
        let mut params: sys::NV_ENC_CREATE_BITSTREAM_BUFFER = Default::default();
        unsafe { api.create_bitstream_buffer(&mut params)? };
        let raw = params.bitstreamBuffer;

        Ok(Self { api, raw })
    }

    pub fn lock(&self) -> Result<BitstreamBufferGuard> {
        BitstreamBufferGuard::lock_buffer(&self)
    }
}
impl Deref for BitstreamBuffer {
    type Target = sys::NV_ENC_INPUT_PTR;
    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}
impl Drop for BitstreamBuffer {
    fn drop(&mut self) {
        unsafe {
            self.api
                .destroy_bitstream_buffer(self.raw)
                .unwrap_or_else(|e| eprintln!("unable to destroy bitstream buffer: {}", e))
        };
    }
}

pub struct EncodeSession {
    api: EncoderAPI,
    input_buffer: Option<InputBuffer>,
    output_bitstream: Option<BitstreamBuffer>,
}

impl EncodeSession {
    pub fn new(ctx: &Rc<cuda::Context>) -> anyhow::Result<Self> {
        let api = Rc::new(InnerEncoderAPI::new(ctx)?);
        Ok(Self {
            api,
            input_buffer: None,
            output_bitstream: None,
        })
    }

    pub fn get_codec_guids(&self) -> Result<Vec<sys::GUID>> {
        self.api.get_codec_guids()
    }

    pub fn supports_codec(&self, &codec_guid: &sys::GUID) -> Result<bool> {
        Ok(self.get_codec_guids()?.contains(&codec_guid))
    }

    /// Get vec of supported input formats
    pub fn get_input_formats(
        &self,
        &codec_guid: &sys::GUID,
    ) -> Result<Vec<sys::NV_ENC_BUFFER_FORMAT>> {
        self.api.get_input_formats(&codec_guid)
    }

    fn make_config(
        &self,
        resolution: &Resolution,
        rate: &RateParameters,
    ) -> Result<sys::NV_ENC_INITIALIZE_PARAMS> {
        let encode_guid = Codec::H264;
        let preset_guid = Preset::P4;
        let tuning = sys::NV_ENC_TUNING_INFO::NV_ENC_TUNING_INFO_LOW_LATENCY;
        let mut encode_config = unsafe {
            let mut conf: sys::NV_ENC_PRESET_CONFIG = Default::default();
            self.api
                .get_encode_preset_config_ex(encode_guid, preset_guid, tuning, &mut conf)?;
            conf.presetCfg
        };

        encode_config.gopLength = 10;
        encode_config.frameIntervalP = 1;
        encode_config.frameFieldMode =
            sys::NV_ENC_PARAMS_FRAME_FIELD_MODE::NV_ENC_PARAMS_FRAME_FIELD_MODE_FRAME;
        encode_config.rcParams.rateControlMode = sys::NV_ENC_PARAMS_RC_MODE::NV_ENC_PARAMS_RC_CBR;
        encode_config.rcParams.averageBitRate = rate.target_bitrate;

        Ok(sys::NV_ENC_INITIALIZE_PARAMS {
            version: sys::NV_ENC_INITIALIZE_PARAMS_VER,
            maxEncodeHeight: MAX_DIM,
            maxEncodeWidth: MAX_DIM,

            enableEncodeAsync: 0,
            enablePTD: 1,

            encodeConfig: &mut encode_config,
            tuningInfo: tuning,
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
        })
    }

    pub fn initialize(&mut self, resolution: &Resolution, rate: &RateParameters) -> Result<()> {
        let mut params = self.make_config(resolution, rate)?;
        unsafe { self.api.initialize_encoder(&mut params)? };

        self.input_buffer = Some(InputBuffer::new(&self.api, &resolution)?);
        self.output_bitstream = Some(BitstreamBuffer::new(&self.api)?);

        Ok(())
    }

    pub fn update_rates(&mut self, resolution: &Resolution, rate: &RateParameters) -> Result<()> {
        let initialize_param = self.make_config(resolution, rate)?;
        let mut params = sys::NV_ENC_RECONFIGURE_PARAMS {
            reInitEncodeParams: initialize_param,
            ..Default::default()
        };
        params.set_forceIDR(1);
        params.set_resetEncoder(1);
        unsafe { self.api.reconfigure_encoder(&mut params)? };

        match &self.input_buffer {
            Some(buf) => {
                if buf.resolution != *resolution {
                    self.input_buffer = Some(InputBuffer::new(&self.api, &resolution)?);
                }
            }
            None => {
                eprintln!("tried to update rate without existing buffer")
            }
        };

        Ok(())
    }

    fn prep_frame_data(&self, frame: &VideoFrameBuffer) -> anyhow::Result<()> {
        let frame = frame.to_i420()?;

        let dst = self
            .input_buffer
            .as_ref()
            .context("no input buffer present")?
            .lock()?;

        unsafe {
            // luma slice
            let src = frame.data.as_ptr();
            let d_pitch = dst.pitch as usize;
            let width = frame.width;
            let height = frame.height;

            for i in 0..height {
                std::ptr::copy::<u8>(src.add(i * width), dst.add(i * d_pitch), width);
            }

            // chroma slices (U followed by V)
            // 2x2 subsampling
            let src = src.add(height * width);
            let dst = dst.add(height * d_pitch);
            let subsamp_width = width / 2;
            let subsamp_height = height / 2;
            let subsamp_d_pitch = d_pitch / 2;

            for i in 0..subsamp_height * 2 {
                std::ptr::copy::<u8>(
                    src.add(i * subsamp_width),
                    dst.add(i * subsamp_d_pitch),
                    subsamp_width,
                );
            }
        };

        Ok(())
    }

    fn read_output_data(&self) -> anyhow::Result<Bytes> {
        let buf = self
            .output_bitstream
            .as_ref()
            .context("no output bitstream present")?
            .lock()?;

        let bytes = Bytes::copy_from_slice(buf.as_slice());
        Ok(bytes)
    }

    pub fn encode_picture(
        &mut self,
        frame: &VideoFrameBuffer,
        force_keyframe: bool,
    ) -> anyhow::Result<Bytes> {
        self.prep_frame_data(&frame)?;

        let mut params = sys::NV_ENC_PIC_PARAMS {
            inputWidth: frame.width as _,
            inputHeight: frame.height as _,
            inputPitch: frame.width as _,
            encodePicFlags: if force_keyframe {
                sys::NV_ENC_PIC_FLAGS::NV_ENC_PIC_FLAG_FORCEIDR as u32
                    | sys::NV_ENC_PIC_FLAGS::NV_ENC_PIC_FLAG_OUTPUT_SPSPPS as u32
            } else {
                Default::default()
            },
            inputBuffer: *self.input_buffer.as_deref().unwrap(),
            outputBitstream: *self.output_bitstream.as_deref().unwrap(),
            bufferFmt: sys::NV_ENC_BUFFER_FORMAT::NV_ENC_BUFFER_FORMAT_IYUV,
            pictureStruct: sys::NV_ENC_PIC_STRUCT::NV_ENC_PIC_STRUCT_FRAME,
            ..Default::default()
        };

        unsafe { self.api.encode_picture(&mut params)? };

        self.read_output_data()
    }
}
