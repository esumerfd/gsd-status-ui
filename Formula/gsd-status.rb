# This file is updated automatically by the release workflow.
class GsdStatus < Formula
  desc "Terminal status view for a GSD planning workspace"
  homepage "https://github.com/esumerfd/gsd-status-ui"
  version "0.10.0"

  on_macos do
    on_arm do
      url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.10.0/gsd-status-v0.10.0-aarch64-apple-darwin.tar.gz"
      sha256 "89278ef0b25691bb02ec5df0704e1d78828fe262376b2a82c07c3401f535bd97"
    end
    on_intel do
      url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.10.0/gsd-status-v0.10.0-x86_64-apple-darwin.tar.gz"
      sha256 "74e6dd0d0ffd0107d07b3c44e698a079cba621818f9ec68f9abc15ba0dd47182"
    end
  end

  on_linux do
    url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.10.0/gsd-status-v0.10.0-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "32051bb2d09c44442c546ca88fc63ab9c2203cfc9c34ab9ecfd1b275498156b3"
  end

  def install
    bin.install "gsd-status"
  end

  test do
    system "#{bin}/gsd-status", "--help"
  end
end
