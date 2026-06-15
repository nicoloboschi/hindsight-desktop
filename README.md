# hindsight-desktop

A macOS/Windows/Linux **menu-bar app** that supervises a local Hindsight
instance. It's a thin wrapper around [`hindsight-embed`](../hindsight-embed):
the app never talks to the memory engine directly — it delegates lifecycle to
the `hindsight-embed` CLI and reads liveness from the daemon's `/health`.

## What it does

It runs under a dedicated embed profile (`desktop`, pinned to port 8899). It lives
in both the **Dock** and the **menu bar** — the Dock icon is the reliable surface
(the menu-bar icon can be hidden by a notch / a full menu bar). Everything beyond
start/open (LLM/profile config, `.env` editing, stop/restart, control-plane UI,
ports, logs) lives in hindsight-embed's **control center**:

- **Click the Dock icon** (or relaunch the app) → opens the **control center**
  deep-linked to the `desktop` profile. This is the main entry point.
- **Menu-bar icon** — the Hindsight logo (white template, dims when stopped),
  with a small menu:
  - **Status** — `● running (:8899) · API v…` / `○ stopped`, polled every 3s.
  - **Start** — launches the daemon with `HINDSIGHT_EMBED_DAEMON_IDLE_TIMEOUT=0`
    (no idle auto-exit, so it stays *always running* while the app runs).
  - **Open Control Center** — same as clicking the Dock icon.
- **Quit** (menu item, Dock → Quit, or Cmd-Q) → tears down the daemon **and** the
  control center. The control center is auto-started whenever the app launches.

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
