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
  version "0.1.5"
  sha256 "f866a2297b610a97c24cab451ee7b4d92bdf69b306d7168435c74a73ec82efc9"

  url "https://github.com/nicoloboschi/hindsight-desktop/releases/download/v#{version}/Hindsight_#{version}_aarch64.dmg"
  name "Hindsight"
  desc "Menu-bar supervisor for a local Hindsight memory instance"
  homepage "https://github.com/nicoloboschi/hindsight-desktop"

  # `uv` provides `uvx`, which the app uses to fetch and run hindsight-embed
  # (and, on first run, hindsight-api + models). Nothing heavy is bundled.
  depends_on formula: "uv"

  depends_on macos: :catalina

  app "Hindsight.app"

  # The app is ad-hoc signed (not notarized), so a quarantined copy trips
  # Gatekeeper's "damaged" error. Clear the quarantine flag on install so the
  # app opens with no warning. (Remove this once the app is notarized.)
  postflight do
    system_command "/usr/bin/xattr",
                   args: ["-cr", "#{appdir}/Hindsight.app"],
                   sudo: false
  end

  caveats <<~EOS
    Hindsight isn't notarized; this cask clears its quarantine flag on install
    so it opens normally. If macOS ever still says it is "damaged", run:

      xattr -cr /Applications/Hindsight.app
  EOS

  # Only remove the app's own runtime artifacts on `brew uninstall --zap`.
  # NOTE: deliberately does NOT touch ~/.hindsight/embed or the pg0 data dir —
  # that is the user's memory store and must survive an uninstall.
  zap trash: [
    "~/.hindsight/daemon.log",
    "~/.hindsight/daemon.lock",
  ]
end
