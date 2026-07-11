# This file is updated automatically by the release workflow.
class GsdStatus < Formula
  desc "Terminal status view for a GSD planning workspace"
  homepage "https://github.com/esumerfd/gsd-status-ui"
  version "0.3.0"

  on_macos do
    on_arm do
      url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.3.0/gsd-status-v0.3.0-aarch64-apple-darwin.tar.gz"
      sha256 "2310916996fb3d20a66b111406104c703e144a24b83258382d7b3ef8c3dd6775"
    end
    on_intel do
      url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.3.0/gsd-status-v0.3.0-x86_64-apple-darwin.tar.gz"
      sha256 "d9e698f6d358149500f3005d4f6f8ed55a357322e18397ac0b61bd3bb33ef595"
    end
  end

  on_linux do
    url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.3.0/gsd-status-v0.3.0-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "267e7650a2097d6393f532207d50748d7b614562b5a11f1432ba3ce222fcfeab"
  end

  def install
    bin.install "gsd-status"
  end

  test do
    system "#{bin}/gsd-status", "--help"
  end
end
