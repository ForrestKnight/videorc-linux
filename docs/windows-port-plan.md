# Windows Port Plan

Goal: ship a Windows version of Videorc that hits the project's real bar — a
smooth preview and a correct recording (docs/, memory: OBS parity is dropped).
Dark-glass UI carries over; macOS-only niceties degrade gracefully.

## Where we stand (audited 2026-06-12)

The codebase is in better shape for this than expected. The platform seams
already exist:

- **Backend ↔ app transport is portable.** Axum WebSocket on `127.0.0.1` with
  token auth, `READY {host, port, token}` handshake over stdout, OAuth
  callback listener on loopback TCP. Nothing to change.
- **Every macOS framework is already `cfg`-gated** (`screen_capture.rs`,
  `camera_capture.rs`, `audio.rs`, `video_toolbox_encoder.rs`,
  `metal_compositor.rs`, …) with non-macOS stubs that return empty lists or
  bail. The backend should be *near*-compilable for Windows today.
- **The capture/encode hot path is ffmpeg subprocesses**, not frameworks.
  ScreenCaptureKit/AVFoundation are used for *discovery* (device lists,
  format matrices); recording assembles ffmpeg arg lists. VideoToolbox and
  the Metal compositor are opt-in sidecars with CPU/ffmpeg fallbacks.
- **The Electron app is ~95% portable.** Shortcuts already check
  `metaKey || ctrlKey`; mac-only code (dock icon, vibrancy, wallpaper fetch,
  System Settings deep links) is behind `process.platform` guards or
  degrades gracefully.

The real Windows work concentrates in five places:

1. **ffmpeg input plumbing** — recording/preview build `-f avfoundation`
   inputs and `screen:screencapturekit:` / `camera:avfoundation-native:`
   device-ID schemes (`recording.rs:4098`, `recording.rs:2826`). Windows
   needs `ddagrab`/`gdigrab` (screen) and `dshow` (camera/mic) equivalents
   plus new ID schemes.
2. **Device discovery** — Windows implementations behind the existing stubs:
   display/window enumeration, camera + format matrix, microphones.
3. **Unix-isms** — `libc::mkfifo` audio/overlay FIFOs (`audio.rs:237`,
   `recording.rs:4185`), Unix signals + `libc::kill` orphan watchdog
   (`main.rs`), macOS paths in `storage.rs`, Keychain in `secrets.rs`.
4. **ffmpeg + packaging** — a Windows ffmpeg (LGPL: **no libx264**; the
   encoder analog of VideoToolbox is MediaFoundation `h264_mf`, plus
   NVENC/QSV/AMF), `win:` section in electron-builder, `.exe` handling.
5. **Window chrome** — `vibrancy`, `titleBarStyle: 'hiddenInset'`, traffic
   lights, and the osascript wallpaper fetch are mac-only; Windows needs its
   own glass expression and window controls.

## Phase 0 — Prerequisites and decisions

Decisions to make before any code; each unblocks a later phase.

- **Hardware.** A real x64 Windows 11 machine with a GPU is strongly
  recommended (capture + hardware-encode behavior can't be judged in a VM;
  preview smoothness is judged by eye per project memory). A Windows ARM VM
  on the Mac (Parallels/UTM) is fine for Phase 1 bring-up only.
- **CI reality.** GitHub Actions budget is exhausted (memory) — plan around
  local gates on the Windows box, mirroring `smoke:local-gates`. Optional
  later: self-hosted runner on that box.
- **ffmpeg sourcing.** Start with a pinned prebuilt **LGPL win64** build
  (e.g. BtbN `win64-lgpl` release) checked into `vendor/ffmpeg/` layout;
  write `scripts/fetch-ffmpeg-windows.ps1` later if we want reproducible
  in-house builds (mingw-w64 cross-compile is possible but is its own
  project). Must-have components: `ddagrab`/`gdigrab`, `dshow`,
  `h264_mf`/`hevc_mf`, `h264_nvenc`, `h264_qsv`, `h264_amf`, mpegts/mp4
  muxers, flv/rtmp for streaming.
- **Minimum Windows version: Windows 10 1903+** (Windows.Graphics.Capture),
  realistically Windows 11 for Mica/acrylic. Decide explicitly.
- **Crate choice:** `windows` (windows-rs) for all Win32/WinRT/COM work —
  DXGI, MediaFoundation, WASAPI, Job Objects.

## Phase 1 — It builds, launches, and connects (no capture)

Outcome: app starts on Windows, glass-ish UI renders, backend spawns and
connects over loopback, device lists are empty but nothing crashes.

Backend:
- Add Windows clauses to `storage.rs` paths (`%APPDATA%\Videorc`,
  `%USERPROFILE%\Videos\Videorc\Recordings`) and `secrets.rs`
  (Windows Credential Manager; `keyring`-style abstraction).
- Replace Unix signal handling/orphan watchdog in `main.rs` with
  `tokio::signal::ctrl_c` + a Job Object ("kill on job close") so the
  backend and its ffmpeg children die with the app — this is *better* than
  the PID-ledger semantics and worth doing first, not as polish.
- Gate the FIFO helpers (`audio.rs`, `recording.rs`) behind
  `cfg(unix)` so the crate compiles; Windows replacements come in Phase 2/3.
- Gate: `cargo check --target x86_64-pc-windows-msvc` (cross-check runs on
  the Mac — catches type errors without the Windows box).

Electron:
- `.exe` suffixes in backend/ffmpeg resolution (`main/index.ts:2313-2554`),
  spawn semantics, owned-process cleanup via the Job Object.
- OAuth on Windows: `open-url` does not fire; the `videorc://` URL arrives
  in `process.argv` of the second instance — handle it in the existing
  `second-instance` listener.
- Window chrome v1: `titleBarStyle: 'hidden'` + `titleBarOverlay` (native
  min/max/close, themed to the glass tokens); skip transparency initially —
  solid `--background` fallback is already the degraded glass path.
- electron-builder: `win:` section (`nsis` + `dir` targets), `icon.ico`,
  protocol registration; unsigned builds for now.
- Gate: `pnpm package` on Windows produces a launchable app;
  `smoke:dev`-class scripts (the portable ones) pass.

## Phase 2 — Recording MVP via ffmpeg (the tracer bullet)

Outcome: pick a screen + camera + mic on Windows, see previews, record a
correct file. This is the slice that proves the product on Windows.

- **Screen:** enumeration via DXGI outputs (`windows` crate) behind the
  `screen_capture.rs` stub; new ID scheme (e.g. `screen:dxgi:<adapter>:<output>`);
  recording/preview input via `-f ddagrab` (GPU Desktop Duplication,
  preferred) with `gdigrab` fallback. Window capture can lag displays
  (ddagrab is display-oriented; window enumeration via Win32 if/when needed).
- **Camera:** enumeration via MediaFoundation `MFEnumDeviceSources` +
  format matrix behind `camera_capture.rs` stub; ID scheme
  `camera:dshow:<name-or-path>`; capture via `-f dshow`.
- **Mic (MVP):** skip porting the CoreAudio→FIFO path; feed audio with a
  second `-f dshow` audio input directly in the ffmpeg command. The native
  path with gain/mute/epoch alignment is Phase 3.
- **Encoder:** default `h264_mf` (MediaFoundation = the VideoToolbox analog
  in LGPL ffmpeg); probe-and-prefer NVENC/QSV/AMF where present. Wire the
  same `VIDEORC_ENCODER_BRIDGE_VIDEO_OUTPUT` switchboard.
- Preview stays on the existing portable frame-polling surface (the
  IOSurface/CAMetalLayer zero-copy driver is mac-only and explicitly
  optional).
- Gate: `analyze-recording`/`check-real-source-evidence` pass on a Windows
  recording; A/V sync baseline within the same thresholds as macOS; preview
  judged by eye on a moving scene (freezedetect per memory).

## Phase 3 — Native parity where it earns its keep

Outcome: Windows quality matches macOS daily-driver quality.

- **WASAPI mic capture** ported into `audio.rs`'s ring-buffer design, with
  the FIFO replaced by a named pipe (`\\.\pipe\videorc-audio-…`) or stdin
  pipe; restores gain/mute, mono→stereo, video-epoch alignment.
- **Windows.Graphics.Capture** (or stay on ddagrab if Phase 2 quality is
  good — measure first) for window-level capture and the yellow-border-free
  experience.
- **Encoder bridge sidecar:** add a `WindowsMediaFoundationH264` variant to
  `EncoderBridgeVideoOutput` mirroring the VideoToolbox sidecar, if ffmpeg
  `h264_mf` proves limiting (measure first — it may not).
- **Overlay FIFO** (`recording.rs:4185`) → named pipe, only if the overlay
  feature is in the Windows scope.
- Explicit non-goals: Metal compositor port (CPU YUV path is the live path
  today), IOSurface zero-copy preview, native preview host windows (AppKit
  helper binary stays mac-only).

## Phase 4 — Windows-native glass and UX

Outcome: the design language reads as intentional on Windows, not as a mac
app in exile.

- **Glass:** two candidates, A/B by eye on the Windows box:
  (a) Electron `backgroundMaterial: 'acrylic'|'mica'` (Win 11) — real system
  blur, but constrains frame options; (b) port the `GlassWallpaperUnderlay` —
  wallpaper path on Windows is a registry read (`Control Panel\Desktop\WallPaper`),
  no permission prompt needed, and the renderer underlay already does the
  blur. (b) preserves the existing architecture and identical look; likely
  winner.
- **Kbd glyphs:** expose platform via preload; `⌘` → `Ctrl` in `kbd.tsx`
  and the footer/palette hints.
- **Permissions UX:** `ms-settings:privacy-webcam` / `privacy-microphone`
  deep links replacing `x-apple.systempreferences:`; note Windows has no
  screen-recording permission gate.
- Theme toggle, focus rings, reduced-motion: re-verify with
  `ui-theme-screens.mjs` ported (its screenshots go through CDP, not
  `screencapture` — should port nearly for free).

## Phase 5 — Packaging, signing, verification harness

- NSIS installer + portable dir; ffmpeg + backend.exe in `resources`;
  LGPL compliance is already satisfied by shipping ffmpeg as a separate
  spawned binary (same as macOS).
- Code signing: Azure Trusted Signing (cheapest route past SmartScreen) or
  an OV/EV Authenticode cert; unsigned is fine for internal testing only.
- Port the smoke harness tier by tier: the ~30 portable smokes first
  (`smoke:dev`, `smoke:oauth*`, `smoke:sources`, lifecycle, multistream),
  then baselines (`real-source-baseline-app.mjs` needs the Windows device
  IDs from Phase 2), leaving the ~12 `screencapture`-based `ui-*` probes
  mac-only or on CDP screenshots.
- Define `smoke:local-gates:windows` and run it on the Windows box as the
  merge gate (no Actions budget).

## Risks / open questions

- **Hardware encoder variance** (NVENC vs QSV vs AMF vs MF-software) is the
  biggest quality unknown; budget probe time on at least one discrete-GPU
  and one iGPU machine.
- **Electron transparency on Windows** is historically buggy
  (maximize/snap glitches) — hence chrome v1 ships solid, glass lands in
  Phase 4 behind the same env-var switches used for the mac glass bisect.
- **dshow camera format negotiation** is messier than AVFoundation's format
  matrix; expect device-specific quirks.
- **ddagrab + multi-GPU laptops** (Optimus): adapter selection needs an
  explicit flag and a fallback.
- **Dev loop friction:** one developer on macOS, app behavior on Windows —
  budget for remote-access tooling to the Windows box (SSH + screenshots or
  Parsec) so the judge-by-eye loop stays tight.

## Sequencing note

Phases 0–2 are the critical path and deliberately lean on ffmpeg for
everything; that's the shortest route to "a correct recording on Windows."
Phases 3–5 are quality/productization and can interleave with ongoing macOS
work. Suggested first slice for /cut-it: Phase 1's
`cargo check --target x86_64-pc-windows-msvc` green, since it forces every
Unix-ism into the open while still running entirely on the Mac.
