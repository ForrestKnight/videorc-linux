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
| Build/lint/tests | — | **Green** (fmt, clippy `-D warnings`, 695 tests) since `linux/phase0-compile` |
| Backend launch + WS protocol | axum/WS | Phase 0 — in verification |
| Microphone | CoreAudio | Stubbed (`bail!`), planned on PipeWire (phase 1) |
| Camera | AVFoundation | Stubbed (`bail!`), planned on V4L2 (phase 2) |
| Screen/window capture | ScreenCaptureKit | Stubbed (`bail!`), planned on portal ScreenCast + PipeWire (phase 3) |
| Composition | Metal GPU (`metal_compositor.rs`) | CPU compositor path is portable and compiles; GPU path correctly gated off. wgpu port is a later phase |
| Preview | Detached native CAMetalLayer window | Will use the existing MJPEG fallback route with explicit degraded status (phase 1 of preview); native wgpu preview later |
| Encoding | VideoToolbox H.264 (+ `RawYuv420p` raw mode) | `RawYuv420p` is the default off macOS (existing seam). **Known gap:** the raw-output FFmpeg args still name `h264_videotoolbox`; Linux needs libx264 (see below) |
| Streaming | FFmpeg RTMP (tee / fifo-muxer legs) | Arg-building compiles and is platform-forked in tests; blocked on the encoder gap above |
| Storage/protocol/state | Portable | Compiles; Linux default DB path `~/.videorc/videorc.sqlite3` (existing seam), recordings default `~/Movies/Videorc/Recordings` (macOS convention leaking — candidate for an XDG `~/Videos` island) |
| FFmpeg provisioning | `build-ffmpeg-macos.sh` / `fetch-ffmpeg-windows.mjs` | Not started (phase 5); resolution is env-var + PATH (`VIDEORC_BUNDLED_FFMPEG_PATH`), so system FFmpeg works for development |

## Deliberate degradations (and where the status is surfaced)

None active yet — capture subsystems bail with explicit messages rather than
degrade. As Linux paths land, every degraded route (MJPEG preview, software
encode) gets an entry here naming where its status appears in
diagnostics/health copy, per the native-preview rules.

## Known gaps

- **`h264_videotoolbox` off Apple:** `bridge_compositor_ffmpeg_args` hard-codes
  the VideoToolbox FFmpeg encoder on the `RawYuv420p` path. Any encoder-bridge
  recording/stream on Linux fails at FFmpeg spawn. Owned by the Linux encoding
  slice; FIXME sits in `bridge_stream_only_multistream_tees_flv_targets`.
- **Recordings default directory** uses `~/Movies` on every non-Windows
  platform (macOS convention); Linux convention is `~/Videos` (XDG).

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
