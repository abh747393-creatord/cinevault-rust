class MovieboxTui < Formula
  VERSION = "0.1.19"
  MACOS_SHA256 = "805cb97a2d3f4c94219a4ee9a951a933910bbe19ae97ea2fbfec3c57a392f03d"
  LINUX_X64_SHA256 = "69603fc960e2f9c64e8415d74d5410614b35af9f7a5a2eda54dd210f057fee5f"
  LINUX_ARM64_SHA256 = "12a0d75a9bd99b58f5ca0a0a2bf1ba7d24d47ba22e6f1d51765d4ec3def09474"

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
