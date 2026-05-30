use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use chrono::Utc;
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, Command};
use tokio::sync::Mutex;
use tokio::time::{Duration, sleep};

use crate::devices::find_avfoundation_screen_index;
use crate::protocol::{RecordingState, RecordingStatus};
use crate::state::AppState;

#[derive(Debug)]
pub struct ActiveRecording {
    pub pid: u32,
    pub stdin: Option<ChildStdin>,
    pub output_path: PathBuf,
    pub started_at: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartRecordingParams {
    pub output_directory: Option<String>,
    pub ffmpeg_path: Option<String>,
}

impl ActiveRecording {
    pub fn status(&self, state: RecordingState, message: Option<String>) -> RecordingStatus {
        RecordingStatus {
            state,
            output_path: Some(self.output_path.display().to_string()),
            started_at: Some(self.started_at.clone()),
            message,
        }
    }
}

pub fn default_recordings_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Movies")
        .join("Videogre")
        .join("Recordings")
}

pub fn idle_status() -> RecordingStatus {
    RecordingStatus {
        state: RecordingState::Idle,
        output_path: None,
        started_at: None,
        message: Some("Ready to record.".to_string()),
    }
}

pub async fn start_test_recording(
    state: AppState,
    params: StartRecordingParams,
) -> Result<RecordingStatus> {
    if state.recording.lock().await.is_some() {
        bail!("A recording is already running");
    }

    let output_dir = params
        .output_directory
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_recordings_dir);
    fs::create_dir_all(&output_dir)
        .await
        .with_context(|| format!("Could not create {}", output_dir.display()))?;

    let started_at = Utc::now();
    let output_path = output_dir.join(format!(
        "videogre-test-{}.mkv",
        started_at.format("%Y%m%d-%H%M%S")
    ));
    let ffmpeg_path = params
        .ffmpeg_path
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "ffmpeg".to_string());

    let recording_mode = if cfg!(target_os = "macos") {
        match find_avfoundation_screen_index(&ffmpeg_path).await {
            Some(screen_index) => RecordingMode::MacScreen { screen_index },
            None => RecordingMode::TestPattern,
        }
    } else {
        RecordingMode::TestPattern
    };
    let args = ffmpeg_args(&recording_mode, &output_path);

    if matches!(recording_mode, RecordingMode::TestPattern) {
        state.emit_log(
            "warn",
            "Using FFmpeg test pattern because macOS screen capture was not detected.",
        );
    }

    state.emit_event(
        "recording.status",
        RecordingStatus {
            state: RecordingState::Starting,
            output_path: Some(output_path.display().to_string()),
            started_at: Some(started_at.to_rfc3339()),
            message: Some("Starting FFmpeg recording.".to_string()),
        },
    );

    let mut child = Command::new(&ffmpeg_path)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Could not start {ffmpeg_path}"))?;

    let stderr = child.stderr.take();
    let stdin = child.stdin.take();
    let pid = child.id().unwrap_or_default();
    let active = ActiveRecording {
        pid,
        stdin,
        output_path: output_path.clone(),
        started_at: started_at.to_rfc3339(),
    };
    let recording_status = active.status(
        RecordingState::Recording,
        Some("Recording with system FFmpeg.".to_string()),
    );

    *state.recording.lock().await = Some(active);
    state.emit_event("recording.status", recording_status.clone());

    if let Some(stderr) = stderr {
        let log_state = state.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    log_state.emit_log("warn", trimmed);
                }
            }
        });
    }

    tokio::spawn(monitor_recording(state.clone(), child, output_path));
    Ok(recording_status)
}

pub async fn stop_recording(state: AppState) -> Result<RecordingStatus> {
    let mut guard = state.recording.lock().await;
    let Some(active) = guard.as_mut() else {
        return Ok(idle_status());
    };

    let pid = active.pid;
    let output_path = active.output_path.clone();
    if let Some(mut stdin) = active.stdin.take() {
        stdin
            .write_all(b"q\n")
            .await
            .context("Could not send stop command to FFmpeg")?;
        let _ = stdin.shutdown().await;
    }

    let status = active.status(
        RecordingState::Stopping,
        Some("Stopping FFmpeg and finalizing the MKV file.".to_string()),
    );
    drop(guard);

    state.emit_event("recording.status", status.clone());
    tokio::spawn(stop_fallback(state.clone(), pid, output_path));
    Ok(status)
}

async fn stop_fallback(state: AppState, pid: u32, output_path: PathBuf) {
    if pid == 0 {
        return;
    }

    sleep(Duration::from_secs(5)).await;

    let still_running = state
        .recording
        .lock()
        .await
        .as_ref()
        .is_some_and(|active| active.pid == pid && active.output_path == output_path);

    if !still_running {
        return;
    }

    state.emit_log(
        "warn",
        "FFmpeg did not stop after stdin quit command; sending SIGTERM.",
    );
    let _ = Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .status()
        .await;
}

async fn monitor_recording(
    state: AppState,
    mut child: tokio::process::Child,
    output_path: PathBuf,
) {
    let status = child.wait().await;
    let mut guard = state.recording.lock().await;
    let had_active_recording = guard.take().is_some();
    drop(guard);

    if !had_active_recording {
        return;
    }

    match status {
        Ok(exit_status) if exit_status.success() => {
            state.emit_log("info", "FFmpeg recording finalized.");
            state.emit_event(
                "recording.status",
                RecordingStatus {
                    state: RecordingState::Idle,
                    output_path: Some(output_path.display().to_string()),
                    started_at: None,
                    message: Some("Recording finalized.".to_string()),
                },
            );
        }
        Ok(exit_status) => {
            let message = format!("FFmpeg exited with {exit_status}");
            state.emit_log("error", &message);
            state.emit_event(
                "recording.status",
                RecordingStatus {
                    state: RecordingState::Failed,
                    output_path: Some(output_path.display().to_string()),
                    started_at: None,
                    message: Some(message),
                },
            );
        }
        Err(error) => {
            let message = format!("Could not wait for FFmpeg: {error}");
            state.emit_log("error", &message);
            state.emit_event(
                "recording.status",
                RecordingStatus {
                    state: RecordingState::Failed,
                    output_path: Some(output_path.display().to_string()),
                    started_at: None,
                    message: Some(message),
                },
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecordingMode {
    MacScreen { screen_index: usize },
    TestPattern,
}

fn ffmpeg_args(mode: &RecordingMode, output_path: &std::path::Path) -> Vec<String> {
    let output = output_path.display().to_string();

    match mode {
        RecordingMode::MacScreen { screen_index } => vec![
            "-y".to_string(),
            "-hide_banner".to_string(),
            "-loglevel".to_string(),
            "warning".to_string(),
            "-f".to_string(),
            "avfoundation".to_string(),
            "-framerate".to_string(),
            "30".to_string(),
            "-capture_cursor".to_string(),
            "1".to_string(),
            "-i".to_string(),
            format!("{screen_index}:none"),
            "-pix_fmt".to_string(),
            "yuv420p".to_string(),
            "-c:v".to_string(),
            "h264_videotoolbox".to_string(),
            "-b:v".to_string(),
            "6000k".to_string(),
            output,
        ],
        RecordingMode::TestPattern => vec![
            "-y".to_string(),
            "-hide_banner".to_string(),
            "-loglevel".to_string(),
            "warning".to_string(),
            "-f".to_string(),
            "lavfi".to_string(),
            "-i".to_string(),
            "testsrc2=size=1280x720:rate=30".to_string(),
            "-f".to_string(),
            "lavfi".to_string(),
            "-i".to_string(),
            "sine=frequency=880:sample_rate=48000".to_string(),
            "-pix_fmt".to_string(),
            "yuv420p".to_string(),
            "-c:v".to_string(),
            "h264_videotoolbox".to_string(),
            "-b:v".to_string(),
            "3000k".to_string(),
            "-c:a".to_string(),
            "aac".to_string(),
            output,
        ],
    }
}

pub type RecordingSlot = Arc<Mutex<Option<ActiveRecording>>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_recordings_dir_uses_videogre_movies_folder() {
        let path = default_recordings_dir();
        let rendered = path.display().to_string();

        assert!(rendered.contains("Movies"));
        assert!(rendered.ends_with("Videogre/Recordings"));
    }

    #[test]
    fn mac_screen_args_do_not_cross_electron_ipc() {
        let args = ffmpeg_args(
            &RecordingMode::MacScreen { screen_index: 3 },
            std::path::Path::new("/tmp/videogre-test.mkv"),
        );

        assert!(args.contains(&"avfoundation".to_string()));
        assert!(args.contains(&"3:none".to_string()));
        assert!(args.contains(&"h264_videotoolbox".to_string()));
    }
}
