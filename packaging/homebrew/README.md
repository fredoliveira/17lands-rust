# Homebrew tap

`seventeenlands-rust.rb` is a binary "pour" formula — it installs the prebuilt
binaries attached to each GitHub release, so users don't need a Rust toolchain. It
covers macOS (arm64 + x86_64) and Linux (x86_64); Windows isn't a Homebrew target.

> Note on notarization: Homebrew downloads via `curl`, which does **not** apply the
> `com.apple.quarantine` xattr, so Gatekeeper never blocks brew-installed CLIs even
> though these binaries are only ad-hoc signed (not Apple-notarized). No notarization
> is required for this path to "just work".

## One-time: create the tap repo

A Homebrew tap is just a GitHub repo named `homebrew-<tap>` containing a `Formula/`
directory. Create one (e.g. `fredoliveira/homebrew-tap`) and drop the formula in:

```sh
gh repo create fredoliveira/homebrew-tap --public --clone
cd homebrew-tap
mkdir -p Formula
cp /path/to/17lands-rust/packaging/homebrew/seventeenlands-rust.rb Formula/
git add Formula/seventeenlands-rust.rb
git commit -m "Add seventeenlands-rust 0.1.0"
git push
```

Users then install with:

```sh
brew install fredoliveira/tap/seventeenlands-rust
```

## On every release

Bump `version` and the three `sha256` values in the formula (the URLs are derived
from `version`). Get the checksums from the release assets:

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
