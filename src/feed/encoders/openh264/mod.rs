use std::{
    os::{
        self,
        raw::{c_int, c_void},
    },
    ptr::{addr_of_mut, null, null_mut},
};

use anyhow::{Context, Result};
use bytes::Bytes;

use crate::feed::frame::{Resolution, VideoFrameBuffer};

use super::{
    EncoderFrameFlags, FeedEncoder, FeedEncoderConfigImpl, FeedEncoderImpl, RateParameters,
};
use o264::OpenH264API;
use o264_sys::{
    videoFormatI420, ISVCEncoder, ISVCEncoderVtbl, SBitrateInfo, SEncParamBase, SEncParamExt,
    SFrameBSInfo, SSourcePicture, API, ENCODER_OPTION, ENCODER_OPTION_BITRATE,
    ENCODER_OPTION_DATAFORMAT, ENCODER_OPTION_FRAME_RATE, ENCODER_OPTION_SVC_ENCODE_PARAM_EXT,
    ENCODER_OPTION_TRACE_LEVEL, RC_BITRATE_MODE, SCREEN_CONTENT_REAL_TIME, SM_FIXEDSLCNUM_SLICE,
    SPATIAL_LAYER_ALL, UNSPECIFIED_BIT_RATE, WELS_LOG_DETAIL,
};
use openh264 as o264;
use openh264_sys2 as o264_sys;

#[derive(thiserror::Error, Debug)]
#[error("OpenH264 error: {0}")]
pub struct OpenH264Error(i32);

trait OpenH264ErrorCode {
    fn ok(self) -> std::result::Result<(), OpenH264Error>;
}
impl OpenH264ErrorCode for os::raw::c_int {
    fn ok(self) -> std::result::Result<(), OpenH264Error> {
        if self == 0 {
            Ok(())
        } else {
            Err(OpenH264Error(self))
        }
    }
}

/// Wrapper around VTable methods for ISVCEncoderVtbl.
#[rustfmt::skip]
#[allow(dead_code)]
 struct OpenH264InnerEncoder {
    pub api: OpenH264API,
    ptr: *mut *const ISVCEncoderVtbl,

    initialize: unsafe extern "C" fn(arg1: *mut ISVCEncoder, pParam: *const SEncParamBase) -> c_int,
    initialize_ext: unsafe extern "C" fn(arg1: *mut ISVCEncoder, pParam: *const SEncParamExt) -> c_int,
    get_default_params: unsafe extern "C" fn(arg1: *mut ISVCEncoder, pParam: *mut SEncParamExt) -> c_int,
    uninitialize: unsafe extern "C" fn(arg1: *mut ISVCEncoder) -> c_int,
    encode_frame: unsafe extern "C" fn(arg1: *mut ISVCEncoder, kpSrcPic: *const SSourcePicture, pBsInfo: *mut SFrameBSInfo) -> c_int,
    encode_parameter_sets: unsafe extern "C" fn(arg1: *mut ISVCEncoder, pBsInfo: *mut SFrameBSInfo) -> c_int,
    force_intra_frame: unsafe extern "C" fn(arg1: *mut ISVCEncoder, bIDR: bool) -> c_int,
    set_option: unsafe extern "C" fn(arg1: *mut ISVCEncoder, eOptionId: ENCODER_OPTION, pOption: *mut c_void) -> c_int,
    get_option: unsafe extern "C" fn(arg1: *mut ISVCEncoder, eOptionId: ENCODER_OPTION, pOption: *mut c_void) -> c_int,
}

#[rustfmt::skip]
#[allow(dead_code)]
impl OpenH264InnerEncoder {
    pub fn new(api: OpenH264API) -> Result<Self> {
        let mut ptr = null::<ISVCEncoderVtbl>() as *mut *const ISVCEncoderVtbl;
        unsafe {
            api.WelsCreateSVCEncoder(&mut ptr)
                .ok()
                .context("unable to create encoder")?;

            Ok(Self {
                api,
                ptr,
                initialize: (**ptr).Initialize.context("missing Initialize")?,
                initialize_ext: (**ptr).InitializeExt.context("missing InitializeExt")?,
                get_default_params: (**ptr).GetDefaultParams.context("missing GetDefaultParams")?,
                uninitialize: (**ptr).Uninitialize.context("missing Uninitialize")?,
                encode_frame: (**ptr).EncodeFrame.context("missing EncodeFrame")?,
                encode_parameter_sets: (**ptr).EncodeParameterSets.context("missing EncodeParameterSets")?,
                force_intra_frame: (**ptr).ForceIntraFrame.context("missing ForceIntraFrame")?,
                set_option: (**ptr).SetOption.context("missing SetOption")?,
                get_option: (**ptr).GetOption.context("missing GetOption")?,
            })
        }
    }

    pub unsafe fn uninitialize(&self) -> std::result::Result<(), OpenH264Error> { (self.uninitialize)(self.ptr).ok() }
    pub unsafe fn initialize(&self, p_param: *const SEncParamBase) -> std::result::Result<(), OpenH264Error> { (self.initialize)(self.ptr, p_param).ok() }
    pub unsafe fn initialize_ext(&self, p_param: *const SEncParamExt) -> std::result::Result<(), OpenH264Error> { (self.initialize_ext)(self.ptr, p_param).ok() }
    pub unsafe fn get_default_params(&self, p_param: *mut SEncParamExt) -> std::result::Result<(), OpenH264Error> { (self.get_default_params)(self.ptr, p_param).ok() }
    pub unsafe fn encode_frame(&self, kp_src_pic: *const SSourcePicture, p_bs_info: *mut SFrameBSInfo) -> std::result::Result<(), OpenH264Error> { (self.encode_frame)(self.ptr, kp_src_pic, p_bs_info).ok() }
    pub unsafe fn encode_parameter_sets(&self, p_bs_info: *mut SFrameBSInfo) -> std::result::Result<(), OpenH264Error> { (self.encode_parameter_sets)(self.ptr, p_bs_info).ok() }
    pub unsafe fn force_intra_frame(&self, b_idr: bool) -> std::result::Result<(), OpenH264Error> { (self.force_intra_frame)(self.ptr, b_idr).ok() }
    pub unsafe fn set_option(&self, e_option_id: ENCODER_OPTION, p_option: *mut c_void) -> std::result::Result<(), OpenH264Error> { (self.set_option)(self.ptr, e_option_id, p_option).ok() }
    pub unsafe fn get_option(&self, e_option_id: ENCODER_OPTION, p_option: *mut c_void) -> std::result::Result<(), OpenH264Error> { (self.get_option)(self.ptr, e_option_id, p_option).ok() }
}

impl Drop for OpenH264InnerEncoder {
    fn drop(&mut self) {
        unsafe { self.api.WelsDestroySVCEncoder(self.ptr) }
    }
}

#[derive(Clone)]
pub struct OpenH264FeedEncoderConfig {
    keyframe_interval: u32,
    debug: bool,
}

impl Default for OpenH264FeedEncoderConfig {
    fn default() -> Self {
        Self {
            keyframe_interval: 0,
            debug: false,
        }
    }
}

impl FeedEncoderConfigImpl for OpenH264FeedEncoderConfig {
    fn build(&self, rate: RateParameters) -> Result<super::FeedEncoder> {
        let source = OpenH264FeedEncoder::new(self, rate)?;
        return Ok(FeedEncoder::OpenH264(source));
    }
}

pub struct OpenH264FeedEncoder {
    config: OpenH264FeedEncoderConfig,
    encoder: OpenH264InnerEncoder,
    previous_resolution: Option<(u32, u32)>,
    previous_rate: RateParameters,
}

impl OpenH264FeedEncoder {
    pub fn new(config: &OpenH264FeedEncoderConfig, rate: RateParameters) -> Result<Self> {
        // TODO: Replace with dll/dylib
        let api = o264::OpenH264API::from_source();
        let encoder = OpenH264InnerEncoder::new(api)?;

        Ok(Self {
            config: config.to_owned(),
            encoder,
            previous_resolution: None,
            previous_rate: rate,
        })
    }

    fn create_encoder_params(&self, resolution: Resolution) -> Result<SEncParamExt> {
        let width = resolution.0;
        let height = resolution.1;
        let bitrate = self.previous_rate.target_bitrate;
        let framerate = self.previous_rate.max_fps;

        let mut params = SEncParamExt::default();
        unsafe { self.encoder.get_default_params(&mut params)? };

        params.iPicWidth = width as _;
        params.iPicHeight = height as _;
        params.iTargetBitrate = (bitrate * 1000) as _;
        params.fMaxFrameRate = framerate;

        params.iUsageType = SCREEN_CONTENT_REAL_TIME;
        params.iMaxBitrate = UNSPECIFIED_BIT_RATE as _;
        params.iRCMode = RC_BITRATE_MODE;
        params.bEnableFrameSkip = true;
        params.uiIntraPeriod = self.config.keyframe_interval;
        params.uiMaxNalSize = 0;
        params.iMultipleThreadIdc = 3;

        // not supported by SCREEN_CONTENT_REAL_TIME
        params.bEnableAdaptiveQuant = false;
        // params.eSpsPpsIdStrategy = SPS_LISTING;
        params.bEnableBackgroundDetection = false;

        params.sSpatialLayers[0].iVideoWidth = params.iPicWidth;
        params.sSpatialLayers[0].iVideoHeight = params.iPicHeight;
        params.sSpatialLayers[0].fFrameRate = params.fMaxFrameRate;
        params.sSpatialLayers[0].iSpatialBitrate = params.iTargetBitrate;
        params.sSpatialLayers[0].sSliceArgument.uiSliceNum = 1;
        params.sSpatialLayers[0].sSliceArgument.uiSliceMode = SM_FIXEDSLCNUM_SLICE;

        params.iTemporalLayerNum = 1;

        Ok(params)
    }
}

impl FeedEncoderImpl for OpenH264FeedEncoder {
    fn encode(&mut self, frame: &VideoFrameBuffer, mut flags: EncoderFrameFlags) -> Result<Bytes> {
        if self.previous_resolution != Some(frame.resolution()) {
            let mut params = self.create_encoder_params(frame.resolution())?;

            unsafe {
                if self.previous_resolution.is_none() {
                    // First run, initialize it like normal.
                    self.encoder
                        .initialize_ext(&params)
                        .context("failed to initialize encoder")?;
                    if self.config.debug {
                        let mut level = WELS_LOG_DETAIL;
                        self.encoder
                            .set_option(ENCODER_OPTION_TRACE_LEVEL, addr_of_mut!(level).cast())?
                    }

                    let mut data_format = videoFormatI420;
                    self.encoder
                        .set_option(ENCODER_OPTION_DATAFORMAT, addr_of_mut!(data_format).cast())?
                } else {
                    // Use SetOption to update config
                    self.encoder.set_option(
                        ENCODER_OPTION_SVC_ENCODE_PARAM_EXT,
                        addr_of_mut!(params).cast(),
                    )?;
                    flags.force_keyframe = true;
                }

                self.previous_resolution = Some(frame.resolution());
            }
        }

        let frame = frame.to_i420()?;
        let stride = frame.width as i32;
        let data = frame.yuv_slices();
        let source = SSourcePicture {
            iPicWidth: frame.width as _,
            iPicHeight: frame.height as _,
            iColorFormat: videoFormatI420,
            uiTimeStamp: frame.timestamp.to_millis() as _,
            iStride: [stride, stride / 2, stride / 2, 0],
            pData: [
                data.0.as_ptr().cast_mut(),
                data.1.as_ptr().cast_mut(),
                data.2.as_ptr().cast_mut(),
                null_mut(),
            ],
        };

        if flags.force_keyframe {
            unsafe { self.encoder.force_intra_frame(true)? }
        }

        let mut info = SFrameBSInfo::default();

        unsafe {
            self.encoder
                .encode_frame(&source, &mut info)
                .context("encode_frame failed")?
        };

        let mut bitstream = Vec::with_capacity(info.iFrameSizeInBytes as _);
        for l in 0..(info.iLayerNum as usize) {
            let layer = &info.sLayerInfo[l];
            let mut layer_size = 0;
            for n in 0..(layer.iNalCount as usize) {
                layer_size += unsafe { *layer.pNalLengthInByte.add(n) };
            }
            bitstream.extend_from_slice(unsafe {
                std::slice::from_raw_parts(layer.pBsBuf, layer_size as _)
            });
        }

        Ok(Bytes::from(bitstream))
    }

    fn set_rate(&mut self, mut rate: RateParameters) -> Result<()> {
        if self.previous_resolution.is_none() {
            // Encoder is not initialized yet.
            self.previous_rate = rate;
            return Ok(());
        }
        unsafe {
            self.encoder
                .set_option(ENCODER_OPTION_FRAME_RATE, addr_of_mut!(rate.max_fps).cast())
                .ok()
                .context("unable to update framerate")?;
        }

        let mut bitrate = SBitrateInfo {
            iBitrate: rate.target_bitrate as _,
            iLayer: SPATIAL_LAYER_ALL,
        };

        unsafe {
            self.encoder
                .set_option(ENCODER_OPTION_BITRATE, addr_of_mut!(bitrate).cast())
                .ok()
                .context("unable to update bitrate")?;
        };

        self.previous_rate = rate;
        Ok(())
    }
}
