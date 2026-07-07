# Linux port 0002 — Phase 1: microphone capture on PipeWire

## Goal

`list_platform_microphones` and `start_platform_audio_source` implemented on
PipeWire, so on Linux: microphones appear in the Sources tab with real names,
the audio check meter works, mic level meters are live during recording, and
a recorded artifact carries correct microphone audio per the repo's A/V
analyzers. Desktop/system audio (monitor sources) is scoped to a follow-up
slice; this one is microphone parity.

## macOS behavior being matched

The audio subsystem is already split so almost everything is portable. The
platform seam is small and precisely specified by the CoreAudio path:

- `list_platform_microphones() -> Result<Vec<Device>>` — one `Device` per
  input, `id: "microphone:coreaudio:{u32}"`, human-readable name, a `detail`
  line that marks the default input, default sorted first, error if no
  devices.
- `start_platform_audio_source(device_id: u32, settings) -> Result<NativeAudioSource>`
  — opens the device and delivers `AudioFrame`s (f32 interleaved, 48 kHz,
  2 ch — `NATIVE_AUDIO_SAMPLE_RATE`/`NATIVE_AUDIO_CHANNELS`) into a bounded
  `mpsc::sync_channel(1024)`. The capture callback runs
  `process_interleaved_f32` (voice-centering mono mixdown, gain, mute, stereo
  duplication), stamps `timestamp_micros` from a frame cursor and
  `captured_at: Instant`, counts captured/dropped frames in
  `AudioCaptureStats`, and never blocks (`try_send`; a full ring counts a
  drop).
- Everything downstream is shared and already tested: the video-epoch
  pre-roll trim, the f32le FIFO writer feeding FFmpeg, warmup detection via
  `captured_frames`, live/session peaks for the Studio meter, caption frame
  offers, `sample_native_audio_meter`.

Consumer-side seams that must open with it:

- `recording.rs` resolves `microphone:coreaudio:{id}` →
  `MicrophoneInput::CoreAudio { device_id, fifo_path }`. Per its own doc
  comment this variant is the platform-neutral "native capture path (CoreAudio
  today, WASAPI on Windows later) writing PCM to a FIFO"; the FFmpeg input it
  produces (`-f f32le -ar 48000 -ac 2 -i <fifo>`) is platform-independent.
- `devices.rs` gates microphone listing and `sample_audio_meter` behind
  `cfg!(target_os = "macos")` with explicit "only implemented for macOS in
  this spike" copy.

## Linux design

Stack: `pipewire` (pipewire-rs, the PipeWire project's own bindings) under
`[target.'cfg(target_os = "linux")'.dependencies]`. No cpal.

1. **Device IDs.** `microphone:pipewire:{node_id}` with a
   `parse_pipewire_microphone_id` twin of the CoreAudio parser, and a small
   platform helper at the two resolve sites (recording.rs, devices.rs) that
   parses the platform's native prefix into the same
   `MicrophoneInput::CoreAudio`-style native-FIFO input. PipeWire node ids
   are u32 (fits `device_id: u32`) and unstable across sessions — exactly
   like CoreAudio device ids, which is why the source registry already
   rematches persisted devices by name; no new persistence mechanism.
2. **Enumeration.** A short-lived connection on a dedicated thread: connect,
   registry roundtrip, collect nodes with `media.class == "Audio/Source"`
   (physical and virtual mics; monitors are `Audio/Sink` monitors and out of
   scope here), read `node.description`/`node.nick` for the display name,
   mark the default source from the `default` metadata object. Same
   default-first sort and `detail` copy shape as CoreAudio
   ("PipeWire input · default").
3. **Capture.** One dedicated OS thread per source owning a PipeWire
   `MainLoop` + `Stream` (`media.type=Audio, media.category=Capture`,
   `target.object={node_id}`), format negotiated to f32 interleaved
   48 kHz / 2 ch (SPA pod). The `process` callback mirrors the CoreAudio
   callback line for line: `process_interleaved_f32` → frame cursor →
   `try_send` → drop accounting. Stop: `NativeAudioSource`'s existing
   `Arc<AtomicBool>` plus a `pipewire::channel` sender to quit the loop from
   `Drop`; the platform handle field on `NativeAudioSource` becomes a
   cfg(linux) twin of the cfg(macos) `audio_unit` field (join handle + quit
   sender).
4. **Meter + devices.** Flip the two `devices.rs` gates from "macos only" to
   macos-or-linux, keeping the explicit bail copy for other platforms.

## Alternatives considered

- **cpal** — rejected for this slice: on Linux it enumerates through the ALSA
  compat layer (plugin aliases instead of the device names users see in
  their sound settings), cannot reach PipeWire monitor sources (needed by the
  desktop-audio follow-up), and duplicates a media stack that phase 3's
  portal screencast requires anyway. Recorded fallback: if pipewire-rs input
  capture proves unworkable, cpal serves microphone-only capture and desktop
  audio still goes to pipewire-rs later.
- **Resampling in-process** (accept the node's native rate, resample to 48k)
  — rejected: PipeWire negotiates rate/format per stream and converts in the
  graph, so requesting f32/48k/2ch directly matches what CoreAudio's
  `set_stream_format` does today.
- **String node names as device ids** (stable across reboots) — rejected:
  would fork the `device_id: u32` contract and the registry's name-rematch
  already owns persistence.

## De-risk probe

Before wiring anything: a standalone pipewire-rs probe (scratch, promoted to
`scripts/` only if it stays useful) that (a) lists `Audio/Source` nodes with
ids/names/default marker, (b) captures 2 s from one of them at f32/48k/2ch,
printing frame count, callback cadence, and peak. Pass = stable callbacks,
~96k frames captured, nonzero peak when speaking. This validates the bindings,
the format negotiation, and the threading model in isolation.

## Test plan

- Unit: `parse_pipewire_microphone_id` (twin of the CoreAudio parser tests);
  device-list mapping (node props → `Device`) as a pure function over
  collected node metadata; default-first sort.
- Gates: fmt, clippy `-D warnings`, `cargo test -p videorc-backend`,
  `pnpm typecheck` — plus `pnpm smoke:dev` staying green (no-mic sessions
  keep the paced test tone).
- Phase gate (with the Scarlett Solo on the dev box): mic listed by name in
  the Sources tab; audio check meter shows level; a recorded session's
  artifact passes `analyze-recording.mjs` with a real audio track and sane
  A/V skew; mic warmup/pre-roll diagnostics (`micCapturedFrames`, session
  peak) live during recording.
- macOS-only behavior untouched; the CoreAudio path compiles identically
  (verified by upstream CI on merge).
