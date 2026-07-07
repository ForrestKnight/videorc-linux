#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

#[cfg(any(target_os = "macos", target_os = "linux"))]
use crate::protocol::DeviceKind;
use crate::protocol::{Device, DeviceStatus};

/// Platform-valued: the id namespace names the capture backend, and the
/// unique id inside the hex encoding is backend-specific (AVFoundation
/// unique ids on macOS, `/dev/videoN` paths on Linux).
#[cfg(not(target_os = "linux"))]
const NATIVE_CAMERA_PREFIX: &str = "camera:avfoundation-native:";
#[cfg(target_os = "linux")]
const NATIVE_CAMERA_PREFIX: &str = "camera:v4l2-native:";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeCameraDevices {
    pub devices: Vec<Device>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CameraFormatSummary {
    pub width: u32,
    pub height: u32,
    pub min_fps: f64,
    pub max_fps: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CameraFormatChoice {
    pub format: CameraFormatSummary,
    pub fallback_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeCameraPermission {
    Authorized,
    NotDetermined,
    Denied,
    Restricted,
    Unknown,
}

pub fn list_native_cameras() -> NativeCameraDevices {
    #[cfg(target_os = "macos")]
    {
        macos::list_native_cameras()
    }

    #[cfg(target_os = "linux")]
    {
        linux::list_native_cameras()
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        NativeCameraDevices {
            devices: Vec::new(),
            warnings: vec![
                "Native camera discovery is only available on macOS and Linux.".to_string(),
            ],
        }
    }
}

pub fn native_camera_name_for_id(camera_id: &str) -> Option<String> {
    let unique_id = parse_native_camera_id(camera_id)?;

    #[cfg(target_os = "macos")]
    {
        macos::camera_name_for_unique_id(&unique_id)
    }

    #[cfg(target_os = "linux")]
    {
        linux::camera_name_for_unique_id(&unique_id)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = unique_id;
        None
    }
}

pub fn camera_capability_matrix_for_id(
    camera_id: &str,
) -> Result<Vec<CameraFormatSummary>, String> {
    let unique_id = parse_native_camera_id(camera_id)
        .ok_or_else(|| "Selected camera is not a native camera.".to_string())?;

    #[cfg(target_os = "macos")]
    {
        macos::camera_capability_matrix_for_unique_id(&unique_id)
    }

    #[cfg(target_os = "linux")]
    {
        linux::camera_capability_matrix_for_unique_id(&unique_id)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = unique_id;
        Err(
            "Native camera capability diagnostics are only available on macOS and Linux."
                .to_string(),
        )
    }
}

pub fn native_camera_device_id(unique_id: &str) -> String {
    format!("{NATIVE_CAMERA_PREFIX}{}", encode_hex(unique_id.as_bytes()))
}

pub fn parse_native_camera_id(id: &str) -> Option<String> {
    let encoded = id.strip_prefix(NATIVE_CAMERA_PREFIX)?;
    let bytes = decode_hex(encoded)?;
    String::from_utf8(bytes).ok()
}

pub fn camera_permission_status(permission: NativeCameraPermission) -> DeviceStatus {
    match permission {
        NativeCameraPermission::Authorized => DeviceStatus::Available,
        NativeCameraPermission::NotDetermined
        | NativeCameraPermission::Denied
        | NativeCameraPermission::Restricted => DeviceStatus::PermissionRequired,
        NativeCameraPermission::Unknown => DeviceStatus::Unavailable,
    }
}

pub fn choose_camera_format(
    formats: &[CameraFormatSummary],
    target_width: u32,
    target_height: u32,
    target_fps: u32,
) -> Option<CameraFormatChoice> {
    let target_fps = f64::from(target_fps);
    let supports_target = |format: &&CameraFormatSummary| {
        format.width == target_width
            && format.height == target_height
            && format_supports_fps(format, target_fps)
    };

    if let Some(format) = formats.iter().find(supports_target) {
        return Some(CameraFormatChoice {
            format: (*format).clone(),
            fallback_reason: None,
        });
    }

    let target_pixels = u64::from(target_width) * u64::from(target_height);
    let fps_capable = formats
        .iter()
        .filter(|format| format_supports_fps(format, target_fps))
        .collect::<Vec<_>>();
    let selected = fps_capable
        .iter()
        .copied()
        .filter(|format| camera_format_pixels(format) >= target_pixels)
        .min_by_key(|format| camera_format_pixels(format).saturating_sub(target_pixels))
        .or_else(|| {
            fps_capable.iter().copied().max_by_key(|format| {
                (
                    camera_format_pixels(format),
                    format.max_fps.round().max(0.0) as u64,
                )
            })
        })
        .or_else(|| {
            formats.iter().max_by_key(|format| {
                (
                    camera_format_pixels(format),
                    format.max_fps.round().max(0.0) as u64,
                )
            })
        })?;

    Some(CameraFormatChoice {
        format: selected.clone(),
        fallback_reason: Some(format!(
            "Requested {target_width}x{target_height}@{target_fps:.0} was not available; selected native {}x{} at {:.0}-{:.0} fps.",
            selected.width, selected.height, selected.min_fps, selected.max_fps
        )),
    })
}

pub fn normalize_camera_formats(mut formats: Vec<CameraFormatSummary>) -> Vec<CameraFormatSummary> {
    formats.retain(|format| {
        format.width > 0
            && format.height > 0
            && format.min_fps.is_finite()
            && format.max_fps.is_finite()
            && format.max_fps > 0.0
            && format.min_fps <= format.max_fps
    });
    formats.sort_by(|left, right| {
        left.width
            .cmp(&right.width)
            .then(left.height.cmp(&right.height))
            .then(left.min_fps.total_cmp(&right.min_fps))
            .then(left.max_fps.total_cmp(&right.max_fps))
    });
    formats.dedup_by(|left, right| {
        left.width == right.width
            && left.height == right.height
            && left.min_fps == right.min_fps
            && left.max_fps == right.max_fps
    });
    formats
}

fn format_supports_fps(format: &CameraFormatSummary, target_fps: f64) -> bool {
    format.min_fps <= target_fps && format.max_fps >= target_fps
}

fn camera_format_pixels(format: &CameraFormatSummary) -> u64 {
    u64::from(format.width) * u64::from(format.height)
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return None;
    }

    value
        .as_bytes()
        .chunks_exact(2)
        .map(|chunk| {
            let high = hex_value(chunk[0])?;
            let low = hex_value(chunk[1])?;
            Some((high << 4) | low)
        })
        .collect()
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

/// The capture loop in `preview_camera.rs` shares the discovery helpers.
#[cfg(target_os = "linux")]
pub(crate) use linux::{SUPPORTED_CAPTURE_FOURCCS, device_format_summaries};

/// V4L2 camera discovery (Linux port phase 2). Enumeration and the format
/// capability matrix; the capture loop itself lives in `preview_camera.rs`.
#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use v4l::video::Capture;

    /// Fourccs the capture loop can convert to BGRA, in preference order —
    /// raw formats first, JPEG decode last.
    pub(crate) const SUPPORTED_CAPTURE_FOURCCS: [[u8; 4]; 5] =
        [*b"NV12", *b"YUYV", *b"UYVY", *b"YU12", *b"MJPG"];

    pub(crate) fn fourcc_supported(fourcc: &[u8; 4]) -> bool {
        SUPPORTED_CAPTURE_FOURCCS
            .iter()
            .any(|entry| entry == fourcc)
    }

    pub(super) fn list_native_cameras() -> NativeCameraDevices {
        let mut devices = Vec::new();
        let mut warnings = Vec::new();
        let mut nodes: Vec<_> = v4l::context::enum_devices()
            .into_iter()
            .map(|node| node.path().to_path_buf())
            .collect();
        nodes.sort();

        for path in nodes {
            let path_string = path.display().to_string();
            let device = match v4l::Device::with_path(&path) {
                Ok(device) => device,
                Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                    devices.push(Device {
                        id: native_camera_device_id(&path_string),
                        name: path_string.clone(),
                        kind: DeviceKind::Camera,
                        status: DeviceStatus::PermissionRequired,
                        detail: Some(format!(
                            "Could not open {path_string}: permission denied. Add your user \
                             to the 'video' group and log back in."
                        )),
                        width: None,
                        height: None,
                    });
                    continue;
                }
                Err(_) => continue,
            };
            let Ok(caps) = device.query_caps() else {
                continue;
            };
            if !caps
                .capabilities
                .contains(v4l::capability::Flags::VIDEO_CAPTURE)
                || !caps
                    .capabilities
                    .contains(v4l::capability::Flags::STREAMING)
            {
                continue;
            }
            let Ok(descriptions) = device.enum_formats() else {
                continue;
            };
            // UVC metadata nodes and exotic-only devices report no fourcc we
            // can convert; they are not cameras the pipeline can show.
            if !descriptions
                .iter()
                .any(|description| fourcc_supported(&description.fourcc.repr))
            {
                continue;
            }

            let formats = normalize_camera_formats(collect_formats(&device, &descriptions));
            let mut detail = "V4L2 camera.".to_string();
            if let Some(reason) = choose_camera_format(&formats, 1920, 1080, 30)
                .and_then(|choice| choice.fallback_reason)
            {
                detail = format!("{detail} {reason}");
            }
            devices.push(Device {
                id: native_camera_device_id(&path_string),
                name: caps.card.clone(),
                kind: DeviceKind::Camera,
                status: DeviceStatus::Available,
                detail: Some(detail),
                width: None,
                height: None,
            });
        }

        if devices.is_empty() {
            warnings.push("No V4L2 cameras were found under /dev/video*.".to_string());
        }
        NativeCameraDevices { devices, warnings }
    }

    pub(super) fn camera_name_for_unique_id(unique_id: &str) -> Option<String> {
        let device = v4l::Device::with_path(unique_id).ok()?;
        Some(device.query_caps().ok()?.card)
    }

    pub(super) fn camera_capability_matrix_for_unique_id(
        unique_id: &str,
    ) -> Result<Vec<CameraFormatSummary>, String> {
        let device = v4l::Device::with_path(unique_id)
            .map_err(|error| format!("Could not open camera {unique_id}: {error}"))?;
        device_format_summaries(&device)
            .map_err(|error| format!("Could not enumerate formats for {unique_id}: {error}"))
    }

    /// The normalized capability matrix over every convertible fourcc.
    pub(crate) fn device_format_summaries(
        device: &v4l::Device,
    ) -> std::io::Result<Vec<CameraFormatSummary>> {
        let descriptions = device.enum_formats()?;
        Ok(normalize_camera_formats(collect_formats(
            device,
            &descriptions,
        )))
    }

    fn collect_formats(
        device: &v4l::Device,
        descriptions: &[v4l::format::Description],
    ) -> Vec<CameraFormatSummary> {
        let mut formats = Vec::new();
        for description in descriptions {
            if !fourcc_supported(&description.fourcc.repr) {
                continue;
            }
            let Ok(sizes) = device.enum_framesizes(description.fourcc) else {
                continue;
            };
            for size in sizes {
                for discrete in frame_size_candidates(size.size) {
                    let Ok(intervals) = device.enum_frameintervals(
                        description.fourcc,
                        discrete.width,
                        discrete.height,
                    ) else {
                        continue;
                    };
                    let mut min_fps = f64::INFINITY;
                    let mut max_fps: f64 = 0.0;
                    for interval in intervals {
                        let (low, high) = interval_fps_range(interval.interval);
                        min_fps = min_fps.min(low);
                        max_fps = max_fps.max(high);
                    }
                    if max_fps > 0.0 && min_fps.is_finite() {
                        formats.push(CameraFormatSummary {
                            width: discrete.width,
                            height: discrete.height,
                            min_fps,
                            max_fps,
                        });
                    }
                }
            }
        }
        formats
    }

    /// Stepwise size ranges expand only at their corners: per-step expansion
    /// can be thousands of sizes, and the format chooser only needs the hull.
    fn frame_size_candidates(size: v4l::framesize::FrameSizeEnum) -> Vec<v4l::framesize::Discrete> {
        use v4l::framesize::{Discrete, FrameSizeEnum};
        match size {
            FrameSizeEnum::Discrete(discrete) => vec![discrete],
            FrameSizeEnum::Stepwise(stepwise) => vec![
                Discrete {
                    width: stepwise.min_width,
                    height: stepwise.min_height,
                },
                Discrete {
                    width: stepwise.max_width,
                    height: stepwise.max_height,
                },
            ],
        }
    }

    /// A V4L2 frame interval is seconds-per-frame; fps = denominator/numerator.
    fn interval_fps_range(interval: v4l::frameinterval::FrameIntervalEnum) -> (f64, f64) {
        use v4l::frameinterval::FrameIntervalEnum;
        match interval {
            FrameIntervalEnum::Discrete(fraction) => {
                let fps = fraction_fps(fraction);
                (fps, fps)
            }
            // Longest interval = lowest fps and vice versa.
            FrameIntervalEnum::Stepwise(stepwise) => {
                (fraction_fps(stepwise.max), fraction_fps(stepwise.min))
            }
        }
    }

    fn fraction_fps(fraction: v4l::Fraction) -> f64 {
        if fraction.numerator == 0 {
            return 0.0;
        }
        f64::from(fraction.denominator) / f64::from(fraction.numerator)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn interval_fps_maps_seconds_per_frame_to_fps() {
            let (low, high) = interval_fps_range(v4l::frameinterval::FrameIntervalEnum::Discrete(
                v4l::Fraction::new(1, 30),
            ));
            assert_eq!((low, high), (30.0, 30.0));
        }

        #[test]
        fn stepwise_interval_swaps_to_fps_range() {
            let (low, high) = interval_fps_range(v4l::frameinterval::FrameIntervalEnum::Stepwise(
                v4l::frameinterval::Stepwise {
                    min: v4l::Fraction::new(1, 60),
                    max: v4l::Fraction::new(1, 5),
                    step: v4l::Fraction::new(1, 60),
                },
            ));
            assert_eq!((low, high), (5.0, 60.0));
        }

        #[test]
        fn fourcc_preference_covers_camlink_and_common_webcams() {
            assert!(fourcc_supported(b"NV12"));
            assert!(fourcc_supported(b"YUYV"));
            assert!(fourcc_supported(b"MJPG"));
            assert!(!fourcc_supported(b"H264"));
        }
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeVideo};
    use objc2_core_media::CMVideoFormatDescriptionGetDimensions;
    use objc2_foundation::NSString;

    pub fn list_native_cameras() -> NativeCameraDevices {
        let Some(video_media_type) = video_media_type() else {
            return NativeCameraDevices {
                devices: vec![unavailable_camera(
                    "camera:avfoundation-native-media-type-missing",
                    "AVFoundation video media type is unavailable.",
                )],
                warnings: vec!["AVFoundation video media type is unavailable.".to_string()],
            };
        };

        let permission = native_camera_permission();
        let status = camera_permission_status(permission);
        #[allow(deprecated)]
        let devices = unsafe { AVCaptureDevice::devicesWithMediaType(video_media_type) };
        let mut camera_devices = Vec::new();

        for index in 0..devices.count() {
            let camera = devices.objectAtIndex(index);
            let unique_id = unsafe { camera.uniqueID() };
            let unique_id =
                ns_string_to_string(&unique_id).unwrap_or_else(|| format!("unknown-{index}"));
            let name = unsafe { camera.localizedName() };
            let name =
                ns_string_to_string(&name).unwrap_or_else(|| format!("Camera {}", index + 1));
            let formats = normalize_camera_formats(camera_formats(&camera));
            let active_format = active_camera_format_detail(&camera);
            let permission_detail = camera_permission_detail(permission);
            let detail = match (active_format, permission_detail) {
                (Some(active_format), Some(permission_detail)) => {
                    format!("{permission_detail} {active_format}")
                }
                (Some(active_format), None) => active_format,
                (None, Some(permission_detail)) => permission_detail.to_string(),
                (None, None) => {
                    "Native AVFoundation camera. Recording currently uses the FFmpeg fallback bridge."
                        .to_string()
                }
            };

            let choice = choose_camera_format(&formats, 1920, 1080, 30);
            let detail = if let Some(reason) = choice.and_then(|choice| choice.fallback_reason) {
                format!("{detail} {reason}")
            } else {
                detail
            };

            camera_devices.push(Device {
                id: native_camera_device_id(&unique_id),
                name,
                kind: DeviceKind::Camera,
                status: status.clone(),
                detail: Some(detail),
                width: None,
                height: None,
            });
        }

        if camera_devices.is_empty() {
            camera_devices.push(unavailable_camera(
                "camera:avfoundation-native-missing",
                if status == DeviceStatus::PermissionRequired {
                    "AVFoundation did not return cameras. Camera permission may be missing."
                } else {
                    "AVFoundation did not return any video cameras."
                },
            ));
        }

        NativeCameraDevices {
            devices: camera_devices,
            warnings: camera_permission_warning(permission).into_iter().collect(),
        }
    }

    pub fn camera_name_for_unique_id(unique_id: &str) -> Option<String> {
        let unique_id = NSString::from_str(unique_id);
        let camera = unsafe { AVCaptureDevice::deviceWithUniqueID(&unique_id) }?;
        let name = unsafe { camera.localizedName() };
        ns_string_to_string(&name)
    }

    pub fn camera_capability_matrix_for_unique_id(
        unique_id: &str,
    ) -> Result<Vec<CameraFormatSummary>, String> {
        let unique_id = NSString::from_str(unique_id);
        let camera = unsafe { AVCaptureDevice::deviceWithUniqueID(&unique_id) }
            .ok_or_else(|| "Camera device is missing.".to_string())?;
        let formats = normalize_camera_formats(camera_formats(&camera));
        if formats.is_empty() {
            Err("Camera did not report usable AVFoundation video formats.".to_string())
        } else {
            Ok(formats)
        }
    }

    fn native_camera_permission() -> NativeCameraPermission {
        let Some(video_media_type) = video_media_type() else {
            return NativeCameraPermission::Unknown;
        };
        match unsafe { AVCaptureDevice::authorizationStatusForMediaType(video_media_type) } {
            status if status == AVAuthorizationStatus::Authorized => {
                NativeCameraPermission::Authorized
            }
            status if status == AVAuthorizationStatus::NotDetermined => {
                NativeCameraPermission::NotDetermined
            }
            status if status == AVAuthorizationStatus::Denied => NativeCameraPermission::Denied,
            status if status == AVAuthorizationStatus::Restricted => {
                NativeCameraPermission::Restricted
            }
            _ => NativeCameraPermission::Unknown,
        }
    }

    fn video_media_type() -> Option<&'static objc2_av_foundation::AVMediaType> {
        unsafe { AVMediaTypeVideo }
    }

    fn camera_permission_detail(permission: NativeCameraPermission) -> Option<&'static str> {
        match permission {
            NativeCameraPermission::Authorized => None,
            NativeCameraPermission::NotDetermined => {
                Some("Camera permission has not been granted yet.")
            }
            NativeCameraPermission::Denied => Some("Camera permission is denied."),
            NativeCameraPermission::Restricted => Some("Camera permission is restricted by macOS."),
            NativeCameraPermission::Unknown => Some("Camera permission state is unknown."),
        }
    }

    fn camera_permission_warning(permission: NativeCameraPermission) -> Option<String> {
        match permission {
            NativeCameraPermission::Authorized => None,
            NativeCameraPermission::NotDetermined => Some(
                "Camera permission has not been granted yet. Open Camera privacy settings if preview shows black frames."
                    .to_string(),
            ),
            NativeCameraPermission::Denied | NativeCameraPermission::Restricted => Some(
                "Camera permission is blocked. Open macOS Camera privacy settings and enable Videorc or the development shell."
                    .to_string(),
            ),
            NativeCameraPermission::Unknown => {
                Some("Could not determine Camera permission state.".to_string())
            }
        }
    }

    fn camera_formats(camera: &AVCaptureDevice) -> Vec<CameraFormatSummary> {
        let formats = unsafe { camera.formats() };
        let mut summaries = Vec::new();

        for index in 0..formats.count() {
            let format = formats.objectAtIndex(index);
            let description = unsafe { format.formatDescription() };
            let dimensions = unsafe { CMVideoFormatDescriptionGetDimensions(&description) };
            let ranges = unsafe { format.videoSupportedFrameRateRanges() };

            for range_index in 0..ranges.count() {
                let range = ranges.objectAtIndex(range_index);
                summaries.push(CameraFormatSummary {
                    width: dimensions.width.max(0) as u32,
                    height: dimensions.height.max(0) as u32,
                    min_fps: unsafe { range.minFrameRate() },
                    max_fps: unsafe { range.maxFrameRate() },
                });
            }
        }

        summaries
    }

    fn active_camera_format_detail(camera: &AVCaptureDevice) -> Option<String> {
        let active_format = unsafe { camera.activeFormat() };
        let description = unsafe { active_format.formatDescription() };
        let dimensions = unsafe { CMVideoFormatDescriptionGetDimensions(&description) };
        let ranges = unsafe { active_format.videoSupportedFrameRateRanges() };
        let max_fps = max_frame_rate(&ranges);
        let width = dimensions.width.max(0);
        let height = dimensions.height.max(0);

        if width == 0 || height == 0 {
            return None;
        }

        Some(match max_fps {
            Some(max_fps) => format!(
                "Native AVFoundation camera active format: {width}x{height} up to {max_fps:.0} fps. Recording currently uses the FFmpeg fallback bridge."
            ),
            None => format!(
                "Native AVFoundation camera active format: {width}x{height}. Recording currently uses the FFmpeg fallback bridge."
            ),
        })
    }

    fn max_frame_rate(
        ranges: &objc2_foundation::NSArray<objc2_av_foundation::AVFrameRateRange>,
    ) -> Option<f64> {
        let mut max_fps: Option<f64> = None;
        for index in 0..ranges.count() {
            let range = ranges.objectAtIndex(index);
            let fps = unsafe { range.maxFrameRate() };
            max_fps = Some(max_fps.map_or(fps, |current| current.max(fps)));
        }
        max_fps
    }

    fn unavailable_camera(id: &str, detail: &str) -> Device {
        Device {
            id: id.to_string(),
            name: "Camera".to_string(),
            kind: DeviceKind::Camera,
            status: DeviceStatus::Unavailable,
            detail: Some(detail.to_string()),
            width: None,
            height: None,
        }
    }

    fn ns_string_to_string(value: &NSString) -> Option<String> {
        let value = value.to_string();
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_camera_ids_round_trip_unique_ids() {
        let unique_id = "AppleCamera-0x8020000005ac8514";
        let device_id = native_camera_device_id(unique_id);

        assert_eq!(
            parse_native_camera_id(&device_id).as_deref(),
            Some(unique_id)
        );
        assert_eq!(parse_native_camera_id("camera:avfoundation:0"), None);
        assert_eq!(
            parse_native_camera_id("camera:avfoundation-native:not-hex"),
            None
        );
    }

    #[test]
    fn maps_camera_permission_to_device_status() {
        assert_eq!(
            camera_permission_status(NativeCameraPermission::Authorized),
            DeviceStatus::Available
        );
        assert_eq!(
            camera_permission_status(NativeCameraPermission::Denied),
            DeviceStatus::PermissionRequired
        );
        assert_eq!(
            camera_permission_status(NativeCameraPermission::Unknown),
            DeviceStatus::Unavailable
        );
    }

    #[test]
    fn chooses_exact_camera_format_when_available() {
        let formats = vec![
            CameraFormatSummary {
                width: 1280,
                height: 720,
                min_fps: 1.0,
                max_fps: 60.0,
            },
            CameraFormatSummary {
                width: 1920,
                height: 1080,
                min_fps: 1.0,
                max_fps: 30.0,
            },
        ];

        let choice = choose_camera_format(&formats, 1920, 1080, 30).unwrap();

        assert_eq!(choice.format.width, 1920);
        assert_eq!(choice.format.height, 1080);
        assert_eq!(choice.fallback_reason, None);
    }

    #[test]
    fn chooses_clear_camera_format_fallback() {
        let formats = vec![CameraFormatSummary {
            width: 1280,
            height: 720,
            min_fps: 1.0,
            max_fps: 60.0,
        }];

        let choice = choose_camera_format(&formats, 1920, 1080, 30).unwrap();

        assert_eq!(choice.format.width, 1280);
        assert!(choice.fallback_reason.unwrap().contains("not available"));
    }

    #[test]
    fn chooses_smallest_native_format_covering_target_at_requested_fps() {
        let formats = vec![
            CameraFormatSummary {
                width: 640,
                height: 360,
                min_fps: 1.0,
                max_fps: 60.0,
            },
            CameraFormatSummary {
                width: 3840,
                height: 2160,
                min_fps: 1.0,
                max_fps: 30.0,
            },
            CameraFormatSummary {
                width: 1920,
                height: 1080,
                min_fps: 1.0,
                max_fps: 30.0,
            },
        ];

        let choice = choose_camera_format(&formats, 1280, 720, 30).unwrap();

        assert_eq!(choice.format.width, 1920);
        assert_eq!(choice.format.height, 1080);
    }

    #[test]
    fn chooses_largest_format_at_requested_fps_when_no_mode_covers_target() {
        let formats = vec![
            CameraFormatSummary {
                width: 1280,
                height: 720,
                min_fps: 1.0,
                max_fps: 60.0,
            },
            CameraFormatSummary {
                width: 1920,
                height: 1080,
                min_fps: 1.0,
                max_fps: 60.0,
            },
            CameraFormatSummary {
                width: 3840,
                height: 2160,
                min_fps: 1.0,
                max_fps: 30.0,
            },
        ];

        let choice = choose_camera_format(&formats, 3840, 2160, 60).unwrap();

        assert_eq!(choice.format.width, 1920);
        assert_eq!(choice.format.height, 1080);
        assert!(
            choice
                .fallback_reason
                .unwrap()
                .contains("selected native 1920x1080")
        );
    }

    #[test]
    fn falls_back_to_largest_mode_when_requested_fps_is_unavailable() {
        let formats = vec![
            CameraFormatSummary {
                width: 1920,
                height: 1080,
                min_fps: 1.0,
                max_fps: 30.0,
            },
            CameraFormatSummary {
                width: 3840,
                height: 2160,
                min_fps: 1.0,
                max_fps: 30.0,
            },
        ];

        let choice = choose_camera_format(&formats, 3840, 2160, 60).unwrap();

        assert_eq!(choice.format.width, 3840);
        assert_eq!(choice.format.height, 2160);
        assert!(choice.fallback_reason.unwrap().contains("1-30 fps"));
    }

    #[test]
    fn normalizes_camera_format_matrix_for_diagnostics() {
        let formats = normalize_camera_formats(vec![
            CameraFormatSummary {
                width: 0,
                height: 2160,
                min_fps: 1.0,
                max_fps: 60.0,
            },
            CameraFormatSummary {
                width: 3840,
                height: 2160,
                min_fps: 1.0,
                max_fps: 60.0,
            },
            CameraFormatSummary {
                width: 1920,
                height: 1080,
                min_fps: 1.0,
                max_fps: 30.0,
            },
            CameraFormatSummary {
                width: 3840,
                height: 2160,
                min_fps: 1.0,
                max_fps: 60.0,
            },
        ]);

        assert_eq!(
            formats,
            vec![
                CameraFormatSummary {
                    width: 1920,
                    height: 1080,
                    min_fps: 1.0,
                    max_fps: 30.0,
                },
                CameraFormatSummary {
                    width: 3840,
                    height: 2160,
                    min_fps: 1.0,
                    max_fps: 60.0,
                },
            ]
        );
    }
}
