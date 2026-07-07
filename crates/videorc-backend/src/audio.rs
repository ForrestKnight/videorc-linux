#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
// The remaining top-level `bail!` callers are the macOS listing and the
// other-platform stubs; the Linux implementation imports its own.
#[cfg(not(target_os = "linux"))]
use anyhow::bail;

use crate::protocol::{AudioMeterResult, AudioMeterStatus, Device, DeviceKind, DeviceStatus};

pub const NATIVE_AUDIO_SAMPLE_RATE: u32 = 48_000;
pub const NATIVE_AUDIO_CHANNELS: u16 = 2;
/// Minimum elapsed capture time before audio-capture coverage is meaningful — below this
/// the sampled count is too noisy (start-up jitter, FIFO preroll) to judge a gap.
pub const AUDIO_COVERAGE_WARMUP_SECS: f64 = 3.0;
// CoreAudio must never block on the realtime callback. The recording bridge can briefly
// stop reading the audio FIFO while probing/flushing the H.264 video FIFO, so keep a
// bounded multi-second packet cushion instead of dropping valid mic callbacks.
const AUDIO_RING_CAPACITY_PACKETS: usize = 1024;
const METER_SAMPLE_DURATION: Duration = Duration::from_millis(700);
const FIFO_OPEN_RETRY: Duration = Duration::from_millis(20);
pub const NATIVE_AUDIO_FFMPEG_QUEUE_SIZE: u32 = 1024;

/// Fraction of the expected audio sample-frames that were actually captured over the
/// elapsed window: `captured / (elapsed × sample_rate)`. 1.0 means full real-time
/// coverage; values meaningfully below 1.0 indicate the mic stalled (a capture gap).
/// Returns `None` before [`AUDIO_COVERAGE_WARMUP_SECS`] (too little signal) or when the
/// sample rate is zero. Pure and deterministic, so it is unit-tested directly.
pub fn audio_capture_coverage(
    captured_frames: u64,
    elapsed_secs: f64,
    sample_rate: u32,
) -> Option<f64> {
    if sample_rate == 0 || !elapsed_secs.is_finite() || elapsed_secs < AUDIO_COVERAGE_WARMUP_SECS {
        return None;
    }
    let expected = elapsed_secs * f64::from(sample_rate);
    if expected <= 0.0 {
        return None;
    }
    Some(captured_frames as f64 / expected)
}

#[derive(Debug, Clone)]
pub struct AudioFrame {
    pub timestamp_micros: u64,
    pub captured_at: Instant,
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

impl AudioFrame {
    pub fn frame_count(&self) -> usize {
        if self.channels == 0 {
            return 0;
        }
        self.samples.len() / usize::from(self.channels)
    }

    fn duration(&self) -> Duration {
        if self.sample_rate == 0 {
            return Duration::ZERO;
        }
        Duration::from_secs_f64(self.frame_count() as f64 / f64::from(self.sample_rate))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AudioProcessingSettings {
    pub gain_db: f32,
    pub muted: bool,
}

impl Default for AudioProcessingSettings {
    fn default() -> Self {
        Self {
            gain_db: 0.0,
            muted: false,
        }
    }
}

#[derive(Debug, Default)]
pub struct AudioCaptureStats {
    captured_frames: AtomicU64,
    dropped_frames: AtomicU64,
    fifo_write_errors: AtomicU64,
    recording_window_finished: AtomicBool,
    // Peak amplitude of the most recent frame window (milli-units, 0..=1000):
    // the Studio mixer's live meter reads this via the diagnostics sampler —
    // no extra device open (post-0.9.4 fix batch F7).
    live_peak_milli: AtomicU64,
    // Loudest peak seen over the whole recording window (same milli-units).
    // A TCC-unauthorized process gets SILENT ZEROS from CoreAudio, not an
    // error — frames keep counting while the track holds nothing. This is the
    // truthful "did the mic capture any sound at all" signal (plan 021 F3).
    session_peak_milli: AtomicU64,
}

impl AudioCaptureStats {
    pub fn captured_frames(&self) -> u64 {
        self.captured_frames.load(Ordering::Relaxed)
    }

    pub fn dropped_frames(&self) -> u64 {
        self.dropped_frames.load(Ordering::Relaxed)
    }

    pub fn live_peak(&self) -> f32 {
        self.live_peak_milli.load(Ordering::Relaxed) as f32 / 1000.0
    }

    pub fn session_peak(&self) -> f32 {
        self.session_peak_milli.load(Ordering::Relaxed) as f32 / 1000.0
    }

    fn record_live_peak(&self, peak: f32) {
        let clamped = (peak.clamp(0.0, 1.0) * 1000.0) as u64;
        self.live_peak_milli.store(clamped, Ordering::Relaxed);
        self.session_peak_milli
            .fetch_max(clamped, Ordering::Relaxed);
    }

    fn reset_recording_window(&self) {
        self.captured_frames.store(0, Ordering::Relaxed);
        self.dropped_frames.store(0, Ordering::Relaxed);
        self.fifo_write_errors.store(0, Ordering::Relaxed);
        self.session_peak_milli.store(0, Ordering::Relaxed);
        self.recording_window_finished
            .store(false, Ordering::Relaxed);
    }

    fn finish_recording_window(&self) {
        self.recording_window_finished
            .store(true, Ordering::Relaxed);
    }

    fn recording_window_finished(&self) -> bool {
        self.recording_window_finished.load(Ordering::Relaxed)
    }

    fn record_captured_frames(&self, frames: u64) {
        if !self.recording_window_finished() {
            self.captured_frames.fetch_add(frames, Ordering::Relaxed);
        }
    }

    fn record_dropped_frames(&self, frames: u64) {
        if !self.recording_window_finished() {
            self.dropped_frames.fetch_add(frames, Ordering::Relaxed);
        }
    }
}

pub struct NativeAudioSource {
    pub device_id: u32,
    pub device_name: String,
    receiver: Option<mpsc::Receiver<AudioFrame>>,
    stats: Arc<AudioCaptureStats>,
    stop: Arc<AtomicBool>,
    stop_on_drop: bool,
    #[cfg(target_os = "macos")]
    audio_unit: Option<coreaudio::audio_unit::AudioUnit>,
    #[cfg(target_os = "linux")]
    pipewire_capture: Option<linux::PipewireCaptureHandle>,
}

impl NativeAudioSource {
    /// A cloneable, `Send` handle to the capture stats — lets callers poll mic warmup
    /// across await points without holding the (`!Send`) source itself. `captured_frames`
    /// stays zero until CoreAudio delivers its first callback (the warmed-up signal).
    pub fn stats_handle(&self) -> Arc<AudioCaptureStats> {
        self.stats.clone()
    }
}

impl std::fmt::Debug for NativeAudioSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeAudioSource")
            .field("device_id", &self.device_id)
            .field("device_name", &self.device_name)
            .field("captured_frames", &self.stats.captured_frames())
            .field("dropped_frames", &self.stats.dropped_frames())
            .finish_non_exhaustive()
    }
}

impl Drop for NativeAudioSource {
    fn drop(&mut self) {
        if !self.stop_on_drop {
            return;
        }
        self.stop.store(true, Ordering::Relaxed);
        #[cfg(target_os = "macos")]
        if let Some(audio_unit) = self.audio_unit.as_mut() {
            let _ = audio_unit.stop();
        }
        #[cfg(target_os = "linux")]
        if let Some(capture) = self.pipewire_capture.as_mut() {
            capture.stop();
        }
    }
}

pub struct NativeAudioCaptureSession {
    pub device_id: u32,
    pub device_name: String,
    pub fifo_path: PathBuf,
    stats: Arc<AudioCaptureStats>,
    stop: Arc<AtomicBool>,
    writer: Option<thread::JoinHandle<()>>,
    #[cfg(target_os = "macos")]
    audio_unit: Option<coreaudio::audio_unit::AudioUnit>,
    #[cfg(target_os = "linux")]
    pipewire_capture: Option<linux::PipewireCaptureHandle>,
}

impl std::fmt::Debug for NativeAudioCaptureSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeAudioCaptureSession")
            .field("device_id", &self.device_id)
            .field("device_name", &self.device_name)
            .field("fifo_path", &self.fifo_path)
            .field("captured_frames", &self.captured_frames())
            .field("dropped_frames", &self.dropped_frames())
            .finish_non_exhaustive()
    }
}

impl NativeAudioCaptureSession {
    pub fn captured_frames(&self) -> u64 {
        self.stats.captured_frames()
    }

    pub fn dropped_frames(&self) -> u64 {
        self.stats.dropped_frames()
    }

    pub fn live_peak(&self) -> f32 {
        self.stats.live_peak()
    }

    pub fn session_peak(&self) -> f32 {
        self.stats.session_peak()
    }

    pub fn finish_recording_window(&self) {
        self.stats.finish_recording_window();
    }
}

impl Drop for NativeAudioCaptureSession {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
        #[cfg(target_os = "macos")]
        if let Some(audio_unit) = self.audio_unit.as_mut() {
            let _ = audio_unit.stop();
        }
        #[cfg(target_os = "linux")]
        if let Some(capture) = self.pipewire_capture.as_mut() {
            capture.stop();
        }
        let _ = std::fs::remove_file(&self.fifo_path);
    }
}

pub fn parse_coreaudio_microphone_id(id: &str) -> Option<u32> {
    id.strip_prefix("microphone:coreaudio:")?.parse().ok()
}

#[cfg(target_os = "linux")]
pub fn parse_pipewire_microphone_id(id: &str) -> Option<u32> {
    id.strip_prefix("microphone:pipewire:")?.parse().ok()
}

/// Parse the current platform's native microphone id (CoreAudio on macOS,
/// PipeWire on Linux) into the shared u32 device id. Platforms without a
/// native capture path never match, so their sessions fall back exactly as
/// before.
pub fn parse_native_microphone_id(id: &str) -> Option<u32> {
    #[cfg(target_os = "macos")]
    {
        parse_coreaudio_microphone_id(id)
    }
    #[cfg(target_os = "linux")]
    {
        parse_pipewire_microphone_id(id)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = id;
        None
    }
}

pub fn native_audio_fifo_path(session_id: &str) -> PathBuf {
    std::env::temp_dir().join(format!("videorc-audio-{session_id}.f32le"))
}

pub fn create_native_audio_fifo(path: &Path) -> Result<()> {
    if path.exists() {
        std::fs::remove_file(path)
            .with_context(|| format!("Could not remove stale audio FIFO {}", path.display()))?;
    }

    crate::fifo::create(path)
        .with_context(|| format!("Could not create audio FIFO {}", path.display()))
}

pub fn start_native_audio_source(
    device_id: u32,
    settings: AudioProcessingSettings,
) -> Result<NativeAudioSource> {
    start_platform_audio_source(device_id, settings)
}

/// How long the FIFO writer waits for the encoder bridge to deliver its first video
/// frame before giving up on epoch alignment (mirrors the recording startup budget).
const VIDEO_EPOCH_WAIT_TIMEOUT: Duration = Duration::from_secs(20);

struct AudioPreroll {
    discarded_frames: u64,
    ready_frames: VecDeque<AudioFrame>,
}

/// Discard queued audio until the shared video epoch is set (the encoder bridge's
/// first composited video frame), so the first audio sample written corresponds to the
/// same instant as the first video frame. This replaces the old calibrated constant:
/// the video pipeline's startup latency varies with resolution (4K warms slower than
/// 1080p), so no fixed offset can align both. Returns the discarded frame count plus
/// any already-queued audio captured at/after the epoch, or None when the wait timed
/// out and the writer should proceed unaligned.
fn discard_audio_until_video_epoch(
    receiver: &mpsc::Receiver<AudioFrame>,
    video_epoch: &OnceLock<Instant>,
    stop: &AtomicBool,
) -> Option<AudioPreroll> {
    let waited_since = Instant::now();
    let mut pending = VecDeque::new();
    loop {
        if let Some(epoch) = video_epoch.get().copied() {
            return Some(discard_audio_before_epoch(receiver, pending, epoch, stop));
        }
        if stop.load(Ordering::Relaxed) || waited_since.elapsed() >= VIDEO_EPOCH_WAIT_TIMEOUT {
            return None;
        }
        match receiver.recv_timeout(Duration::from_millis(2)) {
            Ok(frame) => pending.push_back(frame),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Some(AudioPreroll {
                    discarded_frames: pending.iter().map(|frame| frame.frame_count() as u64).sum(),
                    ready_frames: VecDeque::new(),
                });
            }
        }
    }
}

fn discard_audio_before_epoch(
    receiver: &mpsc::Receiver<AudioFrame>,
    mut pending: VecDeque<AudioFrame>,
    epoch: Instant,
    stop: &AtomicBool,
) -> AudioPreroll {
    let mut discarded_frames = 0_u64;
    let mut ready_frames = VecDeque::new();

    loop {
        let frame = if let Some(frame) = pending.pop_front() {
            frame
        } else if ready_frames.is_empty() && !stop.load(Ordering::Relaxed) {
            match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(frame) => frame,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    return AudioPreroll {
                        discarded_frames,
                        ready_frames,
                    };
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return AudioPreroll {
                        discarded_frames,
                        ready_frames,
                    };
                }
            }
        } else {
            return AudioPreroll {
                discarded_frames,
                ready_frames,
            };
        };

        let trimmed = trim_audio_frame_before_epoch(frame, epoch);
        discarded_frames = discarded_frames.saturating_add(trimmed.discarded_frames);
        if let Some(frame) = trimmed.frame {
            ready_frames.push_back(frame);
            ready_frames.extend(pending);
            return AudioPreroll {
                discarded_frames,
                ready_frames,
            };
        }
    }
}

struct TrimmedAudioFrame {
    discarded_frames: u64,
    frame: Option<AudioFrame>,
}

fn trim_audio_frame_before_epoch(mut frame: AudioFrame, epoch: Instant) -> TrimmedAudioFrame {
    let frame_count = frame.frame_count();
    if frame_count == 0 || frame.sample_rate == 0 || frame.channels == 0 {
        return TrimmedAudioFrame {
            discarded_frames: frame_count as u64,
            frame: None,
        };
    }

    let frame_end = frame.captured_at;
    let frame_start = frame_end.checked_sub(frame.duration()).unwrap_or(frame_end);
    if frame_end <= epoch {
        return TrimmedAudioFrame {
            discarded_frames: frame_count as u64,
            frame: None,
        };
    }
    if frame_start >= epoch {
        return TrimmedAudioFrame {
            discarded_frames: 0,
            frame: Some(frame),
        };
    }

    let trim_duration = epoch.duration_since(frame_start);
    let frames_to_trim = ((trim_duration.as_secs_f64() * f64::from(frame.sample_rate)).round()
        as usize)
        .min(frame_count);
    if frames_to_trim == 0 {
        return TrimmedAudioFrame {
            discarded_frames: 0,
            frame: Some(frame),
        };
    }
    if frames_to_trim >= frame_count {
        return TrimmedAudioFrame {
            discarded_frames: frame_count as u64,
            frame: None,
        };
    }

    let sample_offset = frames_to_trim * usize::from(frame.channels);
    frame.samples = frame.samples[sample_offset..].to_vec();
    frame.timestamp_micros = frame.timestamp_micros.saturating_add(
        (frames_to_trim as u64).saturating_mul(1_000_000) / u64::from(frame.sample_rate),
    );
    TrimmedAudioFrame {
        discarded_frames: frames_to_trim as u64,
        frame: Some(frame),
    }
}

pub fn attach_fifo_writer(
    mut source: NativeAudioSource,
    fifo_path: PathBuf,
    video_epoch: Option<Arc<OnceLock<Instant>>>,
) -> NativeAudioCaptureSession {
    source.stop_on_drop = false;
    let device_id = source.device_id;
    let device_name = std::mem::take(&mut source.device_name);
    let receiver = source
        .receiver
        .take()
        .expect("native audio source receiver is available before attaching FIFO writer");
    let stats = source.stats.clone();
    let stop = source.stop.clone();
    #[cfg(target_os = "macos")]
    let audio_unit = source.audio_unit.take();
    #[cfg(target_os = "linux")]
    let pipewire_capture = source.pipewire_capture.take();

    let writer_stats = stats.clone();
    let writer_stop = stop.clone();
    let writer_path = fifo_path.clone();
    // Clear warmup/pre-roll counters before the session is published as active; the
    // writer thread repeats this after it drains queued pre-roll frames.
    stats.reset_recording_window();
    let writer = thread::spawn(move || {
        let mut file = match open_fifo_writer(&writer_path, &writer_stop) {
            Ok(file) => file,
            Err(error) => {
                writer_stats
                    .fifo_write_errors
                    .fetch_add(1, Ordering::Relaxed);
                tracing::warn!("Could not open native audio FIFO: {error}");
                return;
            }
        };

        let preroll = match video_epoch.as_deref() {
            Some(epoch) => match discard_audio_until_video_epoch(&receiver, epoch, &writer_stop) {
                Some(preroll) => preroll,
                None => {
                    tracing::warn!(
                        "Video epoch never arrived; writing native audio without epoch alignment."
                    );
                    AudioPreroll {
                        discarded_frames: discard_preroll_audio_frames(&receiver),
                        ready_frames: VecDeque::new(),
                    }
                }
            },
            None => AudioPreroll {
                discarded_frames: discard_preroll_audio_frames(&receiver),
                ready_frames: VecDeque::new(),
            },
        };
        if preroll.discarded_frames > 0 {
            tracing::info!(
                "Discarded {} native audio pre-roll frames before starting the recording FIFO.",
                preroll.discarded_frames
            );
        }
        // Warmup starts CoreAudio before FFmpeg is ready, so the bounded callback queue can
        // legitimately fill during pre-roll. The live recording diagnostics should count
        // only frames captured/dropped after the FIFO is open and pre-roll has been
        // discarded.
        writer_stats.reset_recording_window();

        for frame in preroll.ready_frames {
            let frame_count = frame.frame_count() as u64;
            if let Err(error) = write_frame_f32le(&mut file, &frame) {
                writer_stats
                    .fifo_write_errors
                    .fetch_add(1, Ordering::Relaxed);
                tracing::warn!("Could not write native audio frame: {error}");
                return;
            }
            writer_stats.record_captured_frames(frame_count);
        }

        while !writer_stop.load(Ordering::Relaxed) {
            match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(frame) => {
                    // Live captions listen on the same mic frames; the offer is
                    // a relaxed-atomic no-op unless a caption session is active
                    // and never blocks this writer.
                    crate::captions::offer_caption_frame(&frame);
                    let frame_peak = frame
                        .samples
                        .iter()
                        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
                    writer_stats.record_live_peak(frame_peak);
                    if let Err(error) = write_frame_f32le(&mut file, &frame) {
                        writer_stats
                            .fifo_write_errors
                            .fetch_add(1, Ordering::Relaxed);
                        tracing::warn!("Could not write native audio frame: {error}");
                        break;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });

    NativeAudioCaptureSession {
        device_id,
        device_name,
        fifo_path,
        stats,
        stop,
        writer: Some(writer),
        #[cfg(target_os = "macos")]
        audio_unit,
        #[cfg(target_os = "linux")]
        pipewire_capture,
    }
}

fn discard_preroll_audio_frames(receiver: &mpsc::Receiver<AudioFrame>) -> u64 {
    let mut discarded = 0_u64;
    while let Ok(frame) = receiver.try_recv() {
        discarded = discarded.saturating_add(frame.frame_count() as u64);
    }
    discarded
}

pub fn sample_native_audio_meter(
    device_id: u32,
    settings: AudioProcessingSettings,
) -> AudioMeterResult {
    match start_native_audio_source(device_id, settings) {
        Ok(source) => sample_meter_from_source(source, METER_SAMPLE_DURATION),
        Err(error) => AudioMeterResult {
            status: permission_or_unavailable(&error.to_string()),
            level: None,
            peak_db: None,
            mean_db: None,
            message: Some(error.to_string()),
        },
    }
}

pub fn list_native_microphones() -> Vec<Device> {
    match list_platform_microphones() {
        Ok(devices) => devices,
        Err(error) => vec![Device {
            id: "microphone:coreaudio-unavailable".to_string(),
            name: "Native microphone capture".to_string(),
            kind: DeviceKind::Microphone,
            status: DeviceStatus::Unavailable,
            detail: Some(error.to_string()),
            width: None,
            height: None,
        }],
    }
}

fn sample_meter_from_source(mut source: NativeAudioSource, duration: Duration) -> AudioMeterResult {
    let receiver = source
        .receiver
        .take()
        .expect("native audio source receiver is available before sampling meter");
    let started = Instant::now();
    let mut peak = 0.0_f32;
    let mut sum_squares = 0.0_f64;
    let mut samples = 0_u64;

    while started.elapsed() < duration {
        match receiver.recv_timeout(Duration::from_millis(80)) {
            Ok(frame) => {
                for sample in frame.samples {
                    let value = sample.abs();
                    peak = peak.max(value);
                    sum_squares += f64::from(value * value);
                    samples += 1;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    if samples == 0 {
        return AudioMeterResult {
            status: AudioMeterStatus::NoFrames,
            level: None,
            peak_db: None,
            mean_db: None,
            message: Some(
                "This microphone opened but did not send audio frames. Try a fallback input or another mic."
                    .to_string(),
            ),
        };
    }

    let peak_db = amplitude_to_db(peak);
    let rms = (sum_squares / samples as f64).sqrt() as f32;
    let mean_db = amplitude_to_db(rms);
    let level = db_to_level(peak_db);
    let silent = peak_db <= -55.0;

    AudioMeterResult {
        status: if silent {
            AudioMeterStatus::Silent
        } else {
            AudioMeterStatus::Ready
        },
        level: Some(level),
        peak_db: Some(f64::from(peak_db)),
        mean_db: Some(f64::from(mean_db)),
        message: Some(if silent {
            "Native microphone signal is very low.".to_string()
        } else {
            "Native microphone signal detected.".to_string()
        }),
    }
}

fn open_fifo_writer(path: &Path, stop: &AtomicBool) -> io::Result<File> {
    crate::fifo::open_writer(
        path,
        stop,
        FIFO_OPEN_RETRY,
        true,
        "native audio writer stopped before FIFO opened",
    )
}

fn write_frame_f32le(file: &mut File, frame: &AudioFrame) -> io::Result<()> {
    if frame.sample_rate != NATIVE_AUDIO_SAMPLE_RATE || frame.channels != NATIVE_AUDIO_CHANNELS {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "native audio frame format does not match FFmpeg FIFO format",
        ));
    }
    let _timestamp_micros = frame.timestamp_micros;
    let mut bytes = Vec::with_capacity(frame.samples.len() * std::mem::size_of::<f32>());
    for sample in &frame.samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    file.write_all(&bytes)
}

pub fn process_interleaved_f32(
    input: &[f32],
    source_channels: usize,
    settings: AudioProcessingSettings,
) -> Vec<f32> {
    if input.is_empty() || source_channels == 0 {
        return Vec::new();
    }

    let gain = if settings.muted {
        0.0
    } else {
        db_to_gain(settings.gain_db)
    };
    let frame_count = input.len() / source_channels;
    let mut output = Vec::with_capacity(frame_count * usize::from(NATIVE_AUDIO_CHANNELS));

    for frame_index in 0..frame_count {
        let base = frame_index * source_channels;
        let mono = if source_channels > 1 {
            centered_voice_sample(
                input[base],
                input.get(base + 1).copied().unwrap_or(input[base]),
            )
        } else {
            input[base]
        };
        let sample = (mono * gain).clamp(-1.0, 1.0);
        output.push(sample);
        output.push(sample);
    }

    output
}

fn centered_voice_sample(left: f32, right: f32) -> f32 {
    const SILENT_CHANNEL_THRESHOLD: f32 = 1.0e-5;

    match (
        left.abs() > SILENT_CHANNEL_THRESHOLD,
        right.abs() > SILENT_CHANNEL_THRESHOLD,
    ) {
        (true, true) => (left + right) * 0.5,
        (true, false) => left,
        (false, true) => right,
        (false, false) => 0.0,
    }
}

#[cfg(test)]
fn fake_pcm_frames(frame_count: usize, chunk_frames: usize, frequency_hz: f32) -> Vec<AudioFrame> {
    let mut frames = Vec::new();
    let mut produced = 0usize;

    while produced < frame_count {
        let current_frames = chunk_frames.min(frame_count - produced);
        let mut samples = Vec::with_capacity(current_frames * usize::from(NATIVE_AUDIO_CHANNELS));
        for frame_offset in 0..current_frames {
            let phase = ((produced + frame_offset) as f32 * frequency_hz * std::f32::consts::TAU)
                / NATIVE_AUDIO_SAMPLE_RATE as f32;
            let sample = phase.sin() * 0.25;
            samples.push(sample);
            samples.push(sample);
        }
        frames.push(AudioFrame {
            timestamp_micros: timestamp_for_frame(produced as u64),
            captured_at: Instant::now(),
            sample_rate: NATIVE_AUDIO_SAMPLE_RATE,
            channels: NATIVE_AUDIO_CHANNELS,
            samples,
        });
        produced += current_frames;
    }

    frames
}

fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

pub(crate) fn amplitude_to_db(amplitude: f32) -> f32 {
    if amplitude <= f32::EPSILON {
        -90.0
    } else {
        20.0 * amplitude.log10()
    }
}

pub(crate) fn db_to_level(db: f32) -> f64 {
    f64::from(((db + 60.0) / 60.0).clamp(0.0, 1.0))
}

fn timestamp_for_frame(frame_cursor: u64) -> u64 {
    frame_cursor.saturating_mul(1_000_000) / u64::from(NATIVE_AUDIO_SAMPLE_RATE)
}

fn permission_or_unavailable(message: &str) -> AudioMeterStatus {
    let lower = message.to_lowercase();
    if lower.contains("permission") || lower.contains("unauthor") {
        AudioMeterStatus::PermissionRequired
    } else {
        AudioMeterStatus::Unavailable
    }
}

#[cfg(target_os = "macos")]
fn start_platform_audio_source(
    device_id: u32,
    settings: AudioProcessingSettings,
) -> Result<NativeAudioSource> {
    use coreaudio::audio_unit::audio_format::LinearPcmFlags;
    use coreaudio::audio_unit::macos_helpers::{audio_unit_from_device_id, get_device_name};
    use coreaudio::audio_unit::render_callback::{self, data};
    use coreaudio::audio_unit::{Element, SampleFormat, Scope, StreamFormat};
    use std::sync::mpsc::TrySendError;

    let device_name =
        get_device_name(device_id).unwrap_or_else(|_| format!("CoreAudio device {device_id}"));
    let mut audio_unit = audio_unit_from_device_id(device_id, true)
        .with_context(|| format!("Could not open CoreAudio input device {device_name}"))?;

    let stream_format = StreamFormat {
        sample_rate: f64::from(NATIVE_AUDIO_SAMPLE_RATE),
        sample_format: SampleFormat::F32,
        flags: LinearPcmFlags::IS_FLOAT | LinearPcmFlags::IS_PACKED,
        channels: u32::from(NATIVE_AUDIO_CHANNELS),
    };
    audio_unit
        .set_stream_format(stream_format, Scope::Output, Element::Input)
        .with_context(|| format!("Could not set CoreAudio stream format for {device_name}"))?;

    let (sender, receiver) = mpsc::sync_channel(AUDIO_RING_CAPACITY_PACKETS);
    let stats = Arc::new(AudioCaptureStats::default());
    let callback_stats = stats.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let callback_stop = stop.clone();
    let mut frame_cursor = 0_u64;

    type Args = render_callback::Args<data::Interleaved<f32>>;
    audio_unit
        .set_input_callback(move |args: Args| {
            if callback_stop.load(Ordering::Relaxed) {
                return Ok(());
            }

            let samples = process_interleaved_f32(args.data.buffer, args.data.channels, settings);
            let frame_count = samples.len() / usize::from(NATIVE_AUDIO_CHANNELS);
            let frame = AudioFrame {
                timestamp_micros: timestamp_for_frame(frame_cursor),
                captured_at: Instant::now(),
                sample_rate: NATIVE_AUDIO_SAMPLE_RATE,
                channels: NATIVE_AUDIO_CHANNELS,
                samples,
            };
            frame_cursor = frame_cursor.saturating_add(frame_count as u64);
            callback_stats.record_captured_frames(frame_count as u64);

            match sender.try_send(frame) {
                Ok(()) => {}
                Err(TrySendError::Full(frame)) => {
                    callback_stats.record_dropped_frames(frame.frame_count() as u64);
                }
                Err(TrySendError::Disconnected(_)) => {}
            }

            Ok(())
        })
        .with_context(|| {
            format!("Could not register CoreAudio input callback for {device_name}")
        })?;

    audio_unit
        .start()
        .with_context(|| format!("Could not start CoreAudio input device {device_name}"))?;

    Ok(NativeAudioSource {
        device_id,
        device_name,
        receiver: Some(receiver),
        stats,
        stop,
        stop_on_drop: true,
        audio_unit: Some(audio_unit),
    })
}

#[cfg(target_os = "linux")]
fn start_platform_audio_source(
    device_id: u32,
    settings: AudioProcessingSettings,
) -> Result<NativeAudioSource> {
    linux::start_platform_audio_source(device_id, settings)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn start_platform_audio_source(
    _device_id: u32,
    _settings: AudioProcessingSettings,
) -> Result<NativeAudioSource> {
    bail!("Native microphone capture is only implemented on macOS and Linux");
}

#[cfg(target_os = "macos")]
fn list_platform_microphones() -> Result<Vec<Device>> {
    use coreaudio::audio_unit::Scope;
    use coreaudio::audio_unit::macos_helpers::{
        get_audio_device_ids_for_scope, get_audio_device_supports_scope, get_default_device_id,
        get_device_name,
    };

    let default_input = get_default_device_id(true);
    let mut devices = Vec::new();

    for device_id in get_audio_device_ids_for_scope(Scope::Input)? {
        if !get_audio_device_supports_scope(device_id, Scope::Input).unwrap_or(false) {
            continue;
        }
        let name =
            get_device_name(device_id).unwrap_or_else(|_| format!("CoreAudio device {device_id}"));
        let is_default = default_input == Some(device_id);
        devices.push(Device {
            id: format!("microphone:coreaudio:{device_id}"),
            name,
            kind: DeviceKind::Microphone,
            status: DeviceStatus::Available,
            detail: Some(if is_default {
                "Native CoreAudio input · default".to_string()
            } else {
                "Native CoreAudio input".to_string()
            }),
            width: None,
            height: None,
        });
    }

    if devices.is_empty() {
        bail!("CoreAudio did not report any input devices");
    }

    devices.sort_by_key(|device| {
        if device
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("default"))
        {
            (0, device.name.clone())
        } else {
            (1, device.name.clone())
        }
    });
    Ok(devices)
}

#[cfg(target_os = "linux")]
fn list_platform_microphones() -> Result<Vec<Device>> {
    linux::list_platform_microphones()
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn list_platform_microphones() -> Result<Vec<Device>> {
    bail!("Native microphone discovery is only implemented on macOS and Linux")
}

/// PipeWire microphone capture (Linux port phase 1). The `!Send` main loop,
/// stream, and proxies all live on one dedicated thread per capture; frames
/// cross into the shared pipeline through the same bounded ring the CoreAudio
/// callback feeds on macOS, so everything downstream (epoch trim, FIFO
/// writer, meters, captions) is platform-blind.
#[cfg(target_os = "linux")]
mod linux {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, mpsc};
    use std::thread;
    use std::time::Instant;

    use anyhow::{Context, Result, anyhow, bail};
    use pipewire as pw;
    use pw::{properties::properties, spa};

    use super::{
        AUDIO_RING_CAPACITY_PACKETS, AudioCaptureStats, AudioFrame, AudioProcessingSettings,
        NATIVE_AUDIO_CHANNELS, NATIVE_AUDIO_SAMPLE_RATE, NativeAudioSource,
        process_interleaved_f32, timestamp_for_frame,
    };
    use crate::protocol::{Device, DeviceKind, DeviceStatus};

    fn ensure_pipewire_init() {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(pw::init);
    }

    #[derive(Debug, Clone)]
    struct SourceNodeInfo {
        node_id: u32,
        label: String,
        node_name: String,
    }

    /// Keeps the capture thread stoppable from `Drop`: the quit sender wakes
    /// the thread's main loop (the pw channel is the only sanctioned way to
    /// poke a loop from another thread), then the join bounds teardown.
    pub(super) struct PipewireCaptureHandle {
        quit: Option<pw::channel::Sender<()>>,
        thread: Option<thread::JoinHandle<()>>,
    }

    impl PipewireCaptureHandle {
        pub(super) fn stop(&mut self) {
            if let Some(quit) = self.quit.take() {
                let _ = quit.send(());
            }
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    impl Drop for PipewireCaptureHandle {
        fn drop(&mut self) {
            self.stop();
        }
    }

    pub(super) fn list_platform_microphones() -> Result<Vec<Device>> {
        ensure_pipewire_init();
        let mainloop = pw::main_loop::MainLoopRc::new(None)
            .context("Could not create a PipeWire main loop")?;
        let context = pw::context::ContextRc::new(&mainloop, None)
            .context("Could not create a PipeWire context")?;
        let core = context.connect_rc(None).context(
            "Could not connect to PipeWire. Is the PipeWire daemon running for this session?",
        )?;
        let (sources, default_node_name) = collect_audio_sources(&mainloop, &core)?;
        if sources.is_empty() {
            bail!("PipeWire did not report any audio input devices");
        }
        Ok(microphone_devices(sources, default_node_name.as_deref()))
    }

    /// One registry roundtrip collecting every `Audio/Source` node plus the
    /// session manager's default-source node name from the "default"
    /// metadata object. Two syncs: the first flushes registry globals (which
    /// binds the metadata proxy), the second flushes the bound proxy's
    /// property events.
    fn collect_audio_sources(
        mainloop: &pw::main_loop::MainLoopRc,
        core: &pw::core::CoreRc,
    ) -> Result<(Vec<SourceNodeInfo>, Option<String>)> {
        let registry = core
            .get_registry_rc()
            .context("Could not get the PipeWire registry")?;

        let sources: Rc<RefCell<Vec<SourceNodeInfo>>> = Rc::default();
        let default_source: Rc<RefCell<Option<String>>> = Rc::default();
        let metadata_holds: Rc<
            RefCell<Vec<(pw::metadata::Metadata, pw::metadata::MetadataListener)>>,
        > = Rc::default();

        let sources_clone = sources.clone();
        let default_clone = default_source.clone();
        let holds_clone = metadata_holds.clone();
        let registry_clone = registry.clone();
        let _registry_listener = registry
            .add_listener_local()
            .global(move |global| {
                let Some(props) = global.props else { return };
                match global.type_ {
                    pw::types::ObjectType::Node => {
                        let media_class = props.get("media.class").unwrap_or("");
                        // Physical + virtual microphones. Sink monitors are a
                        // separate media class and a separate (desktop-audio)
                        // slice.
                        if media_class == "Audio/Source" || media_class == "Audio/Source/Virtual" {
                            sources_clone.borrow_mut().push(SourceNodeInfo {
                                node_id: global.id,
                                label: props
                                    .get("node.description")
                                    .or_else(|| props.get("node.nick"))
                                    .or_else(|| props.get("node.name"))
                                    .unwrap_or("PipeWire input")
                                    .to_string(),
                                node_name: props.get("node.name").unwrap_or("").to_string(),
                            });
                        }
                    }
                    pw::types::ObjectType::Metadata => {
                        if props.get("metadata.name") != Some("default") {
                            return;
                        }
                        let Ok(metadata) = registry_clone.bind::<pw::metadata::Metadata, _>(global)
                        else {
                            return;
                        };
                        let default_clone = default_clone.clone();
                        let listener = metadata
                            .add_listener_local()
                            .property(move |_subject, key, _type, value| {
                                if key == Some("default.audio.source")
                                    && let Some(name) = value.and_then(extract_default_node_name)
                                {
                                    *default_clone.borrow_mut() = Some(name);
                                }
                                0
                            })
                            .register();
                        holds_clone.borrow_mut().push((metadata, listener));
                    }
                    _ => {}
                }
            })
            .register();

        let pending = Rc::new(Cell::new(
            core.sync(0).context("Could not sync with PipeWire")?,
        ));
        let rounds = Rc::new(Cell::new(0_u8));
        let loop_clone = mainloop.clone();
        let core_clone = core.clone();
        let pending_clone = pending.clone();
        let _core_listener = core
            .add_listener_local()
            .done(move |id, seq| {
                if id != pw::core::PW_ID_CORE || seq != pending_clone.get() {
                    return;
                }
                if rounds.get() == 0 {
                    rounds.set(1);
                    match core_clone.sync(0) {
                        Ok(seq) => pending_clone.set(seq),
                        Err(_) => loop_clone.quit(),
                    }
                } else {
                    loop_clone.quit();
                }
            })
            .register();
        mainloop.run();

        let collected = sources.borrow().clone();
        let default_node_name = default_source.borrow().clone();
        Ok((collected, default_node_name))
    }

    /// The "default" metadata value is JSON shaped like {"name":"alsa_input…"}.
    fn extract_default_node_name(value: &str) -> Option<String> {
        serde_json::from_str::<serde_json::Value>(value)
            .ok()?
            .get("name")?
            .as_str()
            .map(str::to_string)
    }

    fn microphone_devices(
        sources: Vec<SourceNodeInfo>,
        default_node_name: Option<&str>,
    ) -> Vec<Device> {
        let mut devices: Vec<Device> = sources
            .into_iter()
            .map(|source| {
                let is_default = default_node_name == Some(source.node_name.as_str());
                Device {
                    id: format!("microphone:pipewire:{}", source.node_id),
                    name: source.label,
                    kind: DeviceKind::Microphone,
                    status: DeviceStatus::Available,
                    detail: Some(if is_default {
                        "PipeWire input · default".to_string()
                    } else {
                        "PipeWire input".to_string()
                    }),
                    width: None,
                    height: None,
                }
            })
            .collect();

        devices.sort_by_key(|device| {
            if device
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("default"))
            {
                (0, device.name.clone())
            } else {
                (1, device.name.clone())
            }
        });
        devices
    }

    pub(super) fn start_platform_audio_source(
        device_id: u32,
        settings: AudioProcessingSettings,
    ) -> Result<NativeAudioSource> {
        ensure_pipewire_init();
        let (sender, receiver) = mpsc::sync_channel(AUDIO_RING_CAPACITY_PACKETS);
        let stats = Arc::new(AudioCaptureStats::default());
        let stop = Arc::new(AtomicBool::new(false));
        let (quit_tx, quit_rx) = pw::channel::channel::<()>();
        let (setup_tx, setup_rx) = mpsc::channel::<Result<String>>();

        let thread_stats = stats.clone();
        let thread_stop = stop.clone();
        let thread = thread::Builder::new()
            .name(format!("pw-mic-{device_id}"))
            .spawn(move || {
                if let Err(error) = run_capture_loop(
                    device_id,
                    settings,
                    sender,
                    thread_stats,
                    thread_stop,
                    quit_rx,
                    &setup_tx,
                ) {
                    let _ = setup_tx.send(Err(error));
                }
            })
            .context("Could not spawn the PipeWire microphone capture thread")?;

        let device_name = match setup_rx.recv() {
            Ok(Ok(name)) => name,
            Ok(Err(error)) => {
                let _ = thread.join();
                return Err(error);
            }
            Err(_) => {
                let _ = thread.join();
                bail!("The PipeWire capture thread exited before the stream was ready");
            }
        };

        Ok(NativeAudioSource {
            device_id,
            device_name,
            receiver: Some(receiver),
            stats,
            stop,
            stop_on_drop: true,
            pipewire_capture: Some(PipewireCaptureHandle {
                quit: Some(quit_tx),
                thread: Some(thread),
            }),
        })
    }

    /// Everything `!Send` lives here, on the capture thread: verify the node
    /// still exists (and get its name — ids are recycled, names are how
    /// `target.object` links reliably), connect the stream, report readiness,
    /// then park in the main loop until the quit channel fires.
    fn run_capture_loop(
        node_id: u32,
        settings: AudioProcessingSettings,
        sender: mpsc::SyncSender<AudioFrame>,
        stats: Arc<AudioCaptureStats>,
        stop: Arc<AtomicBool>,
        quit_rx: pw::channel::Receiver<()>,
        setup_tx: &mpsc::Sender<Result<String>>,
    ) -> Result<()> {
        let mainloop = pw::main_loop::MainLoopRc::new(None)
            .context("Could not create a PipeWire main loop")?;
        let context = pw::context::ContextRc::new(&mainloop, None)
            .context("Could not create a PipeWire context")?;
        let core = context.connect_rc(None).context(
            "Could not connect to PipeWire. Is the PipeWire daemon running for this session?",
        )?;

        let (sources, _default) = collect_audio_sources(&mainloop, &core)?;
        let Some(node) = sources.into_iter().find(|source| source.node_id == node_id) else {
            bail!(
                "Microphone (PipeWire node {node_id}) was not found. It may have been \
                 unplugged — refresh the device list and pick another input."
            );
        };

        let props = properties! {
            *pw::keys::MEDIA_TYPE => "Audio",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Communication",
            *pw::keys::NODE_NAME => "videorc-microphone",
            *pw::keys::TARGET_OBJECT => node.node_name.clone(),
        };
        let stream = pw::stream::StreamBox::new(&core, "videorc-microphone", props)
            .context("Could not create a PipeWire capture stream")?;

        // Channel count follows the negotiated format; frame math must never
        // assume the offer was honored verbatim.
        let negotiated_channels = Rc::new(Cell::new(u32::from(NATIVE_AUDIO_CHANNELS)));
        let mut frame_cursor = 0_u64;

        let channels_format = negotiated_channels.clone();
        let channels_process = negotiated_channels;
        let process_stop = stop;
        let _stream_listener = stream
            .add_local_listener_with_user_data(())
            .param_changed(move |_, (), id, param| {
                let Some(param) = param else { return };
                if id != spa::param::ParamType::Format.as_raw() {
                    return;
                }
                let Ok((media_type, media_subtype)) = spa::param::format_utils::parse_format(param)
                else {
                    return;
                };
                if media_type != spa::param::format::MediaType::Audio
                    || media_subtype != spa::param::format::MediaSubtype::Raw
                {
                    return;
                }
                let mut info = spa::param::audio::AudioInfoRaw::default();
                if info.parse(param).is_ok() && info.channels() > 0 {
                    channels_format.set(info.channels());
                }
            })
            .state_changed(|_, (), _old, new| {
                if let pw::stream::StreamState::Error(error) = new {
                    tracing::warn!("PipeWire microphone stream error: {error}");
                }
            })
            .process(move |stream, ()| {
                if process_stop.load(Ordering::Relaxed) {
                    return;
                }
                let Some(mut buffer) = stream.dequeue_buffer() else {
                    return;
                };
                let datas = buffer.datas_mut();
                let Some(data) = datas.first_mut() else {
                    return;
                };
                let offset = data.chunk().offset() as usize;
                let size = data.chunk().size() as usize;
                let Some(bytes) = data.data() else { return };
                let end = offset.saturating_add(size).min(bytes.len());
                if end <= offset {
                    return;
                }

                let input: Vec<f32> = bytes[offset..end]
                    .chunks_exact(std::mem::size_of::<f32>())
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect();
                let channels = channels_process.get().max(1) as usize;
                let samples = process_interleaved_f32(&input, channels, settings);
                let frame_count = samples.len() / usize::from(NATIVE_AUDIO_CHANNELS);
                if frame_count == 0 {
                    return;
                }

                let frame = AudioFrame {
                    timestamp_micros: timestamp_for_frame(frame_cursor),
                    captured_at: Instant::now(),
                    sample_rate: NATIVE_AUDIO_SAMPLE_RATE,
                    channels: NATIVE_AUDIO_CHANNELS,
                    samples,
                };
                frame_cursor = frame_cursor.saturating_add(frame_count as u64);
                stats.record_captured_frames(frame_count as u64);

                match sender.try_send(frame) {
                    Ok(()) => {}
                    Err(mpsc::TrySendError::Full(frame)) => {
                        stats.record_dropped_frames(frame.frame_count() as u64);
                    }
                    Err(mpsc::TrySendError::Disconnected(_)) => {}
                }
            })
            .register()
            .context("Could not register the PipeWire stream listener")?;

        let format_pod = audio_format_pod()?;
        let mut params = [spa::pod::Pod::from_bytes(&format_pod)
            .ok_or_else(|| anyhow!("Could not build the PipeWire audio format pod"))?];
        stream
            .connect(
                spa::utils::Direction::Input,
                None,
                pw::stream::StreamFlags::AUTOCONNECT
                    | pw::stream::StreamFlags::MAP_BUFFERS
                    | pw::stream::StreamFlags::RT_PROCESS,
                &mut params,
            )
            .with_context(|| {
                format!(
                    "Could not connect a PipeWire capture stream to {}",
                    node.label
                )
            })?;

        let loop_clone = mainloop.clone();
        let _quit_guard = quit_rx.attach(mainloop.loop_(), move |_| loop_clone.quit());

        let _ = setup_tx.send(Ok(node.label));
        mainloop.run();
        Ok(())
    }

    /// Offer exactly one format — f32 interleaved at the pipeline's fixed
    /// rate/channels — so the graph converts to us and the FIFO contract
    /// downstream (`write_frame_f32le`) holds without app-side resampling.
    fn audio_format_pod() -> Result<Vec<u8>> {
        let mut audio_info = spa::param::audio::AudioInfoRaw::new();
        audio_info.set_format(spa::param::audio::AudioFormat::F32LE);
        audio_info.set_rate(NATIVE_AUDIO_SAMPLE_RATE);
        audio_info.set_channels(u32::from(NATIVE_AUDIO_CHANNELS));
        let object = spa::pod::Object {
            type_: spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
            id: spa::param::ParamType::EnumFormat.as_raw(),
            properties: audio_info.into(),
        };
        let (cursor, _) = spa::pod::serialize::PodSerializer::serialize(
            std::io::Cursor::new(Vec::new()),
            &spa::pod::Value::Object(object),
        )
        .map_err(|error| anyhow!("Could not serialize the PipeWire format request: {error:?}"))?;
        Ok(cursor.into_inner())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn node(node_id: u32, label: &str, node_name: &str) -> SourceNodeInfo {
            SourceNodeInfo {
                node_id,
                label: label.to_string(),
                node_name: node_name.to_string(),
            }
        }

        #[test]
        fn extracts_default_node_name_from_metadata_json() {
            assert_eq!(
                extract_default_node_name(r#"{"name":"alsa_input.usb-mic"}"#),
                Some("alsa_input.usb-mic".to_string())
            );
            assert_eq!(extract_default_node_name(r#"{"other":"x"}"#), None);
            assert_eq!(extract_default_node_name("not json"), None);
        }

        #[test]
        fn microphone_devices_sort_default_first_and_carry_pipewire_ids() {
            let devices = microphone_devices(
                vec![
                    node(70, "Cam Link 4K", "alsa_input.camlink"),
                    node(66, "Scarlett Solo Mic", "alsa_input.scarlett"),
                ],
                Some("alsa_input.scarlett"),
            );

            assert_eq!(devices[0].id, "microphone:pipewire:66");
            assert_eq!(
                devices[0].detail.as_deref(),
                Some("PipeWire input · default")
            );
            assert_eq!(devices[1].id, "microphone:pipewire:70");
            assert_eq!(devices[1].detail.as_deref(), Some("PipeWire input"));
        }

        #[test]
        fn unknown_default_marks_nothing() {
            let devices = microphone_devices(
                vec![node(66, "Scarlett Solo Mic", "alsa_input.scarlett")],
                None,
            );
            assert_eq!(devices[0].detail.as_deref(), Some("PipeWire input"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(samples: usize) -> AudioFrame {
        frame_at(Instant::now(), samples)
    }

    fn frame_at(captured_at: Instant, samples: usize) -> AudioFrame {
        AudioFrame {
            timestamp_micros: 0,
            captured_at,
            sample_rate: NATIVE_AUDIO_SAMPLE_RATE,
            channels: NATIVE_AUDIO_CHANNELS,
            samples: vec![0.0; samples * NATIVE_AUDIO_CHANNELS as usize],
        }
    }

    #[test]
    fn epoch_trim_discards_audio_captured_before_the_first_video_frame() {
        let (tx, rx) = mpsc::channel::<AudioFrame>();
        let epoch: OnceLock<Instant> = OnceLock::new();
        let stop = AtomicBool::new(false);

        // Pre-epoch capture: queued before the video pipeline delivered anything.
        let anchor = Instant::now();
        tx.send(frame_at(anchor + Duration::from_millis(10), 480))
            .unwrap();
        tx.send(frame_at(anchor + Duration::from_millis(20), 480))
            .unwrap();
        epoch.set(anchor + Duration::from_millis(25)).unwrap();

        let preroll = discard_audio_until_video_epoch(&rx, &epoch, &stop)
            .expect("epoch was set, the wait must succeed");
        assert_eq!(
            preroll.discarded_frames, 960,
            "both pre-epoch frames are trimmed"
        );
        assert!(preroll.ready_frames.is_empty());

        // Post-epoch frames flow to the FIFO untouched.
        tx.send(frame(480)).unwrap();
        assert_eq!(rx.try_recv().unwrap().frame_count(), 480);
    }

    #[test]
    fn epoch_trim_preserves_audio_already_captured_after_the_first_video_frame() {
        let (tx, rx) = mpsc::channel::<AudioFrame>();
        let epoch: OnceLock<Instant> = OnceLock::new();
        let stop = AtomicBool::new(false);
        let anchor = Instant::now();

        tx.send(frame_at(anchor + Duration::from_millis(10), 480))
            .unwrap();
        tx.send(frame_at(anchor + Duration::from_millis(30), 480))
            .unwrap();
        tx.send(frame_at(anchor + Duration::from_millis(40), 480))
            .unwrap();
        epoch.set(anchor + Duration::from_millis(25)).unwrap();

        let preroll = discard_audio_until_video_epoch(&rx, &epoch, &stop)
            .expect("epoch was set, the wait must succeed");

        assert_eq!(
            preroll.discarded_frames, 720,
            "one full pre-roll packet and half of the boundary packet are trimmed"
        );
        let ready_counts = preroll
            .ready_frames
            .iter()
            .map(AudioFrame::frame_count)
            .collect::<Vec<_>>();
        assert_eq!(ready_counts, vec![240]);
        assert_eq!(preroll.ready_frames[0].timestamp_micros, 5_000);
        assert_eq!(rx.try_recv().unwrap().frame_count(), 480);
    }

    #[test]
    fn epoch_trim_gives_up_when_stopped_before_video_arrives() {
        let (tx, rx) = mpsc::channel::<AudioFrame>();
        let epoch: OnceLock<Instant> = OnceLock::new();
        let stop = AtomicBool::new(true);
        tx.send(frame(480)).unwrap();
        assert!(discard_audio_until_video_epoch(&rx, &epoch, &stop).is_none());
    }

    #[test]
    fn audio_capture_coverage_flags_real_time_vs_stalled_capture() {
        // Below warmup: not enough signal yet.
        assert_eq!(audio_capture_coverage(48_000, 1.0, 48_000), None);
        // Full real-time capture over 4s ≈ 1.0 coverage.
        let full = audio_capture_coverage(4 * 48_000, 4.0, 48_000).unwrap();
        assert!((full - 1.0).abs() < 1e-6, "full coverage {full}");
        // Mic delivered only half the expected samples → a clear gap.
        let stalled = audio_capture_coverage(2 * 48_000, 4.0, 48_000).unwrap();
        assert!((stalled - 0.5).abs() < 1e-6, "stalled coverage {stalled}");
        // Zero sample rate is undefined.
        assert_eq!(audio_capture_coverage(1000, 5.0, 0), None);
    }

    #[test]
    fn meter_reports_no_frames_separately_from_unavailable() {
        let (_sender, receiver) = mpsc::channel();
        let source = NativeAudioSource {
            device_id: 42,
            device_name: "Built-in Microphone".to_string(),
            receiver: Some(receiver),
            stats: Arc::new(AudioCaptureStats::default()),
            stop: Arc::new(AtomicBool::new(false)),
            stop_on_drop: true,
            #[cfg(target_os = "macos")]
            audio_unit: None,
            #[cfg(target_os = "linux")]
            pipewire_capture: None,
        };

        let result = sample_meter_from_source(source, Duration::from_millis(1));

        assert_eq!(result.status, AudioMeterStatus::NoFrames);
        assert!(
            result
                .message
                .as_deref()
                .unwrap_or_default()
                .contains("did not send audio frames")
        );
    }

    #[test]
    fn fake_pcm_frames_keep_monotonic_timestamps() {
        let frames = fake_pcm_frames(4_800, 480, 440.0);
        assert!(frames.len() > 1);
        assert_eq!(frames[0].timestamp_micros, 0);
        assert_eq!(frames[0].sample_rate, NATIVE_AUDIO_SAMPLE_RATE);
        assert_eq!(frames[0].channels, NATIVE_AUDIO_CHANNELS);

        for pair in frames.windows(2) {
            assert!(pair[1].timestamp_micros > pair[0].timestamp_micros);
        }
    }

    // Plan 021 F3: session_peak is the max over the recording window (the
    // live meter only keeps the LAST frame's peak, which reads 0 the moment
    // the speaker pauses), and it resets with the window so a silent second
    // take is not masked by a loud first one.
    #[test]
    fn session_peak_holds_the_window_maximum_and_resets() {
        let stats = AudioCaptureStats::default();
        assert_eq!(stats.session_peak(), 0.0);

        stats.record_live_peak(0.25);
        stats.record_live_peak(0.9);
        stats.record_live_peak(0.1);
        // Live meter follows the last frame; the session keeps the loudest.
        assert!((stats.live_peak() - 0.1).abs() < 1e-3);
        assert!((stats.session_peak() - 0.9).abs() < 1e-3);

        stats.reset_recording_window();
        assert_eq!(stats.session_peak(), 0.0);
    }

    #[test]
    fn queued_audio_frames_are_discarded_before_fifo_writer_starts() {
        let (sender, receiver) = mpsc::sync_channel(AUDIO_RING_CAPACITY_PACKETS);
        for frame in fake_pcm_frames(1_920, 480, 440.0) {
            sender.try_send(frame).unwrap();
        }

        assert_eq!(discard_preroll_audio_frames(&receiver), 1_920);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn recording_audio_stats_start_after_preroll() {
        let stats = AudioCaptureStats::default();
        stats.captured_frames.fetch_add(48_000, Ordering::Relaxed);
        stats.dropped_frames.fetch_add(4_800, Ordering::Relaxed);
        stats.fifo_write_errors.fetch_add(1, Ordering::Relaxed);

        stats.reset_recording_window();

        assert_eq!(stats.captured_frames(), 0);
        assert_eq!(stats.dropped_frames(), 0);
        assert_eq!(stats.fifo_write_errors.load(Ordering::Relaxed), 0);

        stats.record_captured_frames(480);
        stats.record_dropped_frames(96);

        assert_eq!(stats.captured_frames(), 480);
        assert_eq!(stats.dropped_frames(), 96);

        stats.finish_recording_window();
        stats.record_captured_frames(480);
        stats.record_dropped_frames(96);

        assert_eq!(stats.captured_frames(), 480);
        assert_eq!(stats.dropped_frames(), 96);

        stats.reset_recording_window();
        stats.record_captured_frames(240);

        assert_eq!(stats.captured_frames(), 240);
        assert_eq!(stats.dropped_frames(), 0);
    }

    #[test]
    fn gain_and_mute_are_deterministic() {
        let input = [0.25, 0.25, 0.5, 0.5];
        let gained = process_interleaved_f32(
            &input,
            2,
            AudioProcessingSettings {
                gain_db: 6.0,
                muted: false,
            },
        );
        assert!(gained[0] > input[0]);
        assert!(gained[1] > input[1]);

        let muted = process_interleaved_f32(
            &input,
            2,
            AudioProcessingSettings {
                gain_db: 24.0,
                muted: true,
            },
        );
        assert!(muted.iter().all(|sample| *sample == 0.0));
    }

    #[test]
    fn mono_input_is_duplicated_to_stereo() {
        let output = process_interleaved_f32(&[0.1, 0.2], 1, AudioProcessingSettings::default());
        assert_eq!(output, vec![0.1, 0.1, 0.2, 0.2]);
    }

    #[test]
    fn left_only_stereo_input_is_centered() {
        let output = process_interleaved_f32(
            &[0.5, 0.0, -0.25, 0.0],
            2,
            AudioProcessingSettings::default(),
        );
        assert_eq!(output, vec![0.5, 0.5, -0.25, -0.25]);
    }

    #[test]
    fn right_only_stereo_input_is_centered() {
        let output = process_interleaved_f32(
            &[0.0, 0.5, 0.0, -0.25],
            2,
            AudioProcessingSettings::default(),
        );
        assert_eq!(output, vec![0.5, 0.5, -0.25, -0.25]);
    }

    #[test]
    fn true_stereo_input_is_averaged_then_centered() {
        let output = process_interleaved_f32(
            &[0.5, 0.25, -0.25, -0.75],
            2,
            AudioProcessingSettings::default(),
        );
        assert_eq!(output, vec![0.375, 0.375, -0.5, -0.5]);
    }

    #[test]
    fn native_voice_centering_uses_first_two_channels() {
        let output =
            process_interleaved_f32(&[0.6, 0.2, 1.0], 3, AudioProcessingSettings::default());
        assert_eq!(output, vec![0.4, 0.4]);
    }

    #[test]
    fn parses_coreaudio_microphone_ids() {
        assert_eq!(
            parse_coreaudio_microphone_id("microphone:coreaudio:42"),
            Some(42)
        );
        assert_eq!(
            parse_coreaudio_microphone_id("microphone:avfoundation:1"),
            None
        );
    }
}
