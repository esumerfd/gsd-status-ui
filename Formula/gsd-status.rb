# This file is updated automatically by the release workflow.
class GsdStatus < Formula
  desc "Terminal status view for a GSD planning workspace"
  homepage "https://github.com/esumerfd/gsd-status-ui"
  version "0.8.0"

  on_macos do
    on_arm do
      url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.8.0/gsd-status-v0.8.0-aarch64-apple-darwin.tar.gz"
      sha256 "e47645582366358d54e337617fec558ce429262fb3585134c036e4138fe3c091"
    end
    on_intel do
      url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.8.0/gsd-status-v0.8.0-x86_64-apple-darwin.tar.gz"
      sha256 "272c13a7cfa70aa7eebea27a5dfb6e31cad7e82f521e5333d5963ebd50bc76e2"
    end
  end

  on_linux do
    url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.8.0/gsd-status-v0.8.0-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "5682f36f1199a0564ce332182946c5398621651d8504e982543712a6483785dd"
  end

  def install
    bin.install "gsd-status"
  end

  test do
    system "#{bin}/gsd-status", "--help"
  end
end
