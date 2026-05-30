# Videogre

Videogre is an AI-native desktop studio for creator recording and livestreaming workflows.

This repository currently contains the technical spike:

- Electron + React/TypeScript desktop shell
- Rust backend process launched by Electron
- Authenticated localhost WebSocket protocol
- Device discovery stubs with FFmpeg-backed macOS device probing
- FFmpeg-backed test recording path that writes MKV files

Raw media frames do not move through Electron IPC. Electron receives backend connection details, state updates, device metadata, recording status, and logs.

## Prerequisites

- Node.js 24+
- pnpm 11+
- Rust stable via rustup
- FFmpeg available on `PATH`

The spike uses the system FFmpeg binary only. Distribution and closed-source licensing decisions for bundled FFmpeg builds are intentionally out of scope.

## Development

```sh
pnpm install
pnpm dev
```

The desktop app launches the Rust backend automatically. Recordings default to:

```text
~/Movies/Videogre/Recordings
```

## Verification

```sh
pnpm typecheck
pnpm build
cargo fmt --check
cargo test
cargo clippy -- -D warnings
```
