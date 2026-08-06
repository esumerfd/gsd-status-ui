# This file is updated automatically by the release workflow.
class GsdStatus < Formula
  desc "Terminal status view for a GSD planning workspace"
  homepage "https://github.com/esumerfd/gsd-status-ui"
  version "0.7.0"

  on_macos do
    on_arm do
      url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.7.0/gsd-status-v0.7.0-aarch64-apple-darwin.tar.gz"
      sha256 "8b69b76fb2061674f4f3559f42578b7dfc2d3d3c9ada2d2378048b0bb3c148b3"
    end
    on_intel do
      url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.7.0/gsd-status-v0.7.0-x86_64-apple-darwin.tar.gz"
      sha256 "9d972e02f3a6e30e5d4926927674db78900b9ff7abd1cfa48757861dafa50400"
    end
  end

  on_linux do
    url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.7.0/gsd-status-v0.7.0-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "e270ece233f06b5cc9919e831ea488fb00a0a2d41efcf3068bd5370f1840f761"
  end

  def install
    bin.install "gsd-status"
  end

  test do
    system "#{bin}/gsd-status", "--help"
  end
end
