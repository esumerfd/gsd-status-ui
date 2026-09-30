# This file is updated automatically by the release workflow.
class GsdStatus < Formula
  desc "Terminal status view for a GSD planning workspace"
  homepage "https://github.com/esumerfd/gsd-status-ui"
  version "0.9.0"

  on_macos do
    on_arm do
      url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.9.0/gsd-status-v0.9.0-aarch64-apple-darwin.tar.gz"
      sha256 "6dc06b1317447770f45b67345faceb10ca08591a011d37838d85ddda94e57400"
    end
    on_intel do
      url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.9.0/gsd-status-v0.9.0-x86_64-apple-darwin.tar.gz"
      sha256 "a36a41c5de33c4896f2705548d159a7d087e7c4d15b95ca41f5349167dc2e7fe"
    end
  end

  on_linux do
    url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.9.0/gsd-status-v0.9.0-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "d209761447e89c258376e0b1dbff8117e7c819bac16d182930b7a4826c62acac"
  end

  def install
    bin.install "gsd-status"
  end

  test do
    system "#{bin}/gsd-status", "--help"
  end
end
