# This file is updated automatically by the release workflow.
class GsdStatus < Formula
  desc "Terminal status view for a GSD planning workspace"
  homepage "https://github.com/esumerfd/gsd-status-ui"
  version "0.2.0"

  on_macos do
    on_arm do
      url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.2.0/gsd-status-v0.2.0-aarch64-apple-darwin.tar.gz"
      sha256 "d3603889d4b9722d4217ef3826abe70151d1009605d1365b17a01580f9a40fbf"
    end
    on_intel do
      url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.2.0/gsd-status-v0.2.0-x86_64-apple-darwin.tar.gz"
      sha256 "097c11956a2888cf005e77bd7925898b93d061403a8b0e6cd324b361d04ce8bd"
    end
  end

  on_linux do
    url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.2.0/gsd-status-v0.2.0-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "93c72252e2006ac33c460b3842514fc99259ca9ff000a61f722b5a3708139f85"
  end

  def install
    bin.install "gsd-status"
  end

  test do
    system "#{bin}/gsd-status", "--help"
  end
end
