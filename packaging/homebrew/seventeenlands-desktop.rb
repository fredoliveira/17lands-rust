# Homebrew Cask for the 17Lands desktop app (menu-bar). Source of truth for the tap's
# `Casks/` directory. Users install with:
#
#     brew install --cask fredoliveira/tap/seventeenlands-desktop
#
# The universal (Intel + Apple Silicon) .dmg is built and attached by the release workflow.
# On every release, bump `version` and `sha256` (see packaging/homebrew/README.md).
#
# NOTE: the app is currently only ad-hoc signed (not Apple-notarized), so Gatekeeper
# quarantines it on first launch. Until notarization is set up, open it once via
# right-click -> Open, or clear the quarantine attribute:
#     xattr -dr com.apple.quarantine "/Applications/17Lands.app"
cask "seventeenlands-desktop" do
  version "0.1.1"
  sha256 "0000000000000000000000000000000000000000000000000000000000000000"

  url "https://github.com/fredoliveira/17lands-rust/releases/download/v#{version}/17Lands-universal.dmg"
  name "17Lands"
  desc "Menu-bar app that uploads MTG Arena data to 17lands.com"
  homepage "https://github.com/fredoliveira/17lands-rust"

  depends_on macos: ">= :big_sur"

  app "17Lands.app"

  zap trash: "~/Library/Application Support/17l"
end
