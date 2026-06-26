# Homebrew formula for seventeenlands-rust (binary "pour" — no Rust toolchain needed).
#
# This file is the source of truth for the tap. To publish it, copy it into a tap
# repository named `homebrew-<tap>` under `Formula/` (see packaging/homebrew/README.md),
# then users install with:
#
#     brew install fredoliveira/tap/seventeenlands-rust
#
# On every release, bump `version` and the three `sha256` values (the urls are derived
# from `version`). `brew bump-formula-pr` automates this.
class SeventeenlandsRust < Formula
  desc "Rust port of the 17Lands MTGA log client (uploads MTG Arena data to 17lands.com)"
  homepage "https://github.com/fredoliveira/17lands-rust"
  version "0.1.0"
  license "GPL-3.0-only"

  on_macos do
    on_arm do
      url "https://github.com/fredoliveira/17lands-rust/releases/download/v#{version}/seventeenlands-rust-aarch64-apple-darwin.tar.gz"
      sha256 "0746a60475e4fab0c09fb74272ae3dc7b3e07c79ca98b4d411b7610dfcce4a20"
    end
    on_intel do
      url "https://github.com/fredoliveira/17lands-rust/releases/download/v#{version}/seventeenlands-rust-x86_64-apple-darwin.tar.gz"
      sha256 "8c36cb6bab5ad61aae9e75b68bd39abffffdd842611e761cc3673a0985358c67"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/fredoliveira/17lands-rust/releases/download/v#{version}/seventeenlands-rust-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "be5a9ea85ba65c08db27059b5020d2891559bbf2dfc46372605b6f3e91a612b3"
    end
  end

  def install
    bin.install "seventeenlands-rust"
  end

  test do
    assert_match "17lands.com", shell_output("#{bin}/seventeenlands-rust --help")
  end
end
