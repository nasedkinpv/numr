class Numr < Formula
  desc "Text calculator for natural language expressions with a vim-style TUI"
  homepage "https://github.com/nasedkinpv/numr"
  url "https://github.com/nasedkinpv/numr/archive/v0.1.1.tar.gz"
  sha256 "aab95fd31bdc5aebf140ae642a39f1a643390c35edeb1725a09f0cc84b3f98fc"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "build", "--release", "--locked"
    bin.install "target/release/numr"
    bin.install "target/release/numr-cli"
  end

  test do
    assert_equal "30", shell_output("#{bin}/numr-cli '20% of 150'").strip
  end
end
