# Homebrew tap

This tap ships two things, both built and attached to each GitHub release:

- **`seventeenlands-rust.rb`** — a binary "pour" **formula** for the `seventeenlands`
  CLI. Covers macOS (arm64 + x86_64) and Linux (x86_64); Windows isn't a Homebrew target.
- **`seventeenlands-desktop.rb`** — a **cask** for the menu-bar desktop app (a universal
  `.dmg`, Intel + Apple Silicon). macOS only.

> Note on notarization (CLI): Homebrew downloads via `curl`, which does **not** apply the
> `com.apple.quarantine` xattr, so Gatekeeper never blocks the brew-installed CLI even
> though the binary is only ad-hoc signed (not Apple-notarized).
>
> The **cask is different**: `brew install --cask` quarantines the `.app`, so the unsigned
> desktop app is blocked by Gatekeeper on first launch. Until the app is Apple-notarized,
> users must open it once via right-click → Open (or run
> `xattr -dr com.apple.quarantine "/Applications/17Lands.app"`). The cask file documents this.

## One-time: create the tap repo

A Homebrew tap is just a GitHub repo named `homebrew-<tap>` with a `Formula/` directory
(and `Casks/` for casks). Create one (e.g. `fredoliveira/homebrew-tap`) and drop both in:

```sh
gh repo create fredoliveira/homebrew-tap --public --clone
cd homebrew-tap
mkdir -p Formula Casks
cp /path/to/17lands-rust/packaging/homebrew/seventeenlands-rust.rb    Formula/
cp /path/to/17lands-rust/packaging/homebrew/seventeenlands-desktop.rb Casks/
git add Formula/ Casks/
git commit -m "Add seventeenlands-rust CLI + desktop cask"
git push
```

Users then install:

```sh
brew install fredoliveira/tap/seventeenlands-rust          # the CLI
brew install --cask fredoliveira/tap/seventeenlands-desktop  # the menu-bar app
```

## On every release — CLI formula

Bump the tag (`v0.1.0` → `vX.Y.Z`) in the three `url` lines and update their
`sha256` values. There's no `version` field to touch — Homebrew scans the version
from the release tag in the URL. Get the checksums from the release assets:

```sh
gh release download vX.Y.Z -R fredoliveira/17lands-rust -p '*.tar.gz' -D /tmp/rel
shasum -a 256 /tmp/rel/*.tar.gz
```

Or let Homebrew do it for you from inside the tap repo:

```sh
brew bump-formula-pr --version=X.Y.Z fredoliveira/tap/seventeenlands-rust
```

## Verify the formula before publishing

From within the tap repo (or by pointing `brew` at the file):

```sh
brew audit --strict --online Formula/seventeenlands-rust.rb
brew install --build-from-source Formula/seventeenlands-rust.rb  # for a source formula
brew install Formula/seventeenlands-rust.rb                      # binary pour
brew test seventeenlands-rust
```

## On every release — desktop cask

The `Release Desktop` workflow (`release-desktop.yml`, triggered by the same `v*` tag as
the CLI release) attaches a single universal asset, `17Lands-universal.dmg`, to the
release. If the desktop build failed while the CLI release succeeded, re-run it via
workflow_dispatch and it will attach the `.dmg` to the existing release. In `seventeenlands-desktop.rb`, bump `version` to match
the tag (without the `v`) and update `sha256`:

```sh
gh release download vX.Y.Z -R fredoliveira/17lands-rust -p '17Lands-universal.dmg' -D /tmp/rel
shasum -a 256 /tmp/rel/17Lands-universal.dmg
```

> First availability: the `.dmg` ships starting with the **next tagged release** cut after
> this change (the existing `v0.1.1` release predates the `desktop` job, so it has no
> `.dmg` asset). The placeholder `sha256` in the cask must be replaced with the real one then.

Verify before publishing:

```sh
brew audit --strict --online --cask Casks/seventeenlands-desktop.rb
brew install --cask Casks/seventeenlands-desktop.rb
```

Notarization (to remove the Gatekeeper prompt) is a future step: it needs an Apple
Developer ID signing cert + `xcrun notarytool` in the `desktop` release job, after which
the cask "just works" without the manual quarantine step.
