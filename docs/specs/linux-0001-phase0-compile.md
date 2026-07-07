# Linux port 0001 — Phase 0: compile, launch, synthetic record

## Goal

`videorc-backend` builds, lints, and tests green on Linux with every platform
capture path stubbed out (`bail!` with a clear message), the Electron app
launches with the backend connected over its axum/WS protocol, and the
synthetic compositor records a test pattern through FFmpeg. No capture
functionality is added in this phase.

## macOS behavior being matched

None — this phase adds no behavior. The bar is strictly: macOS compiles
byte-for-byte identically, and the existing `#[cfg(not(target_os = "macos"))]`
stubs (audio, camera, screen, native preview) become reachable instead of
being dead code behind a build that never linked.

## Baseline findings (Arch Linux, 2026-07-07)

- `cargo check` failed with 67 errors, all in `compositor.rs`, two root causes:
  1. `SourceMask` — the camera-bubble mask consumed by the **CPU** compositor
     on every render path — was defined inside `metal_compositor.rs`, which
     removes itself off macOS via an inner `#![cfg(target_os = "macos")]`.
  2. `try_gpu_compose` (the Metal compose path) had lost its platform gate: a
     caption-overlay helper was inserted between the function's doc comment
     and its `#[cfg(target_os = "macos")]` attribute, so the attribute now
     gated the helper and the Metal path compiled everywhere — clashing with
     the deliberate non-macOS stub of the same name.
- `cargo clippy -- -D warnings` then failed with ~130 findings: shared helpers
  whose only callers today are macOS-gated (dead code off macOS), imports and
  `mut` bindings used only under `cfg(macos)`, and one `let_and_return`.
- `cargo test` had 11 failures, all tests asserting macOS behavior (VideoToolbox
  split-output profiles, AVFoundation screen resolution, macOS default paths).
  Several tests already carried `#[cfg(not(target_os = "macos"))]` branches
  that had never been compiled and were wrong (e.g. asserting no `tee` on the
  raw-output multistream path).

## Design

1. **Seam extraction (shared-code commit).** Move `SourceMask` next to the CPU
   compositor's `SourceRenderOptions`; `metal_compositor.rs` re-exports it and
   keeps the shader-packing `impl` (`circle_flag`, `shader_radius`), so no
   macOS call site changes. Restore the `#[cfg(target_os = "macos")]` on
   `try_gpu_compose`.
2. **Cfg hygiene (additive commit).** File-level
   `#![cfg_attr(not(target_os = "macos"), allow(dead_code))]` on the ten
   macOS-heavy modules, so portable helpers (audio processing, camera format
   math, preview geometry, the MPEG-TS writer) stay compiled everywhere for
   later Linux phases to pick up. Platform-gate the handful of macOS-only
   imports and the VideoToolbox stat counters. Platform-fork the test
   expectations that encode platform behavior, matching the pattern the test
   suite already uses.
3. **Launch + synthetic record.** Verify the Electron app starts on Linux, the
   backend serves the WS protocol, device listings surface the stub reasons
   (explicit status, not silence), and the synthetic smoke records a test
   pattern via FFmpeg.

## Alternatives considered

- **Crate-wide `allow(dead_code)` off macOS** — one line, but it would mask
  genuinely dead code in the new Linux implementations; per-file keeps the
  blast radius to the modules that are macOS-heavy today.
- **Per-item `cfg_attr` annotations** — most precise, but ~30 annotations of
  pure noise that each Linux phase would then delete; the file-level attribute
  matches the existing precedent (`metal_compositor.rs` uses a file-level
  `#![allow(dead_code)]`).
- **Gating the shared helpers `#[cfg(target_os = "macos")]`** — rejected:
  phases 1–4 consume them on Linux (the MPEG-TS writer, YUV conversion, audio
  processing), and re-gating one item at a time invites churn in shared code.

## Known gaps carried forward (deliberate)

- The raw-output (`RawYuv420p`) encode path hard-codes `-c:v
  h264_videotoolbox`, which does not exist off Apple. Recording/streaming
  through the encoder bridge on Linux will fail at FFmpeg spawn until the
  Linux encoding slice swaps this (FIXME noted in the multistream test).
- All capture subsystems bail with their existing stub messages; the devices
  list is expected to be empty-with-reason on Linux until phases 1–3.

## Test plan

- `cargo fmt --check --all`, `cargo clippy -p videorc-backend -- -D warnings`,
  `cargo test -p videorc-backend` — green on Linux, and the diff is inert on
  macOS by construction (gates verified there by upstream CI on merge).
- `pnpm typecheck`, `pnpm lint`, desktop unit tests for the Electron side.
- Synthetic smoke (`pnpm smoke:dev` or nearest Linux-runnable equivalent)
  records a test pattern; artifact inspected with ffprobe per house rules.
- macOS-only smokes (device, native preview, recording-studio device gates)
  cannot run on this machine and are listed as not-run in
  `docs/linux-port-status.md` rather than skipped silently.
