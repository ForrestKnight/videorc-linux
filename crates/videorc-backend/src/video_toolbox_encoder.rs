use std::ffi::c_void;
use std::ptr::{self, NonNull};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail, ensure};
use block2::RcBlock;
use objc2_core_foundation::{CFBoolean, CFNumber, CFRetained, CFString, CFType};
use objc2_core_media::{CMSampleBuffer, CMTime, kCMTimeInvalid, kCMVideoCodecType_H264};
use objc2_core_video::{CVPixelBufferGetHeight, CVPixelBufferGetWidth};
use objc2_video_toolbox::{
    VTCompressionSession, VTEncodeInfoFlags, VTSession, VTSessionSetProperty,
    kVTCompressionPropertyKey_AllowFrameReordering, kVTCompressionPropertyKey_ExpectedFrameRate,
    kVTCompressionPropertyKey_MaxKeyFrameInterval, kVTCompressionPropertyKey_RealTime,
};

use crate::metal_compositor::MetalCompositorTargetPixelBuffer;

const NO_ERR: OSStatus = 0;
type OSStatus = i32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoToolboxRealtimePropertyStatuses {
    pub real_time: OSStatus,
    pub allow_frame_reordering: OSStatus,
    pub expected_frame_rate: OSStatus,
    pub max_key_frame_interval: OSStatus,
    pub expected_frame_rate_value: i32,
    pub max_key_frame_interval_value: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoToolboxH264ProbeResult {
    pub width: usize,
    pub height: usize,
    pub create_status: OSStatus,
    pub property_statuses: VideoToolboxRealtimePropertyStatuses,
    pub prepare_status: OSStatus,
    pub encode_status: Option<OSStatus>,
    pub complete_status: Option<OSStatus>,
    pub encode_info_flags: Option<u32>,
    pub callback_count: usize,
    pub callback_status: Option<OSStatus>,
    pub callback_info_flags: Option<u32>,
    pub sample_buffer_count: usize,
    pub sample_total_size_bytes: usize,
    pub block_buffer_data_bytes: usize,
    pub copied_sample_count: usize,
    pub copied_sample_bytes: usize,
    pub copied_sample_prefix: Vec<u8>,
    pub sample_copy_error_status: Option<OSStatus>,
    pub frame_dropped: bool,
    pub iosurface_backed: bool,
}

impl VideoToolboxH264ProbeResult {
    fn prepared(
        width: usize,
        height: usize,
        property_statuses: VideoToolboxRealtimePropertyStatuses,
        prepare_status: OSStatus,
    ) -> Self {
        Self {
            width,
            height,
            create_status: NO_ERR,
            property_statuses,
            prepare_status,
            encode_status: None,
            complete_status: None,
            encode_info_flags: None,
            callback_count: 0,
            callback_status: None,
            callback_info_flags: None,
            sample_buffer_count: 0,
            sample_total_size_bytes: 0,
            block_buffer_data_bytes: 0,
            copied_sample_count: 0,
            copied_sample_bytes: 0,
            copied_sample_prefix: Vec::new(),
            sample_copy_error_status: None,
            frame_dropped: false,
            iosurface_backed: false,
        }
    }
}

#[derive(Debug, Default)]
struct EncodeCallbackState {
    callback_count: usize,
    status: Option<OSStatus>,
    info_flags: Option<u32>,
    sample_buffer_count: usize,
    sample_total_size_bytes: usize,
    block_buffer_data_bytes: usize,
    copied_sample_count: usize,
    copied_sample_bytes: usize,
    copied_sample_prefix: Vec<u8>,
    sample_copy_error_status: Option<OSStatus>,
    frame_dropped: bool,
}

pub struct VideoToolboxH264Session {
    session: CFRetained<VTCompressionSession>,
    width: usize,
    height: usize,
    property_statuses: VideoToolboxRealtimePropertyStatuses,
}

impl VideoToolboxH264Session {
    pub fn new(width: usize, height: usize) -> Result<Self> {
        Self::new_realtime(width, height, 30, 60)
    }

    pub fn new_realtime(
        width: usize,
        height: usize,
        expected_frame_rate: i32,
        max_key_frame_interval: i32,
    ) -> Result<Self> {
        ensure!(width > 0, "VideoToolbox session width must be non-zero");
        ensure!(height > 0, "VideoToolbox session height must be non-zero");
        ensure!(
            expected_frame_rate > 0,
            "VideoToolbox expected frame rate must be positive"
        );
        ensure!(
            max_key_frame_interval > 0,
            "VideoToolbox max keyframe interval must be positive"
        );
        ensure!(
            width <= i32::MAX as usize && height <= i32::MAX as usize,
            "VideoToolbox session dimensions exceed i32 range: {width}x{height}"
        );

        let mut raw_session: *mut VTCompressionSession = ptr::null_mut();
        let status = unsafe {
            VTCompressionSession::create(
                None,
                width as i32,
                height as i32,
                kCMVideoCodecType_H264,
                None,
                None,
                None,
                None,
                ptr::null_mut(),
                NonNull::from(&mut raw_session),
            )
        };
        if status != NO_ERR {
            bail!("VTCompressionSessionCreate(H.264 {width}x{height}) failed with {status}");
        }
        let raw_session =
            NonNull::new(raw_session).context("VTCompressionSessionCreate returned null")?;
        let session = unsafe { CFRetained::from_raw(raw_session) };
        let mut session = Self {
            session,
            width,
            height,
            property_statuses: VideoToolboxRealtimePropertyStatuses {
                real_time: NO_ERR,
                allow_frame_reordering: NO_ERR,
                expected_frame_rate: NO_ERR,
                max_key_frame_interval: NO_ERR,
                expected_frame_rate_value: expected_frame_rate,
                max_key_frame_interval_value: max_key_frame_interval,
            },
        };
        session.property_statuses =
            session.configure_realtime_low_latency(expected_frame_rate, max_key_frame_interval)?;
        Ok(session)
    }

    pub fn prepare(&self) -> Result<VideoToolboxH264ProbeResult> {
        let status = unsafe { self.session.prepare_to_encode_frames() };
        if status != NO_ERR {
            bail!(
                "VTCompressionSessionPrepareToEncodeFrames(H.264 {}x{}) failed with {status}",
                self.width,
                self.height
            );
        }
        Ok(VideoToolboxH264ProbeResult::prepared(
            self.width,
            self.height,
            self.property_statuses,
            status,
        ))
    }

    pub fn encode_retained_target(
        &self,
        target: &MetalCompositorTargetPixelBuffer,
    ) -> Result<VideoToolboxH264ProbeResult> {
        let pixel_buffer = target.pixel_buffer();
        let width = CVPixelBufferGetWidth(pixel_buffer);
        let height = CVPixelBufferGetHeight(pixel_buffer);
        ensure!(
            width == self.width && height == self.height,
            "target dimensions {width}x{height} do not match VideoToolbox session {}x{}",
            self.width,
            self.height
        );
        let iosurface_backed = target.has_iosurface();
        ensure!(
            iosurface_backed,
            "retained Metal target is not IOSurface-backed"
        );

        let state = Arc::new(Mutex::new(EncodeCallbackState::default()));
        let callback_state = state.clone();
        let output_handler: RcBlock<dyn Fn(OSStatus, VTEncodeInfoFlags, *mut CMSampleBuffer)> =
            RcBlock::new(
                move |status: OSStatus,
                      info_flags: VTEncodeInfoFlags,
                      sample_buffer: *mut CMSampleBuffer| {
                    let mut state = callback_state
                        .lock()
                        .expect("encode callback state poisoned");
                    state.callback_count += 1;
                    state.status = Some(status);
                    state.info_flags = Some(info_flags.0);
                    if !sample_buffer.is_null() {
                        state.sample_buffer_count += 1;
                        let sample_buffer = unsafe { &*sample_buffer };
                        state.sample_total_size_bytes +=
                            unsafe { sample_buffer.total_sample_size() };
                        if let Some(data_buffer) = unsafe { sample_buffer.data_buffer() } {
                            state.block_buffer_data_bytes += unsafe { data_buffer.data_length() };
                        }
                        match copy_encoded_sample_buffer_bytes(sample_buffer) {
                            Ok(bytes) if !bytes.is_empty() => {
                                state.copied_sample_count += 1;
                                state.copied_sample_bytes += bytes.len();
                                if state.copied_sample_prefix.is_empty() {
                                    let prefix_len = bytes.len().min(16);
                                    state
                                        .copied_sample_prefix
                                        .extend_from_slice(&bytes[..prefix_len]);
                                }
                            }
                            Ok(_) => {}
                            Err(status) => {
                                state.sample_copy_error_status = Some(status);
                            }
                        }
                    }
                    state.frame_dropped |= info_flags.contains(VTEncodeInfoFlags::FrameDropped);
                },
            );

        let mut encode_info_flags = VTEncodeInfoFlags::empty();
        let presentation_time = unsafe { CMTime::new(0, 60) };
        let duration = unsafe { CMTime::new(1, 60) };
        let encode_status = unsafe {
            self.session.encode_frame_with_output_handler(
                pixel_buffer,
                presentation_time,
                duration,
                None,
                &mut encode_info_flags,
                RcBlock::as_ptr(&output_handler),
            )
        };
        if encode_status != NO_ERR {
            bail!(
                "VTCompressionSessionEncodeFrameWithOutputHandler(H.264 {}x{}) failed with {encode_status}",
                self.width,
                self.height
            );
        }
        let complete_status = unsafe { self.session.complete_frames(kCMTimeInvalid) };
        if complete_status != NO_ERR {
            bail!(
                "VTCompressionSessionCompleteFrames(H.264 {}x{}) failed with {complete_status}",
                self.width,
                self.height
            );
        }

        let state = state.lock().expect("encode callback state poisoned");
        Ok(VideoToolboxH264ProbeResult {
            width,
            height,
            create_status: NO_ERR,
            property_statuses: self.property_statuses,
            prepare_status: NO_ERR,
            encode_status: Some(encode_status),
            complete_status: Some(complete_status),
            encode_info_flags: Some(encode_info_flags.0),
            callback_count: state.callback_count,
            callback_status: state.status,
            callback_info_flags: state.info_flags,
            sample_buffer_count: state.sample_buffer_count,
            sample_total_size_bytes: state.sample_total_size_bytes,
            block_buffer_data_bytes: state.block_buffer_data_bytes,
            copied_sample_count: state.copied_sample_count,
            copied_sample_bytes: state.copied_sample_bytes,
            copied_sample_prefix: state.copied_sample_prefix.clone(),
            sample_copy_error_status: state.sample_copy_error_status,
            frame_dropped: state.frame_dropped
                || encode_info_flags.contains(VTEncodeInfoFlags::FrameDropped),
            iosurface_backed,
        })
    }

    fn configure_realtime_low_latency(
        &self,
        expected_frame_rate: i32,
        max_key_frame_interval: i32,
    ) -> Result<VideoToolboxRealtimePropertyStatuses> {
        let real_time = self.set_property(
            "RealTime",
            unsafe { kVTCompressionPropertyKey_RealTime },
            CFBoolean::new(true).as_ref(),
        )?;
        let allow_frame_reordering = self.set_property(
            "AllowFrameReordering",
            unsafe { kVTCompressionPropertyKey_AllowFrameReordering },
            CFBoolean::new(false).as_ref(),
        )?;
        let expected_frame_rate_value = CFNumber::new_i32(expected_frame_rate);
        let expected_frame_rate_status = self.set_property(
            "ExpectedFrameRate",
            unsafe { kVTCompressionPropertyKey_ExpectedFrameRate },
            expected_frame_rate_value.as_ref(),
        )?;
        let max_key_frame_interval_value = CFNumber::new_i32(max_key_frame_interval);
        let max_key_frame_interval_status = self.set_property(
            "MaxKeyFrameInterval",
            unsafe { kVTCompressionPropertyKey_MaxKeyFrameInterval },
            max_key_frame_interval_value.as_ref(),
        )?;

        Ok(VideoToolboxRealtimePropertyStatuses {
            real_time,
            allow_frame_reordering,
            expected_frame_rate: expected_frame_rate_status,
            max_key_frame_interval: max_key_frame_interval_status,
            expected_frame_rate_value: expected_frame_rate,
            max_key_frame_interval_value: max_key_frame_interval,
        })
    }

    fn set_property(
        &self,
        name: &str,
        property_key: &CFString,
        property_value: &CFType,
    ) -> Result<OSStatus> {
        let compression_session: &VTCompressionSession = &self.session;
        let session: &VTSession = compression_session.as_ref();
        let status = unsafe { VTSessionSetProperty(session, property_key, Some(property_value)) };
        if status != NO_ERR {
            bail!("VTSessionSetProperty({name}) failed with {status}");
        }
        Ok(status)
    }
}

fn copy_encoded_sample_buffer_bytes(
    sample_buffer: &CMSampleBuffer,
) -> std::result::Result<Vec<u8>, OSStatus> {
    let Some(data_buffer) = (unsafe { sample_buffer.data_buffer() }) else {
        return Ok(Vec::new());
    };
    let data_len = unsafe { data_buffer.data_length() };
    if data_len == 0 {
        return Ok(Vec::new());
    }

    let mut bytes = vec![0u8; data_len];
    let destination = NonNull::new(bytes.as_mut_ptr().cast::<c_void>())
        .expect("non-empty Vec must expose a non-null pointer");
    let status = unsafe { data_buffer.copy_data_bytes(0, data_len, destination) };
    if status != NO_ERR {
        return Err(status);
    }
    Ok(bytes)
}

impl Drop for VideoToolboxH264Session {
    fn drop(&mut self) {
        unsafe { self.session.invalidate() };
    }
}

#[allow(dead_code)]
pub fn probe_h264_session_ready(
    width: usize,
    height: usize,
) -> Result<VideoToolboxH264ProbeResult> {
    let session = VideoToolboxH264Session::new(width, height)?;
    session.prepare()
}

#[allow(dead_code)]
pub fn probe_h264_encode_retained_target(
    target: &MetalCompositorTargetPixelBuffer,
) -> Result<VideoToolboxH264ProbeResult> {
    let session = VideoToolboxH264Session::new(target.width(), target.height())?;
    let mut result = session.prepare()?;
    result.iosurface_backed = target.has_iosurface();
    let encode_result = session.encode_retained_target(target)?;
    Ok(VideoToolboxH264ProbeResult {
        prepare_status: result.prepare_status,
        iosurface_backed: result.iosurface_backed,
        ..encode_result
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metal_compositor::{GpuSource, MetalSceneCompositor};

    #[test]
    fn video_toolbox_h264_session_prepares_or_skips() {
        match probe_h264_session_ready(64, 64) {
            Ok(result) => {
                assert_eq!(result.width, 64);
                assert_eq!(result.height, 64);
                assert_eq!(result.create_status, NO_ERR);
                assert_eq!(result.property_statuses.real_time, NO_ERR);
                assert_eq!(result.property_statuses.allow_frame_reordering, NO_ERR);
                assert_eq!(result.property_statuses.expected_frame_rate, NO_ERR);
                assert_eq!(result.property_statuses.max_key_frame_interval, NO_ERR);
                assert_eq!(result.property_statuses.expected_frame_rate_value, 30);
                assert_eq!(result.property_statuses.max_key_frame_interval_value, 60);
                assert_eq!(result.prepare_status, NO_ERR);
            }
            Err(error) => {
                eprintln!("skipping: VideoToolbox H.264 session unavailable: {error:#}");
            }
        }
    }

    #[test]
    fn video_toolbox_accepts_retained_metal_target_or_skips() {
        let Some(mut compositor) = MetalSceneCompositor::new() else {
            eprintln!("skipping: no Metal device available in this environment");
            return;
        };
        let red = [0u8, 0, 255, 255];
        let sources = [GpuSource {
            bgra: &red,
            width: 1,
            height: 1,
            dest: [0.0, 0.0, 1.0, 1.0],
            crop: [0.0; 4],
            mirror: false,
            circle: false,
        }];
        compositor
            .compose_bgra(64, 64, [0.0, 0.0, 0.0, 1.0], &sources)
            .expect("compose retained Metal target");
        let Some(target) = compositor.latest_target_pixel_buffer() else {
            eprintln!("skipping: IOSurface-backed Metal target unavailable");
            return;
        };

        match probe_h264_encode_retained_target(&target) {
            Ok(result) => {
                assert_eq!(result.width, 64);
                assert_eq!(result.height, 64);
                assert_eq!(result.create_status, NO_ERR);
                assert_eq!(result.property_statuses.real_time, NO_ERR);
                assert_eq!(result.property_statuses.allow_frame_reordering, NO_ERR);
                assert_eq!(result.property_statuses.expected_frame_rate, NO_ERR);
                assert_eq!(result.property_statuses.max_key_frame_interval, NO_ERR);
                assert_eq!(result.prepare_status, NO_ERR);
                assert_eq!(result.encode_status, Some(NO_ERR));
                assert_eq!(result.complete_status, Some(NO_ERR));
                assert!(result.iosurface_backed);
                assert!(
                    !result.frame_dropped,
                    "VideoToolbox dropped the retained target frame: {result:?}"
                );
                assert!(
                    result.callback_count > 0,
                    "VideoToolbox did not invoke the encode callback: {result:?}"
                );
                assert_eq!(result.callback_status, Some(NO_ERR));
                assert!(
                    result.sample_buffer_count > 0,
                    "VideoToolbox accepted the frame but returned no sample buffer: {result:?}"
                );
                assert!(
                    result.sample_total_size_bytes > 0,
                    "VideoToolbox sample buffer had no encoded sample bytes: {result:?}"
                );
                assert!(
                    result.block_buffer_data_bytes > 0,
                    "VideoToolbox sample buffer had no accessible CMBlockBuffer bytes: {result:?}"
                );
                assert_eq!(result.sample_copy_error_status, None);
                assert!(
                    result.copied_sample_count > 0,
                    "VideoToolbox sample bytes were not copied from CMBlockBuffer: {result:?}"
                );
                assert!(
                    result.copied_sample_bytes > 0,
                    "VideoToolbox copied sample byte count stayed at zero: {result:?}"
                );
                assert!(
                    !result.copied_sample_prefix.is_empty(),
                    "VideoToolbox copied sample prefix is empty: {result:?}"
                );
            }
            Err(error) => {
                eprintln!(
                    "skipping: VideoToolbox could not encode retained Metal target: {error:#}"
                );
            }
        }
    }
}
