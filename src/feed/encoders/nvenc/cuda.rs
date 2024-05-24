use std::{
    ffi::{CStr, CString},
    os::raw::{c_char, c_int, c_uint},
    ptr::{null, null_mut},
    sync::Arc,
};

use once_cell::sync::OnceCell;

use nvidia_sys::cuda::{self as sys};

static IS_INITIALIZED: OnceCell<bool> = OnceCell::new();

trait CudaErrorCode {
    fn ok(self) -> std::result::Result<(), CudaError>;
}
#[derive(thiserror::Error, Debug)]
#[error("{label}: {message}")]
pub struct CudaError {
    code: sys::CUresult,
    label: String,
    message: String,
}
impl CudaErrorCode for sys::CUresult {
    fn ok(self) -> std::result::Result<(), CudaError> {
        match self {
            sys::CUresult::CUDA_SUCCESS => Ok(()),
            code => {
                let mut label: *const c_char = null();
                let mut message: *const c_char = null();
                // Need to make sure CUDA is initialized before error name stuff
                init()?;
                unsafe {
                    sys::cuGetErrorName(code, &mut label);
                    sys::cuGetErrorString(code, &mut message);
                }

                let label = unsafe { CStr::from_ptr(label) }
                    .to_string_lossy()
                    .to_string();
                let message = unsafe { CStr::from_ptr(message) }
                    .to_string_lossy()
                    .to_string();

                return Err(CudaError {
                    code,
                    label,
                    message,
                });
            }
        }
    }
}

type Result<T> = std::result::Result<T, CudaError>;

pub fn init() -> Result<()> {
    IS_INITIALIZED.get_or_try_init(|| -> Result<bool> {
        unsafe {
            sys::cuInit(0).ok()?;
            Ok(true)
        }
    })?;

    Ok(())
}

pub struct Device {
    raw: sys::CUdevice,
}

impl Device {
    pub fn count() -> Result<c_int> {
        init()?;
        let mut count = 0;
        unsafe {
            sys::cuDeviceGetCount(&mut count).ok()?;
        }
        Ok(count)
    }

    pub fn new(idx: c_int) -> Result<Self> {
        init()?;
        let mut raw: sys::CUdevice = 0;
        unsafe {
            sys::cuDeviceGet(&mut raw, idx).ok()?;
        }

        Ok(Self { raw })
    }

    pub fn as_ptr(&self) -> sys::CUdevice {
        self.raw
    }

    pub fn name(&self) -> Result<String> {
        let name = unsafe {
            let size = 256;
            let name = CString::from_vec_unchecked(vec![0; size]).into_raw();
            sys::cuDeviceGetName(name, size as _, self.raw).ok()?;
            CString::from_raw(name)
        };
        Ok(name.to_string_lossy().to_string())
    }

    pub fn compute_capability(&self) -> Result<(i32, i32)> {
        let mut major = 0;
        let mut minor = 0;
        unsafe {
            sys::cuDeviceComputeCapability(&mut major, &mut minor, self.raw).ok()?;
        };
        Ok((major, minor))
    }
}

pub struct Context {
    raw: sys::CUcontext,
    dev: Arc<Device>,
}

impl Context {
    pub fn new(flags: sys::CUctx_flags_enum, dev: &Arc<Device>) -> Result<Self> {
        init()?;

        let dev = dev.clone();
        let mut raw: sys::CUcontext = null_mut();
        unsafe {
            sys::cuCtxCreate_v2(&mut raw, flags as _, dev.as_ptr()).ok()?;
        };

        Ok(Self { raw, dev })
    }

    pub fn api_version(&self) -> Result<u32> {
        init()?;

        let mut version = 0;
        unsafe {
            sys::cuCtxGetApiVersion(self.raw, &mut version).ok()?;
        };
        Ok(version)
    }

    pub fn as_ptr(&self) -> sys::CUcontext {
        self.raw
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe { sys::cuCtxDestroy_v2(self.raw).ok().unwrap() };
    }
}
