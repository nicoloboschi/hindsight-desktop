# Homebrew Cask for the Hindsight menu-bar app.
#
# Source of truth lives here; the published copy belongs in the tap repo
# `nicoloboschi/homebrew-tap` at `Casks/hindsight.rb`. On each desktop release,
# bump `version` + `sha256` (see packaging/README.md) and push to the tap.
#
# Install:  brew install --cask nicoloboschi/tap/hindsight
#
# NOTE: the `url` filename is finalized after the first `cargo tauri build`
# (Tauri names the dmg per-arch, e.g. Hindsight_0.1.0_aarch64.dmg); update it
# and the sha256 to match the uploaded release asset.
cask "hindsight" do
  version "0.1.1"
  sha256 "b3dc302bd4ccf345e4243d9c17c46c049671ebba1e0f86e16497d16864053073"

  url "https://github.com/nicoloboschi/hindsight-desktop/releases/download/v#{version}/Hindsight_#{version}_aarch64.dmg"
  name "Hindsight"
  desc "Menu-bar supervisor for a local Hindsight memory instance"
  homepage "https://github.com/nicoloboschi/hindsight-desktop"

  # `uv` provides `uvx`, which the app uses to fetch and run hindsight-embed
  # (and, on first run, hindsight-api + models). Nothing heavy is bundled.
  depends_on formula: "uv"
  # The Control Plane UI ("Open Control Plane UI") needs Node/npx. Uncomment to
  # auto-install it; core memory ops work without it.
  # depends_on formula: "node"

  depends_on macos: ">= :catalina"

  app "Hindsight.app"

  # Only remove the app's own runtime artifacts on `brew uninstall --zap`.
  # NOTE: deliberately does NOT touch ~/.hindsight/embed or the pg0 data dir —
  # that is the user's memory store and must survive an uninstall.
  zap trash: [
    "~/.hindsight/daemon.log",
    "~/.hindsight/daemon.lock",
  ]
end
