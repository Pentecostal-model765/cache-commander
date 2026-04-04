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
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/juliensimon/cache-commander/releases/download/v#{version}/ccmd-aarch64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER"
    end
    on_intel do
      url "https://github.com/juliensimon/cache-commander/releases/download/v#{version}/ccmd-x86_64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/juliensimon/cache-commander/releases/download/v#{version}/ccmd-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "PLACEHOLDER"
    end
    on_intel do
      url "https://github.com/juliensimon/cache-commander/releases/download/v#{version}/ccmd-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "PLACEHOLDER"
    end
  end

  def install
    bin.install "ccmd"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/ccmd --version")
  end
end
