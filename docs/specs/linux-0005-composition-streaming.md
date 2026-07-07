# Linux port 0005 — Phase 4: composition + streaming

## Goal

Confirm the two things the earlier phases set up but did not exercise
together: the CPU compositor blends multiple layout scenes into one frame on
Linux, and RTMP streaming reaches a real endpoint. Gate: a multi-source
layout recording and a stream, both verified by artifact analysis.

## What was already true entering the phase

Unlike phases 1–3, phase 4 fills almost no new seam — the pieces are
platform-independent and were proven incrementally:

- **CPU compositor** (`compositor.rs`, `live_layout.rs`, `live_scene.rs`) is
  the non-macOS render path and has no Metal dependency. `smoke:dev` already
  composites every layout preset (screen-camera, side-by-side, camera-only,
  screen-only, asset-background) on Linux and inspects the artifacts with
  ffprobe. The compositor blends frame *buffers*; it does not care whether a
  buffer came from a synthetic source, V4L2, or the portal.
- **Streaming** rides the same encoder-bridge FFmpeg path made
  platform-neutral in phase 0 (libx264 via `platform_h264_encoder_args`).
  `smoke:multistream` already fans one encode out to multiple local RTMP
  listeners (`ffmpeg -listen 1`) and gates every leg's artifact — it passes
  unmodified on Linux.

So the phase is verification, not construction: prove the compositor blends
*real* Linux capture sources together, and confirm streaming end to end.

## Design

No production code changes expected. The capstone is a smoke,
`smoke:linux-studio`, that drives a screen + camera + microphone session in
the screen-camera layout: the portal screen and the V4L2 camera composite
into one frame while the PipeWire mic feeds the audio track, and the finished
artifact must carry composited video (both sources visible) and real audio.

Streaming is covered by the existing `smoke:multistream` (already green on
Linux); the studio smoke focuses on the multi-source composite that no single
earlier smoke exercised.

## Interaction boundary (carried from phase 3)

The screen source needs the one-time portal grant. The studio smoke is
therefore interactive-or-skip, exactly like `smoke:linux-screen`: it SKIPs
explicitly when no restore token is available and no camera is present, and
runs full when a grant/token and a camera exist. This mirrors upstream's
macOS `smoke:recording-studio:devices`, which needs host permissions and
skips otherwise.

## Test plan

- `smoke:multistream` green on Linux (record + multi-target stream, artifact
  gates) — already verified.
- `smoke:dev` green on Linux (every layout preset composited, ffprobe-checked)
  — already verified.
- `smoke:linux-studio`: screen + camera + mic composite recording; artifact
  has composited video and a real (non-silent) audio track passing the
  analyzer. Interactive-or-skip.
- Standard gates (fmt, clippy, cargo test, typecheck) green.
