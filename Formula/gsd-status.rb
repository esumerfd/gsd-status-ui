# This file is updated automatically by the release workflow.
class GsdStatus < Formula
  desc "Terminal status view for a GSD planning workspace"
  homepage "https://github.com/esumerfd/gsd-status-ui"
  version "0.2.1"

  on_macos do
    on_arm do
      url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.2.1/gsd-status-v0.2.1-aarch64-apple-darwin.tar.gz"
      sha256 "163b25fb597ffb0ca612b573c218387aaae6defa4115bd8397b12e8c08690006"
    end
    on_intel do
      url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.2.1/gsd-status-v0.2.1-x86_64-apple-darwin.tar.gz"
      sha256 "7bc60204cf7f84f2fb7d36351619d3f48c9d5badb59c86c230195fb8b32d6fb7"
    end
  end

  on_linux do
    url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.2.1/gsd-status-v0.2.1-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "fc14780e45e8ce495a95e231382cd90c5e16a6afdf04c54ce85cf98c74c77736"
  end

  def install
    bin.install "gsd-status"
  end

  test do
    system "#{bin}/gsd-status", "--help"
  end
end
