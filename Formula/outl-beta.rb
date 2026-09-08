# This file is maintained by `.github/workflows/release.yml`.
# Every push to `main` runs the release workflow, which bumps the
# `version` line (computed from `Cargo.toml` + the workflow run
# number), the bottle `root_url`, and the six `sha256` lines below in
# place. The `# anchor:` comments are how the workflow finds the right
# lines — do not remove them.
#
# Values committed here are bootstrap placeholders: `version "0.0.0"`
# and zeroed SHAs make `brew install outl-beta` fail loudly until the
# first release fires. They become real on the next push to `main`.
class OutlBeta < Formula
  desc "Local-first outliner with CRDT sync (beta channel — every push to main)"
  homepage "https://outl.app"
  version "0.12.0-beta.183"
  license "MIT"

  # We ship pre-built binaries and compile nothing here, but a formula
  # without a bottle counts as "build from source" to Homebrew, which
  # then runs its fatal dev-tools checks and refuses to install on any
  # Mac whose Xcode is older than the running macOS wants:
  #
  #   Error: Your Xcode (26.6) at /Applications/Xcode.app is too outdated.
  #
  # The bottles below are the same binaries repacked into the Cellar
  # layout (`outl-beta/<version>/bin/outl`), which makes `pour_bottle?`
  # true and skips those checks entirely. macOS bottle tags fall back to
  # older releases, so one `ventura` tag per arch covers every macOS
  # from 13 upwards — no runner on the newest macOS required.
  bottle do
    root_url "https://github.com/outlmd/outl/releases/download/v0.12.0-beta.183" # anchor: bottle-root-url
    sha256 cellar: :any_skip_relocation, arm64_ventura: "4a867f41e0d3118445db289c89dd6c07f37adcfbcda783312638ad4f82a4aec1" # anchor: bottle-macos-arm64
    sha256 cellar: :any_skip_relocation, ventura:       "85b8d7a89f3e0e9bd3a173709b38c6396a5fa1c00aefa60d4fe813f80542df16" # anchor: bottle-macos-x64
    sha256 cellar: :any_skip_relocation, x86_64_linux:  "2ed855ab00635f4d7113663f0ca34c1654d1bd7888994c31095d504b1a8b14fd" # anchor: bottle-linux-x64
  end

  on_macos do
    on_arm do
      url "https://github.com/outlmd/outl/releases/download/v#{version}/outl-macos-arm64.tar.gz"
      sha256 "7bc5019c09a34b0a3df4626ea702f999175188538ea405b21d64445a35935ee9" # anchor: macos-arm64
    end
    on_intel do
      url "https://github.com/outlmd/outl/releases/download/v#{version}/outl-macos-x64.tar.gz"
      sha256 "e8c5232a250c2bdd31110442f4e8047d9b9f0e87c761e4ecce6b3f5b92447035" # anchor: macos-x64
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/outlmd/outl/releases/download/v#{version}/outl-linux-x64.tar.gz"
      sha256 "a56f87a221f16616f230280bf210b99d3c2b0c1553dc6b66a3a8955dca48d772" # anchor: linux-x64
    end
  end

  # Beta and stable share the same `outl` binary name. Refuse to install
  # both side-by-side — `brew unlink outl` (or `outl-beta`) before
  # switching channels.
  conflicts_with "outl", because: "both install the `outl` binary"

  def install
    bin.install "outl"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/outl --version")
  end
end
