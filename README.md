# hindsight-desktop

A macOS/Windows/Linux **menu-bar app** that supervises a local Hindsight
instance. It's a thin wrapper around [`hindsight-embed`](../hindsight-embed):
the app never talks to the memory engine directly — it delegates lifecycle to
the `hindsight-embed` CLI and reads liveness from the daemon's `/health`.

## What it does

It runs under a dedicated embed profile (`desktop`, pinned to port 8899) and the
tray menu has just two actions — everything else (LLM/profile config, `.env`
editing, stop/restart, control-plane UI, ports, logs) lives in hindsight-embed's
**control center**:

- **Status** — `● Hindsight — running (:8899) · API v…` or `○ stopped`, refreshed
  every 3s by polling `/health`. The menu-bar icon is the Hindsight logo (white
  template): full when running, dimmed when stopped/starting.
- **Start** — launches the daemon with `HINDSIGHT_EMBED_DAEMON_IDLE_TIMEOUT=0`,
  which disables the 5-minute idle auto-exit so the instance stays *always
  running*. Greyed out while running.
- **Open Control Center** — runs `hindsight-embed control start`, which opens the
  bundled localhost web app (profile config, supervision, logs, health).
- **Quit** — quits the menu-bar app only; the daemon keeps running.

## Install (end users)

```bash
brew install --cask nicoloboschi/tap/hindsight
```

Installs the menu-bar app and pulls `uv` as a dependency. Nothing heavy is
bundled — on first **Start** the app runs `uvx hindsight-embed`, which fetches
`hindsight-api` + models on demand. See [`packaging/README.md`](packaging/README.md).

## Requirements

- `uv`/`uvx` available — the app runs `uvx 'hindsight-embed>=0.8.2'` when no
  installed `hindsight-embed` is found. (The cask installs `uv` for you.) The
  `>=0.8.2` floor guarantees the control center is present.
- Optional: an installed `hindsight-embed` is used if present (must be `>=0.8.2`
  for the control center); override with `HINDSIGHT_EMBED_BIN=/path/to/hindsight-embed`.
- Optional: Node.js / `npx` for the Control Plane UI.

## Develop

```bash
cd hindsight-desktop/src-tauri
cargo run
```

The first build compiles the Tauri/wry stack and takes a few minutes.

## Build a bundle

```bash
cargo install tauri-cli --version '^2'   # once
cd hindsight-desktop
cargo tauri build                          # .app / .dmg / .msi / .deb ...
```

## Shipping notes (not done yet)

- **Self-contained install.** v1 calls `hindsight-embed` from `PATH`. To ship to
  users who don't have it, bundle `hindsight-embed` as a Tauri sidecar (or
  require `uvx`). Only `supervisor::embed_bin` needs to change.
- **Code signing / notarization / auto-update.** Required before distributing
  outside your own machine; not configured here.
