# This file is updated automatically by the release workflow.
class GsdStatus < Formula
  desc "Terminal status view for a GSD planning workspace"
  homepage "https://github.com/esumerfd/gsd-status-ui"
  version "0.4.0"

  on_macos do
    on_arm do
      url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.4.0/gsd-status-v0.4.0-aarch64-apple-darwin.tar.gz"
      sha256 "e665dc76f0415bdfca586d4b05b6aed996b1cef32a9cfa16b95ecc2499056a26"
    end
    on_intel do
      url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.4.0/gsd-status-v0.4.0-x86_64-apple-darwin.tar.gz"
      sha256 "6d283cc40b2099533d6fd7c8289827fcb8203d421cdfe2258bb473a0e40d2f89"
    end
  end

  on_linux do
    url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.4.0/gsd-status-v0.4.0-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "3e4c5aad7261674eeed5d3b1b97376f4a596e12f3165e1ebb5c8ba6a005454c2"
  end

  def install
    bin.install "gsd-status"
  end

  test do
    system "#{bin}/gsd-status", "--help"
  end
end
