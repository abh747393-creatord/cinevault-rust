class MovieboxTui < Formula
  VERSION = "0.1.18"
  MACOS_SHA256 = "84b54b1f418c5941eb88740295a6c4d66139ec64d7a8116a6d4cae7ed4a228f8"
  LINUX_X64_SHA256 = "ab759a70f486a5f016f8500675f6a051c688afe8084fc41cefdad7ff8b9662b2"
  LINUX_ARM64_SHA256 = "57bdfdccec951553c4682100e57275cf6ea95482a3d4d69537035fe597a03a4e"

  desc "Stream movies, shows, anime, and live TV from your terminal"
  homepage "https://github.com/mesamirh/MovieBox-Tui"
  version VERSION
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    url "https://github.com/mesamirh/MovieBox-Tui/releases/download/v#{VERSION}/MovieBox_macOS_Universal.tar.gz"
    sha256 MACOS_SHA256
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/mesamirh/MovieBox-Tui/releases/download/v#{VERSION}/MovieBox_Linux_arm64.tar.gz"
      sha256 LINUX_ARM64_SHA256
    else
      url "https://github.com/mesamirh/MovieBox-Tui/releases/download/v#{VERSION}/MovieBox_Linux_x64.tar.gz"
      sha256 LINUX_X64_SHA256
    end
  end

  def install
    bin.install "moviebox-tui"
  end

  test do
    system "#{bin}/moviebox-tui", "--version"
  end
end
