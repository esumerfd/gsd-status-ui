# This file is updated automatically by the release workflow.
class GsdStatus < Formula
  desc "Terminal status view for a GSD planning workspace"
  homepage "https://github.com/esumerfd/gsd-status-ui"
  version "0.1.0"

  on_macos do
    on_arm do
      url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.1.0/gsd-status-v0.1.0-aarch64-apple-darwin.tar.gz"
      sha256 "b0f44d0f74e7bda3d435e2d155750a49910f86836be8964ab3160becbbc1537a"
    end
    on_intel do
      url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.1.0/gsd-status-v0.1.0-x86_64-apple-darwin.tar.gz"
      sha256 "64e87a3b68fd56e222a593b8692b01b8815cad1ab7e45659d45039e96d38e111"
    end
  end

  on_linux do
    url "https://github.com/esumerfd/gsd-status-ui/releases/download/v0.1.0/gsd-status-v0.1.0-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "1cfa2a0a2cc95920f80ad7b0432aadc86e21cfaf3f3779b482df9ae0ff204008"
  end

  def install
    bin.install "gsd-status"
  end

  test do
    system "#{bin}/gsd-status", "--help"
  end
end
