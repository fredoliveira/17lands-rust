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
      url "https://github.com/fredoliveira/17lands-rust/releases/download/v0.1.1/seventeenlands-rust-aarch64-apple-darwin.tar.gz"
      sha256 "317c6b8d227b663fda5f1db936dfa53b5a2eb5e7795730f9f293ac77887f3a50"
    end
    on_intel do
      url "https://github.com/fredoliveira/17lands-rust/releases/download/v0.1.1/seventeenlands-rust-x86_64-apple-darwin.tar.gz"
      sha256 "7f6f9d4c25d79059812bd99453a69786d4f55aade597f67afc841f6e4af9988a"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/fredoliveira/17lands-rust/releases/download/v0.1.1/seventeenlands-rust-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "5cb4dacd0bc4edbe59c1653526b19c1da76539024964d1e8b5b6624e5c5f39f9"
    end
  end

  def install
    bin.install "seventeenlands"
  end

  test do
    assert_match "17lands.com", shell_output("#{bin}/seventeenlands --help")
  end
end
