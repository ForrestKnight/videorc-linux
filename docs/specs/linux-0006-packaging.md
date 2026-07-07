# Linux port 0006 — Phase 5: packaging (FFmpeg provisioning + AppImage)

## Goal

A distributable Linux artifact that bundles the backend and FFmpeg so an
end user runs one file with no dependencies. AppImage first (universal across
modern glibc distros — the "not Hyprland-specific" proof target); Flatpak and
an AUR PKGBUILD come later.

## macOS/Windows precedent being matched

- FFmpeg provisioning: `build-ffmpeg-macos.sh` builds from source;
  `fetch-ffmpeg-windows.mjs` fetches a pinned prebuilt LGPL BtbN zip and lays
  it out under `vendor/ffmpeg/windows-x64/`. Both keep an LGPL-only policy and
  write a `SOURCE.txt` breadcrumb.
- electron-builder: `mac:` and `win:` sections each override the top-level
  (macOS) `extraResources` with their own backend binary + ffmpeg dir, and
  declare their targets (dmg/zip, nsis).
- The Electron main process already resolves packaged paths generically:
  `resolvePackagedBackendBinary` returns `resourcesPath/videorc-backend` on
  non-Windows, and `resolvePackagedFfmpegBinDir` looks for
  `resourcesPath/ffmpeg/bin/ffmpeg` — both Linux-ready with no code change.

## Linux design

1. **FFmpeg**: `scripts/fetch-ffmpeg-linux.mjs` mirrors the Windows fetch —
   pinned URL + sha256 in `vendor/ffmpeg/linux-pin.json`, LGPL-only guard,
   `SOURCE.txt` breadcrumb, gitignored payload under
   `vendor/ffmpeg/linux-x64/`. Source: BtbN `linux64-lgpl` (static; carries
   both `ffmpeg` and `ffprobe` — the backend derives ffprobe as a sibling for
   post-recording quality checks, so both are bundled). The BtbN `latest` URL
   is a moving target, so the sha256 IS the reproducibility pin: a rebuilt
   upstream asset fails the checksum and forces a deliberate pin bump.
2. **electron-builder** `linux:` section: overrides `extraResources` with the
   linux backend binary + `vendor/ffmpeg/linux-x64`, targets `dir` + AppImage,
   category AudioVideo.
3. **Package scripts**: `ffmpeg:fetch:linux`, `package:desktop:linux`,
   `dist:desktop:linux` — same shape as the Windows scripts.
4. **No signing** yet (AppImage runs unsigned; the auto-update feed for Linux
   is a later release-infra task — the app logs a benign 404 until it exists,
   exactly like the interim Windows state).

## What was verified on this machine (Arch/Hyprland)

- `pnpm ffmpeg:fetch:linux` downloads, checksum-verifies, and lays out
  ffmpeg + ffprobe; the binaries run (`ffmpeg -version` OK).
- `cargo build --release -p videorc-backend` produces the release backend.
- `electron-builder --linux AppImage` produces
  `Videorc-<version>-linux-x86_64.AppImage` (~235 MB, dominated by the two
  static FFmpeg binaries) with the backend + ffmpeg/ffprobe correctly under
  `resources/`.
- Launching the AppImage mounts it and its **bundled backend connects**
  ("Backend ready on 127.0.0.1:PORT", "Videorc backend ready"). The only
  error is the expected missing Linux update feed (404).

## Not verified here (the outstanding phase-5 gate)

- **Clean non-Arch VM (Fedora/Ubuntu GNOME).** This is the actual
  "not Hyprland-specific" proof and cannot run on this dev box — it needs a
  separate VM. AppImage's design (bundled glibc-compatible binaries; screen
  capture through the same xdg portals GNOME/KDE implement) is built for
  exactly this, but the proof is a human/CI step on another machine. This is
  the one remaining phase-5 item, called out in `docs/linux-port-status.md`.
- Flatpak and AUR PKGBUILD (explicitly "later" in the plan).

## Size note

The AppImage is large (~235 MB) because the BtbN static ffmpeg and ffprobe
are ~115 MB each. Options for later: a size-optimized/stripped FFmpeg build,
or dropping bundled ffprobe if the runtime quality-check path can tolerate a
system fallback. Kept static+bundled for now per the brief's distro-
portability preference.

## Test plan

- `pnpm ffmpeg:fetch:linux` idempotent (skips when the pin matches, re-fetches
  on `--force`, fails loudly on checksum drift).
- `pnpm dist:desktop:linux` produces a launchable AppImage whose bundled
  backend connects.
- Clean-VM record test (screen+cam+mic) — deferred to a Fedora/Ubuntu VM.
