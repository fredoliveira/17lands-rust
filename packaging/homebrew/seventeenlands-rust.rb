# Homebrew formula for seventeenlands-rust (binary "pour" — no Rust toolchain needed).
#
# This file is the source of truth for the tap. Users install with:
#
#     brew install fredoliveira/tap/seventeenlands-rust
#
# On every release, bump the tag in the three urls and their `sha256` values.
# `brew bump-formula-pr` automates this. Homebrew scans the version from the url tag.
class SeventeenlandsRust < Formula
  desc "Rust port of the 17Lands MTGA log client (uploads MTG Arena data to 17lands.com)"
  homepage "https://github.com/fredoliveira/17lands-rust"
  license "GPL-3.0-only"

  on_macos do
    on_arm do
      url "https://github.com/fredoliveira/17lands-rust/releases/download/v0.1.2/seventeenlands-rust-aarch64-apple-darwin.tar.gz"
      sha256 "4f115a0cf4edebd19b307f89e06cabd686e1e4da3864cfbf017cf36671634d54"
    end
    on_intel do
      url "https://github.com/fredoliveira/17lands-rust/releases/download/v0.1.2/seventeenlands-rust-x86_64-apple-darwin.tar.gz"
      sha256 "b1c05e2e78f0134e29492e3d31411426cfb36aecf7ab7dda1b19056ad9c89fc1"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/fredoliveira/17lands-rust/releases/download/v0.1.2/seventeenlands-rust-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "339b9f442d5ed06c4189e16cb0494d0e5e500164c0aa1821e5fff26fff12ad9d"
    end
  end

  def install
    bin.install "seventeenlands"
  end

  test do
    assert_match "17lands.com", shell_output("#{bin}/seventeenlands --help")
  end
end
