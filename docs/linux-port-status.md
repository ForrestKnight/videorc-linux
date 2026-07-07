# Linux port status

Living status of the Linux/Wayland port so the state of every subsystem is
assessable at a glance. Updated with each port slice. Specs live in
`docs/specs/linux-*.md`.

Target: any modern Wayland distro via xdg-desktop-portal + PipeWire — nothing
compositor-specific. Dev machine is Arch/Hyprland; the portability proof is a
clean GNOME/KDE VM at packaging time.

## Subsystems

| Subsystem | macOS implementation | Linux status |
|---|---|---|
| Build/lint/tests | — | **Green** (fmt, clippy `-D warnings`, 695 tests; `pnpm typecheck`/`lint`/`build`, desktop + script tests) since `linux/phase0-compile` |
| Backend launch + WS protocol | axum/WS | **Green** — Electron dev app launches on Wayland, backend spawns via cargo, WS READY handshake and session protocol verified by `smoke:dev` |
| Synthetic recording | Compositor → FIFO → FFmpeg | **Green** — `pnpm smoke:dev` passes all five layout scenarios on Linux (60 frames, A/V skew 16ms, artifact-analyzed) |
| Microphone | CoreAudio | **Green (PipeWire)** — enumeration with default marker, meter, live capture through the shared FIFO/epoch pipeline; gated by `pnpm smoke:linux-mic` (virtual-mic, no hardware needed). Desktop audio (monitor sources) is a follow-up slice |
| Camera | AVFoundation | **Green (V4L2)** — enumeration with capability matrix, live BGRA capture (NV12/YU12/YUYV/UYVY/MJPG) into the shared preview/compositor store, recording verified on a Cam Link 4K; gated by `pnpm smoke:linux-camera` (skips explicitly without a camera) |
| Screen/window capture | ScreenCaptureKit | **Green (portal ScreenCast + PipeWire)** — one portal source entry (compositor picker chooses the monitor/window), live BGRA capture into the shared preview/compositor store, restore-token persistence (per source id, cleared on cancelled re-grant), recording verified end to end on Hyprland (3840×2560, 90-frame artifact). Gated by `pnpm smoke:linux-screen` (SKIPs explicitly without a grant, like upstream's macOS device smokes) |
| Composition | Metal GPU (`metal_compositor.rs`) | CPU compositor path is portable, compiles, and composits the synthetic scenes in `smoke:dev`; GPU path correctly gated off. wgpu port is a later phase |
| Preview | Detached native CAMetalLayer window | Falls back to image polling with an explicit reason ("no Metal IOSurface target"), surfaced in backend status; native wgpu preview later |
| Encoding | VideoToolbox H.264 (+ `RawYuv420p` raw mode) | **Working (software)** — raw legs encode with libx264 `veryfast`+`zerolatency` via the `platform_h264_encoder_args` seam; diagnostics report `SoftwareX264` truthfully. VAAPI/NVENC later as flag swaps in the same seam |
| Streaming | FFmpeg RTMP (tee / fifo-muxer legs) | **Green** — `smoke:multistream` fans one encode to multiple local RTMP listeners with per-leg artifact gates; offline legs isolated; verified on Linux unmodified |
| Composition (multi-source) | Metal GPU | **Green (CPU)** — `smoke:dev` composites all five layout presets (ffprobe-checked); `smoke:linux-studio` blended screen+camera+mic into one recording (correct 30fps cadence, 15ms A/V skew, non-silent audio). Note: native-resolution (4K/5K) screen composite is slow to warm up in a debug build — a phase-6 perf item (release build + dmabuf zero-copy), not a correctness gap |
| Storage/protocol/state | Portable | Compiles; Linux default DB path `~/.videorc/videorc.sqlite3` (existing seam), recordings default `~/Movies/Videorc/Recordings` (macOS convention leaking — candidate for an XDG `~/Videos` island) |
| FFmpeg provisioning | `build-ffmpeg-macos.sh` / `fetch-ffmpeg-windows.mjs` | Not started (phase 5); resolution is env-var + PATH (`VIDEORC_BUNDLED_FFMPEG_PATH`), so system FFmpeg works for development |

## Deliberate degradations (and where the status is surfaced)

- **Software H.264 encode (libx264)** on the raw bridge legs — surfaced in
  diagnostics as `encode_backend: SoftwareX264` (`platform_h264_encode_backend`).
- **Preview falls back to image polling** — the backend logs and status carry
  the explicit reason ("the compositor status carries no Metal IOSurface
  target"), matching the native-preview rules.
- **Screen capture requires a one-time interactive grant** — the compositor
  picker is the Wayland permission model; the first grant is a human click,
  after which the persisted restore token makes it headless. The screen smoke
  SKIPs explicitly (never hangs) without a grant. This is the direct analog of
  the macOS screen-recording permission the upstream device smokes require.

## Known gaps

- **Recordings default directory** uses `~/Movies` on every non-Windows
  platform (macOS convention); Linux convention is `~/Videos` (XDG).
- **System FFmpeg dependency:** development uses the system `ffmpeg` from
  PATH (verified against 8.1.1); bundled-static provisioning is phase 5.

## macOS-only verification that cannot run here

Device, native-preview, and real-capture smokes need macOS
permissions/hardware: `smoke:recording-studio:devices`,
`smoke:screen-recording-real`, `probe:preview-lifecycle` (native surface
paths), and the packaging preflights. These are listed as **not run** in Linux
handoffs; the runnable subset is noted per phase.

## Slice log

- `linux/phase0-compile`: SourceMask seam extraction + non-macOS cfg hygiene.
  Spec: `docs/specs/linux-0001-phase0-compile.md`. Gates green on Linux
  (fmt/clippy/tests); macOS diff inert by construction.
- `linux/phase0-compile` (cont.): libx264 raw-leg encoder seam
  (`platform_h264_encoder_args`) with truthful `SoftwareX264` diagnostics, and
  the bridge test tone paced at realtime (`-re`) — an unpaced tone left a
  130-200ms `-shortest` audio tail that failed the A/V-skew gate (likely the
  same mechanism behind the 100-133ms skew warnings on macOS). Phase-0 gate
  met: `pnpm smoke:dev` fully green on Linux/Wayland.
