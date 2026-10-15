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
  version "0.12.0-beta.219"
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
    root_url "https://github.com/outlmd/outl/releases/download/v0.12.0-beta.219" # anchor: bottle-root-url
    sha256 cellar: :any_skip_relocation, arm64_ventura: "29664148bb39f6fb240ecc12f78376e3e97c54e1a9212e9a2a7de753f3179d76" # anchor: bottle-macos-arm64
    sha256 cellar: :any_skip_relocation, ventura:       "7a5718f58047160b9d0b8fc58bd91345f5c90c49a5a10fcf182553aeca88b0fa" # anchor: bottle-macos-x64
    sha256 cellar: :any_skip_relocation, x86_64_linux:  "af259201875cb1a1b857ea13611fd41f7fdb7e97e90f70524a4e45ab71301b60" # anchor: bottle-linux-x64
  end

  on_macos do
    on_arm do
      url "https://github.com/outlmd/outl/releases/download/v#{version}/outl-macos-arm64.tar.gz"
      sha256 "e7941e879e57eae164fa53212a9b396b8491d0fbdb5a1821982dad8094f42677" # anchor: macos-arm64
    end
    on_intel do
      url "https://github.com/outlmd/outl/releases/download/v#{version}/outl-macos-x64.tar.gz"
      sha256 "c32e29c9a6d64ffc3115f4574a379d82e1f4c0e8973f916a42b17e3c03da6265" # anchor: macos-x64
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/outlmd/outl/releases/download/v#{version}/outl-linux-x64.tar.gz"
      sha256 "8f04faee29f4e29a25ac0069882ea7d0590208cce137b1ed2e58c6bd106c0693" # anchor: linux-x64
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
