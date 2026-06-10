# Packaging & distribution

How an end user installs Hindsight:

```bash
brew install --cask nicoloboschi/tap/hindsight
```

That installs `Hindsight.app` into `/Applications` (it lives in the **menu bar**,
no dock icon) and pulls `uv` as a dependency. Nothing else is bundled: on the
first **Start**, the app runs `uvx hindsight-embed`, which fetches `hindsight-api`
and the ML models on demand (~1–3 min the first time). Subsequent starts are fast.

## What ships vs. what's fetched

| Layer | Where it comes from |
|-------|---------------------|
| Menu-bar app (`Hindsight.app`, ~tens of MB) | the cask / `.dmg` |
| `uv` / `uvx` | Homebrew dependency (`depends_on formula: "uv"`) |
| `hindsight-embed` | fetched at runtime via `uvx hindsight-embed` |
| `hindsight-api` + models | fetched by hindsight-embed on first daemon start |
| Node/`npx` (only for the Control Plane UI) | optional `depends_on formula: "node"` |

## Cutting a desktop release

1. **Build the bundle:**
   ```bash
   cargo install tauri-cli --version '^2'   # once
   cd hindsight-desktop && cargo tauri build
   ```
   Artifacts land in `src-tauri/target/release/bundle/` (`.dmg`, `.app`; also
   `.msi`/`.deb`/`.AppImage` on those platforms).

2. **Sign + notarize (macOS, required for friction-free installs).** With an
   Apple Developer ID, set the signing identity + notarization creds and Tauri
   signs/notarizes during `tauri build`. Until then the app is unsigned: users
   must right-click → Open once, or run
   `xattr -dr com.apple.quarantine /Applications/Hindsight.app`.

3. **Publish a GitHub Release** in `nicoloboschi/hindsight-desktop` tagged
   `vX.Y.Z` and upload the `.dmg` (named to match the cask's `url`, e.g.
   `Hindsight_X.Y.Z_aarch64.dmg`).

4. **Update the cask** in `packaging/homebrew/hindsight.rb`:
   ```bash
   shasum -a 256 Hindsight_X.Y.Z_aarch64.dmg   # -> sha256
   ```
   Bump `version` + `sha256`, then copy the file to the tap repo
   `nicoloboschi/homebrew-tap` at `Casks/hindsight.rb` and push.

## The tap (one-time)

Create a public repo `nicoloboschi/homebrew-tap` with `Casks/hindsight.rb`.
Users then `brew install --cask nicoloboschi/tap/hindsight`. A custom tap avoids
the official homebrew-cask repo's hard notarization requirement, but Gatekeeper
still quarantines unsigned apps on first launch — notarization (step 2) is the
real fix for a smooth install.
