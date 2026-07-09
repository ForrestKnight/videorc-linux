# Linux port 0004 — Phase 3: screen capture via portal ScreenCast + PipeWire

## Goal

Screen recording end-to-end on any Wayland compositor: the ScreenCast portal
negotiates what to capture (its dialog is the platform's permission model and
its picker), PipeWire delivers the frames, and they land in the existing
`PreviewScreenShared` BGRA store — so preview, compositor, recording
readiness, and diagnostics work unchanged. Restore tokens make the dialog a
one-time event per selection, not a per-launch tax.

## macOS behavior being matched

`preview_screen.rs` has the same seam shape the camera port used:
`run_native_screen_preview(config, shared, stop_rx, startup_tx)` on a
dedicated thread, publishing BGRA frames and reporting
`Live { native/requested/actual dims, fps } | PermissionNeeded |
SourceMissing | Failed`. `screen_capture.rs` owns enumeration
(`list_native_capture_sources`) and the id scheme
(`screen:screencapturekit:{display_id}` / `window:…`).

## The structural difference, stated up front

ScreenCaptureKit enumerates displays and windows *first* and asks permission
once, globally. The portal model is inverted: **you cannot list capturable
sources without showing the user the system picker** — selection and
permission are the same dialog, per session, compositor-owned. Forcing the
macOS shape onto it (fake pre-enumeration via compositor IPC) is exactly
what the port charter bans. So on Linux:

- `list_native_capture_sources` returns **one** screen entry
  (`screen:portal:screencast`, "Screen Capture") whose
  detail says the compositor's own dialog chooses the actual
  monitor/window — the same portal model OBS uses on Wayland. No fake
  per-monitor rows the backend cannot honor.
- The portal's `restore_token` is persisted (sqlite settings, one key) and
  passed to `SelectSources`; with a valid token the compositor restores the
  previous selection without showing the dialog. Token invalidation (source
  gone, compositor restarted) falls back to the dialog — surfaced in the
  startup message, never silent.

## Linux design

Crates: `ashpd` (portal DBus, tokio feature) + the `pipewire` dep already in
the tree.

1. **Handshake** (on the capture thread, via a small current-thread tokio
   runtime — the thread seam stays sync like macOS):
   `Screencast::create_session` → `select_sources(types=MONITOR|WINDOW,
   cursor_mode=EMBEDDED per config.include_cursor, restore_token,
   persist_mode=ExplicitlyRevoked)` → `start()` (dialog here, unless the
   token restores) → streams: `[(pipewire_node_id, props)]` +
   `restore_token` (persist it) → `open_pipe_wire_remote()` → fd.
2. **Video stream**: `pipewire` context `connect_fd(fd)`, stream targeting
   the node id, format offer: video/raw in BGRx/BGRA/RGBx/RGBA at the
   negotiated size (no dmabuf modifiers — memory buffers only for phase 3;
   the wgpu phase can revisit zero-copy). `param_changed` reports the actual
   size for the `Live` message; `process` converts (BGRx/RGBx get alpha
   forced, RGBA channels swap) into the shared frame store with the same
   sequence/fps/drop accounting as the camera loop.
3. **Ids**: `screen:portal:screencast` parses via a small linux arm beside
   `parse_screencapturekit_display_id`; `display_id/window_id` stay `None`.
4. **Stop**: same two-layer teardown as audio — a `pipewire::channel` quit
   plus thread join; closing the session proxy ends the portal session.
5. **Testing**: the smoke automates the compositor picker (hyprland's
   share-picker) with `hyprctl` — *test tooling only*; the app path is pure
   portal and compositor-agnostic. The smoke asserts preview frames, a
   recorded artifact with A/V sync via the analyzer, and that a second
   session start with the persisted token skips the dialog.

## Alternatives considered

- **wlr-screencopy / Hyprland IPC enumeration** — banned by the charter;
  would not survive GNOME/KDE.
- **Faking per-monitor device rows** from `wl_output` — the backend could
  list names but not honor a specific pick (the portal decides), which is a
  silent lie to the UI. One honest entry + the system picker matches how
  OBS's Wayland capture behaves.
- **dmabuf import for zero-copy** — deferred to the wgpu phase; memory
  buffers keep phase 3 CPU-compositor-compatible everywhere.

## Test plan

- Unit: portal id parse, BGRx/RGBx/RGBA→BGRA conversions, restore-token
  persistence round-trip.
- Probe first: standalone ashpd+pipewire binary proving dialog → node id →
  frames (with picker automation), before any backend wiring.
- Gate: `smoke:linux-screen` — screen listed, preview live with frames, a
  screen-only recording passing the analyzer, restore token persisted and
  reused. `measure-av-sync.mjs` on a screen+tone artifact where feasible.
- macOS-only smokes that cover ScreenCaptureKit specifics stay listed as
  not-run in the status doc.
