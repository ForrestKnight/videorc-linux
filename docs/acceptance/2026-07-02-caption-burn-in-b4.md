# Caption burn-in (A0–A4, B1–B3) — gate acceptance, 2026-07-02

Scope: the caption burn-in plan (Obsidian `2026-07-02 - Videorc Caption Burn-In Plan`,
grilled same day). All slices implemented and pushed; this note records what the
local gates PROVED versus what still needs a by-eye pass with a live session.

## Verified by gates (all green at commit `7715f8c5`)

- **Backend 693 tests / desktop 333 / web 198.**
- **Overlay slot (A1)**: PNG→RGBA decode with fail-closed limits; bad payloads
  keep the previous overlay; revisions increment; clear semantics.
- **Compositor blit (A2)**: synthetic-compositor tests prove the bar composites
  only on the leg carrying it (recording-leg frame byte-identical to baseline),
  correct straight-alpha math against a scene render, top placement + safe
  margin, wider-than-canvas center-crop.
- **Forced split (A4)**: same-profile record+stream yields no aux leg without
  burn-in and a same-resolution aux leg with it (unit test).
- **Rasterizer layout (A3)**: font scaling per width + S/M/L with a 24px floor,
  two-line greedy wrap keeping the tail, 92% width cap (6 vitest cases).
- **Chunk records (B1)**: epoch-anchor reset on frame-timestamp regression;
  web segments parse (camelCase); route returns word segments (web tests).
- **SRT (B2)**: segment-timed cues with absolute offsets, chunk-window
  fallback, overlap clamping, empty-chunk skipping (fixture tests).
- **ASS (B3)**: glass-adjacent style values (size/alignment/margin per knobs),
  brace/newline escaping, `(captioned)` path naming, ffmpeg filter-path
  escaping (unit tests).
- **Burn command shape**: validated against a synthesized clip **except** the
  ass filter itself — see the gap below.

## Environmental gap (explicit follow-up decision)

**No ffmpeg on this machine (or in the app bundle) has libass**: the bundled
build is deliberately dependency-free LGPL (`build-ffmpeg-macos.sh`, no
`--enable-libass`) and this machine's homebrew 8.1.1 is also built without it.
The burn job preflights `-filters` for `ass` and degrades to SRT-only with a
`captions-burn-unsupported` health warning — behavior verified by code review,
not runtime. **The captioned copy cannot materialize anywhere until one of:**
1. the bundled ffmpeg gains the libass chain (libass+freetype+harfbuzz+fribidi:
   build, signing, licenses), or
2. the burn pivots to a core-filters PNG-overlay track (renderer-rasterized
   bars → concat/qtrle overlay video → single `overlay` filter — works with the
   bundled binary, no new deps), or
3. dev-only: an ffmpeg with libass configured via Settings → ffmpegPath.

## Pending by-eye (needs a live premium session with a mic)

1. Stream leg shows the glass bar (~4s behind speech); native preview matches
   in stream-only sessions; recording-leg stays clean in record+stream.
2. `.srt` appears next to the recording with sane timings after stop.
3. Knobs (Top/Bottom, S/M/L) visibly apply.
4. Perf spot-check: forced-split extra render at 4K during burn-in.

Record the verdict here when run.
