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
| Microphone | CoreAudio | Stubbed (`bail!`), planned on PipeWire (phase 1) |
| Camera | AVFoundation | Stubbed (`bail!`), planned on V4L2 (phase 2) |
| Screen/window capture | ScreenCaptureKit | Stubbed (`bail!`), planned on portal ScreenCast + PipeWire (phase 3) |
| Composition | Metal GPU (`metal_compositor.rs`) | CPU compositor path is portable, compiles, and composits the synthetic scenes in `smoke:dev`; GPU path correctly gated off. wgpu port is a later phase |
| Preview | Detached native CAMetalLayer window | Falls back to image polling with an explicit reason ("no Metal IOSurface target"), surfaced in backend status; native wgpu preview later |
| Encoding | VideoToolbox H.264 (+ `RawYuv420p` raw mode) | **Working (software)** — raw legs encode with libx264 `veryfast`+`zerolatency` via the `platform_h264_encoder_args` seam; diagnostics report `SoftwareX264` truthfully. VAAPI/NVENC later as flag swaps in the same seam |
| Streaming | FFmpeg RTMP (tee / fifo-muxer legs) | Arg-building compiles and is platform-forked in tests; end-to-end verification is phase 4 |
| Storage/protocol/state | Portable | Compiles; Linux default DB path `~/.videorc/videorc.sqlite3` (existing seam), recordings default `~/Movies/Videorc/Recordings` (macOS convention leaking — candidate for an XDG `~/Videos` island) |
| FFmpeg provisioning | `build-ffmpeg-macos.sh` / `fetch-ffmpeg-windows.mjs` | Not started (phase 5); resolution is env-var + PATH (`VIDEORC_BUNDLED_FFMPEG_PATH`), so system FFmpeg works for development |

## Deliberate degradations (and where the status is surfaced)

- **Software H.264 encode (libx264)** on the raw bridge legs — surfaced in
  diagnostics as `encode_backend: SoftwareX264` (`platform_h264_encode_backend`).
- **Preview falls back to image polling** — the backend logs and status carry
  the explicit reason ("the compositor status carries no Metal IOSurface
  target"), matching the native-preview rules.
- **Device lists are empty-with-reason** — `devices.rs`, camera and microphone
  discovery all return explicit "only implemented on macOS" warnings rather
  than empty silence, until phases 1–3 fill the seams.

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
