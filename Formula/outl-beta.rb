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
  version "0.12.0-beta.215"
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
    root_url "https://github.com/outlmd/outl/releases/download/v0.12.0-beta.215" # anchor: bottle-root-url
    sha256 cellar: :any_skip_relocation, arm64_ventura: "f3279af1d49acdf9cc711808c45ce409ef232acc258ae6b521e13b7d3d309b45" # anchor: bottle-macos-arm64
    sha256 cellar: :any_skip_relocation, ventura:       "88ef9393bacfa232e820ea2e49e027eac3750d39f351c4a738019d4be3723eff" # anchor: bottle-macos-x64
    sha256 cellar: :any_skip_relocation, x86_64_linux:  "ddce4c477f9d0c726318788564f2df43ebc93772b236a5ba0d6c49989c76f6e0" # anchor: bottle-linux-x64
  end

  on_macos do
    on_arm do
      url "https://github.com/outlmd/outl/releases/download/v#{version}/outl-macos-arm64.tar.gz"
      sha256 "3f24d7f3c17693137707754ff4b704f907a9c0ec8795004fb43114fd82c65ae4" # anchor: macos-arm64
    end
    on_intel do
      url "https://github.com/outlmd/outl/releases/download/v#{version}/outl-macos-x64.tar.gz"
      sha256 "067f0492cb3f7568fc7de8040f0fbaf98b608b648d4571ca2b78e2e8793ff148" # anchor: macos-x64
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/outlmd/outl/releases/download/v#{version}/outl-linux-x64.tar.gz"
      sha256 "fe67347dfdb2625fbcd2df2f8479d3b7a7c058c9698f4a6dbbc7dfd6f93e3b0f" # anchor: linux-x64
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
