class Numr < Formula
  desc "Text calculator for natural language expressions with a vim-style TUI"
  homepage "https://github.com/nasedkinpv/numr"
  version "0.4.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/nasedkinpv/numr/releases/download/v#{version}/numr-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "SKIP_ARM"
    end
    on_intel do
      url "https://github.com/nasedkinpv/numr/releases/download/v#{version}/numr-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "SKIP_INTEL"
    end
  end

  def install
    bin.install "numr"
    bin.install "numr-cli"
  end

  test do
    assert_match "30", shell_output("#{bin}/numr-cli '20% of 150'").strip
  end
end
