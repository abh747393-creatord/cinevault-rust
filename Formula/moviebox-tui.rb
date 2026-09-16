class MovieboxTui < Formula
  VERSION = "0.1.20"
  MACOS_SHA256 = "b35b9daa63bd42189aca46275d3a2b067bcb3c1c4f0d2847e8f86846b7808c15"
  LINUX_X64_SHA256 = "a0a6969047e18aa58fa843d65a5095c91f57edc94e99ee9853eb8295d53a9277"
  LINUX_ARM64_SHA256 = "ba265a6d4cf224dd4d7cd2af49c8c63cea25cbde33b71cbd5dc178a4dd4f3d4e"

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
