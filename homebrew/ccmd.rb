# typed: false
# frozen_string_literal: true

# To use this formula, create a repo named juliensimon/homebrew-tap
# and place this file at Formula/ccmd.rb
#
# Users install with:
#   brew tap juliensimon/tap
#   brew install ccmd

class Ccmd < Formula
  desc "Cache Commander — a TUI for browsing and managing cache directories"
  homepage "https://github.com/juliensimon/cache-commander"
  version "0.1.1"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/juliensimon/cache-commander/releases/download/v#{version}/ccmd-aarch64-apple-darwin.tar.gz"
      sha256 "7e037561082513300f118065d4de6e8b52e242c9b7fde7f36853e689a70a6d3e"
    end
    on_intel do
      url "https://github.com/juliensimon/cache-commander/releases/download/v#{version}/ccmd-x86_64-apple-darwin.tar.gz"
      sha256 "2397b29823ee11813d4a592cd37e89c349e051b4492a4d3161ff03202e3493b6"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/juliensimon/cache-commander/releases/download/v#{version}/ccmd-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "95732d49e608d7655c6eb8dca158a6eaddc4d8ea13d8c9387d7c9c02be6e7534"
    end
    on_intel do
      url "https://github.com/juliensimon/cache-commander/releases/download/v#{version}/ccmd-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "e7a93a6293973ff280bcb18fe1ca5ffe61e7a32a051c5150830383d7d703674e"
    end
  end

  def install
    bin.install "ccmd"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/ccmd --version")
  end
end
