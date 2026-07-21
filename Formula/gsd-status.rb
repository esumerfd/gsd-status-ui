# This file is updated automatically by the release workflow.
class GsdStatus < Formula
  desc "Terminal status view for a GSD planning workspace"
  homepage "https://github.com/esumerfd/gsd-status-ui"
  version "0.5.0"

  on_macos do
    on_arm do
      url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.5.0/gsd-status-v0.5.0-aarch64-apple-darwin.tar.gz"
      sha256 "b31cbfa95da8db0b0a6b52faa4e9d2ac290a2a24bb3f479f3212576fde29f531"
    end
    on_intel do
      url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.5.0/gsd-status-v0.5.0-x86_64-apple-darwin.tar.gz"
      sha256 "f6fa6f1f847f9ad7bad97232211d3a709126257666523e1c35aeb55061ad6f2e"
    end
  end

  on_linux do
    url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.5.0/gsd-status-v0.5.0-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "0b3618ca2d0fa9458977f53bf58e8b51d5e3370409a0aad4803d91a2de7e80fc"
  end

  def install
    bin.install "gsd-status"
  end

  test do
    system "#{bin}/gsd-status", "--help"
  end
end
