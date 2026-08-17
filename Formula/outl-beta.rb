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
  version "0.12.0-beta.163"
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
    root_url "https://github.com/outlmd/outl/releases/download/v0.12.0-beta.163" # anchor: bottle-root-url
    sha256 cellar: :any_skip_relocation, arm64_ventura: "7012544f8fd8bfd692312e6348fdf061c02e4be24fda7c5ab534eeacd7bbf630" # anchor: bottle-macos-arm64
    sha256 cellar: :any_skip_relocation, ventura:       "79246a724968c2451b71e646690ba1d1e2232cd62dce0396c7e64915121be3e3" # anchor: bottle-macos-x64
    sha256 cellar: :any_skip_relocation, x86_64_linux:  "96cd6244723288610d7b0d2c83fd5e88418ca927accc34df40bb89330fa4fe0e" # anchor: bottle-linux-x64
  end

  on_macos do
    on_arm do
      url "https://github.com/outlmd/outl/releases/download/v#{version}/outl-macos-arm64.tar.gz"
      sha256 "84cbf34b0aac017f00a564afa69bab32d827f0dcf6b7e45cd395c46489fb6235" # anchor: macos-arm64
    end
    on_intel do
      url "https://github.com/outlmd/outl/releases/download/v#{version}/outl-macos-x64.tar.gz"
      sha256 "cf77d9d5d48a6993d243f890c697ede7ffd905e96fad461f94b8ee7794c986a7" # anchor: macos-x64
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/outlmd/outl/releases/download/v#{version}/outl-linux-x64.tar.gz"
      sha256 "06cfb680546d8c73e654badadf7772d95d7f3b64d7c42a9d171ea1daf8e0785d" # anchor: linux-x64
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
