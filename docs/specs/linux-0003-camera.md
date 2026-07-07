# Linux port 0003 — Phase 2: camera capture on V4L2

## Goal

Cameras enumerate with real names and format matrices, preview frames flow
into the existing BGRA pipeline (MJPEG preview + compositor + recording), and
a camera recording artifact passes the analyzer gates. Dev-box hardware: an
Elgato Cam Link 4K delivering NV12/YU12 at 3840x2160@30 (verified with a raw
FFmpeg grab before any code).

## macOS behavior being matched

The camera stack has one seam per file, both already stubbed off-macOS:

- `camera_capture.rs` — enumeration (`list_native_cameras`), the per-device
  format capability matrix (`camera_capability_matrix_for_id`), and the id
  scheme: `NATIVE_CAMERA_PREFIX` + hex(unique id), round-tripped by
  `native_camera_device_id` / `parse_native_camera_id`. The format-selection
  brains (`choose_camera_format`, `normalize_camera_formats`) are already
  shared, pure, and unit-tested — Linux only feeds them `CameraFormatSummary`
  rows (width/height/min-max fps).
- `preview_camera.rs` — `run_native_camera_preview(config, shared, stop_rx,
  startup_tx)`: a capture thread that publishes **BGRA8** frames into
  `PreviewCameraShared` (a frame store with fps/drop accounting) and reports
  startup as `Live { requested/selected format, dims, fps } |
  PermissionNeeded | DeviceMissing | Failed`. Everything downstream — MJPEG
  preview, compositor fetch, sources-ready gating for recording — reads that
  shared store and is platform-blind.
- The AVFoundation module converts device pixel formats to BGRA itself
  (`nv12_to_bgra`, `yuv422_to_bgra` — pure, rayon-parallel, BT.709 via
  `color.rs`); the downstream pipeline never sees anything but BGRA.

## Linux design

Crate: `v4l` (Linux-only dep) — thin, ioctl-level V4L2 (enum devices/formats/
frame sizes/intervals, mmap streaming). Rejected `nokhwa`: a cross-platform
abstraction on top of the same ioctls that would hide exactly the
format-matrix detail the diagnostics surface, and duplicate what the shared
choice helpers already do.

1. **Ids.** `NATIVE_CAMERA_PREFIX` becomes platform-valued
   (`camera:v4l2-native:` on Linux); the unique id inside the hex encoding is
   the device node path (`/dev/videoN`). Node numbering is not stable across
   replug — same story as CoreAudio/PipeWire ids, and the source registry's
   name-rematch already owns that. No consumer changes: everything goes
   through the two id functions.
2. **Enumeration** (`camera_capture.rs mod linux`): scan V4L2 nodes, keep
   those with `VIDEO_CAPTURE` + `STREAMING` capabilities that report at least
   one *convertible* pixel format (filters out UVC metadata nodes like the
   Cam Link's second device). Name from the card string. Capability matrix
   from `VIDIOC_ENUM_FMT` x frame sizes x intervals, normalized by the shared
   helper. Discrete/stepwise/continuous size support mapped conservatively
   (discrete as-is; stepwise/continuous contribute min/max corners).
3. **Capture** (`preview_camera.rs mod linux`): open by path, pick
   width/height/fps with the shared `choose_camera_format`, then pick the
   fourcc for that size by preference `NV12 > YUYV > UYVY > YU12 > MJPG` —
   raw formats first, JPEG decode (via the existing `image` dep) only when a
   webcam offers nothing else at the chosen size. mmap stream with a read
   timeout so the loop can honor `stop_rx` even when a source stops
   delivering (unplugged HDMI on a capture card is the norm, not the edge).
   Convert per-frame to BGRA and publish into `PreviewCameraShared` exactly
   like the macOS callback (sequence, fps window, drop accounting, timing
   samples).
4. **Conversions**: extract `nv12_to_bgra` / `yuv422_to_bgra` from `mod
   macos` to shared scope (byte-identical macOS behavior — pure functions,
   the module keeps calling them), add an `i420_to_bgra` sibling for YU12,
   and MJPG via `image`'s jpeg decoder. V4L2 `quantization` flag selects
   full- vs video-range for NV12, mirroring the CoreVideo format split.
5. **Permissions**: Linux has no camera permission dialog; an `EACCES` open
   maps to `PermissionNeeded` with "add your user to the `video` group"
   guidance. Missing node → `DeviceMissing`.

## Alternatives considered

- **nokhwa** — see above; also pulls its own conversion layer that would
  bypass the shared BT.709 code the CPU/Metal/FFmpeg paths agree on.
- **GStreamer** — an entire framework for what is four ioctls and a convert;
  the packaging weight alone disqualifies it for phase 5.
- **FFmpeg-process capture** (`-f v4l2` like the legacy avfoundation path) —
  frames would land in an FFmpeg process, not in `PreviewCameraShared`, so
  preview and the shared compositor would see nothing. The bridge is the
  production path; in-process capture is required.

## Test plan

- Unit: fourcc preference order, V4L2 interval→CameraFormatSummary mapping,
  full/video-range selection, i420 conversion goldens alongside the moved
  functions' existing coverage; id round-trip with the Linux prefix.
- Gates: fmt, clippy `-D warnings`, `cargo test -p videorc-backend`,
  `pnpm typecheck`, `smoke:dev`, `smoke:linux-mic` still green.
- Phase gate on the Cam Link: camera listed with its format matrix; preview
  frames live (frame store sequence advancing, MJPEG preview route serving);
  a camera-only recording artifact passes the analyzer (real motion, sane
  pacing). Promoted smoke where feasible; the Cam Link needs its HDMI source
  active, so the smoke skips explicitly (not silently) when no camera
  delivers frames.
