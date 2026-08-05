# This file is updated automatically by the release workflow.
class GsdStatus < Formula
  desc "Terminal status view for a GSD planning workspace"
  homepage "https://github.com/esumerfd/gsd-status-ui"
  version "0.6.0"

  on_macos do
    on_arm do
      url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.6.0/gsd-status-v0.6.0-aarch64-apple-darwin.tar.gz"
      sha256 "55aa215255bda2a305075c577d508325e52aa218548b29a31fa42e73f98bc0ef"
    end
    on_intel do
      url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.6.0/gsd-status-v0.6.0-x86_64-apple-darwin.tar.gz"
      sha256 "ed815470ed0220935e824faf7fb27e0f0e69ee76a9f286965b52d6249a074937"
    end
  end

  on_linux do
    url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.6.0/gsd-status-v0.6.0-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "50463cbc806a816efd7026c3a5f884a191dc2c74fa642ffa02756dbb72a5f050"
  end

  def install
    bin.install "gsd-status"
  end

  test do
    system "#{bin}/gsd-status", "--help"
  end
end
