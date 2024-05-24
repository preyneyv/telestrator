use std::{ffi::c_void, ptr, sync::Arc};

use nvidia_sys::nvencodeapi as sys;

use super::cuda;

pub use nvidia_sys::guids::NV_ENC_CODEC as Codec;

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

pub struct EncodeSession {
    api: sys::NV_ENCODE_API_FUNCTION_LIST,
    ctx: Arc<cuda::Context>,
    encoder_ptr: *mut c_void,
}

impl EncodeSession {
    pub fn new(ctx: &Arc<cuda::Context>) -> anyhow::Result<Self> {
        let ctx = ctx.clone();
        let api = make_encode_api()?;

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
            api,
            ctx,
            encoder_ptr,
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
}

impl Drop for EncodeSession {
    fn drop(&mut self) {
        unsafe { self.api.nvEncDestroyEncoder.unwrap_unchecked()(self.encoder_ptr) };
    }
}
