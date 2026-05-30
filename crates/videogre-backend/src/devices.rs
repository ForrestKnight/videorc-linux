use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;
use tokio::time::timeout;

use crate::protocol::{Device, DeviceKind, DeviceList, DeviceStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvFoundationDevice {
    pub index: usize,
    pub name: String,
    pub kind: AvFoundationDeviceKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AvFoundationDeviceKind {
    Video,
    Audio,
}

pub async fn list_devices(ffmpeg_path: &str) -> DeviceList {
    let mut devices = vec![
        Device {
            id: "window:native-adapter-pending".to_string(),
            name: "Window Capture".to_string(),
            kind: DeviceKind::Window,
            status: DeviceStatus::Unavailable,
            detail: Some(
                "Native window capture adapter is not implemented in this spike.".to_string(),
            ),
        },
        Device {
            id: "system-audio:native-adapter-pending".to_string(),
            name: "System Audio".to_string(),
            kind: DeviceKind::SystemAudio,
            status: DeviceStatus::Unavailable,
            detail: Some("System audio capture depends on the native macOS adapter.".to_string()),
        },
    ];
    let mut warnings = Vec::new();

    if !cfg!(target_os = "macos") {
        devices.insert(
            0,
            Device {
                id: "screen:unsupported-platform".to_string(),
                name: "Primary Display".to_string(),
                kind: DeviceKind::Screen,
                status: DeviceStatus::Unavailable,
                detail: Some("This spike only probes macOS devices.".to_string()),
            },
        );
        warnings.push("Device probing is only implemented for macOS in this spike.".to_string());
        return DeviceList { devices, warnings };
    }

    match probe_avfoundation_devices(ffmpeg_path).await {
        Ok(av_devices) => {
            let screen = av_devices.iter().find(|device| {
                device.kind == AvFoundationDeviceKind::Video
                    && device.name.to_lowercase().contains("capture screen")
            });

            devices.insert(
                0,
                Device {
                    id: screen
                        .map(|device| format!("screen:avfoundation:{}", device.index))
                        .unwrap_or_else(|| "screen:avfoundation-missing".to_string()),
                    name: screen
                        .map(|device| device.name.clone())
                        .unwrap_or_else(|| "Primary Display".to_string()),
                    kind: DeviceKind::Screen,
                    status: if screen.is_some() {
                        DeviceStatus::Available
                    } else {
                        DeviceStatus::PermissionRequired
                    },
                    detail: if screen.is_some() {
                        Some("Detected by FFmpeg avfoundation.".to_string())
                    } else {
                        Some(
                            "FFmpeg did not report a screen device. macOS Screen Recording permission may be missing."
                                .to_string(),
                        )
                    },
                },
            );

            for device in av_devices {
                match device.kind {
                    AvFoundationDeviceKind::Video => {
                        if !device.name.to_lowercase().contains("capture screen") {
                            devices.push(Device {
                                id: format!("camera:avfoundation:{}", device.index),
                                name: device.name,
                                kind: DeviceKind::Camera,
                                status: DeviceStatus::Available,
                                detail: Some("Detected by FFmpeg avfoundation.".to_string()),
                            });
                        }
                    }
                    AvFoundationDeviceKind::Audio => devices.push(Device {
                        id: format!("microphone:avfoundation:{}", device.index),
                        name: device.name,
                        kind: DeviceKind::Microphone,
                        status: DeviceStatus::Available,
                        detail: Some("Detected by FFmpeg avfoundation.".to_string()),
                    }),
                }
            }
        }
        Err(error) => {
            devices.insert(
                0,
                Device {
                    id: "screen:probe-failed".to_string(),
                    name: "Primary Display".to_string(),
                    kind: DeviceKind::Screen,
                    status: DeviceStatus::Unavailable,
                    detail: Some("FFmpeg avfoundation probing failed.".to_string()),
                },
            );
            warnings.push(format!("FFmpeg device probe failed: {error}"));
        }
    }

    DeviceList { devices, warnings }
}

pub async fn find_avfoundation_screen_index(ffmpeg_path: &str) -> Option<usize> {
    probe_avfoundation_devices(ffmpeg_path)
        .await
        .ok()?
        .into_iter()
        .find(|device| {
            device.kind == AvFoundationDeviceKind::Video
                && device.name.to_lowercase().contains("capture screen")
        })
        .map(|device| device.index)
}

pub async fn probe_avfoundation_devices(
    ffmpeg_path: &str,
) -> Result<Vec<AvFoundationDevice>, String> {
    let mut command = Command::new(ffmpeg_path);
    command
        .args([
            "-hide_banner",
            "-f",
            "avfoundation",
            "-list_devices",
            "true",
            "-i",
            "",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = timeout(Duration::from_secs(6), command.output())
        .await
        .map_err(|_| "FFmpeg device probe timed out".to_string())?
        .map_err(|error| format!("Could not run {ffmpeg_path}: {error}"))?;

    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let parsed = parse_avfoundation_devices(&text);

    if parsed.is_empty() {
        Err(if text.trim().is_empty() {
            "FFmpeg returned no avfoundation device output".to_string()
        } else {
            text.lines()
                .next()
                .unwrap_or("No devices found")
                .to_string()
        })
    } else {
        Ok(parsed)
    }
}

pub fn parse_avfoundation_devices(text: &str) -> Vec<AvFoundationDevice> {
    let mut section: Option<AvFoundationDeviceKind> = None;
    let mut devices = Vec::new();

    for line in text.lines() {
        if line.contains("AVFoundation video devices") {
            section = Some(AvFoundationDeviceKind::Video);
            continue;
        }

        if line.contains("AVFoundation audio devices") {
            section = Some(AvFoundationDeviceKind::Audio);
            continue;
        }

        let Some(kind) = section.clone() else {
            continue;
        };

        if let Some((index, name)) = parse_indexed_device_line(line) {
            devices.push(AvFoundationDevice { index, name, kind });
        }
    }

    devices
}

fn parse_indexed_device_line(line: &str) -> Option<(usize, String)> {
    let marker = "] [";
    let after_marker = line.split(marker).nth(1)?;
    let closing_bracket = after_marker.find(']')?;
    let index = after_marker[..closing_bracket].parse().ok()?;
    let name = after_marker[closing_bracket + 1..].trim();

    if name.is_empty() {
        None
    } else {
        Some((index, name.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_avfoundation_device_listing() {
        let text = r#"
[AVFoundation indev @ 0x123] AVFoundation video devices:
[AVFoundation indev @ 0x123] [0] FaceTime HD Camera
[AVFoundation indev @ 0x123] [1] Capture screen 0
[AVFoundation indev @ 0x123] AVFoundation audio devices:
[AVFoundation indev @ 0x123] [0] MacBook Pro Microphone
"#;

        let devices = parse_avfoundation_devices(text);

        assert_eq!(
            devices,
            vec![
                AvFoundationDevice {
                    index: 0,
                    name: "FaceTime HD Camera".to_string(),
                    kind: AvFoundationDeviceKind::Video,
                },
                AvFoundationDevice {
                    index: 1,
                    name: "Capture screen 0".to_string(),
                    kind: AvFoundationDeviceKind::Video,
                },
                AvFoundationDevice {
                    index: 0,
                    name: "MacBook Pro Microphone".to_string(),
                    kind: AvFoundationDeviceKind::Audio,
                },
            ]
        );
    }
}
