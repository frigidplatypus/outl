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
  version "0.12.0-beta.167"
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
    root_url "https://github.com/outlmd/outl/releases/download/v0.12.0-beta.167" # anchor: bottle-root-url
    sha256 cellar: :any_skip_relocation, arm64_ventura: "6e6e414d0d2c5bb5dd0106cd70f1b8b6a985e2ef057684c8f70d7c729c7f1a0b" # anchor: bottle-macos-arm64
    sha256 cellar: :any_skip_relocation, ventura:       "f08406f289ac2aea383d546b923c8155bbc20eef063c4698ee6dc7f6f46d68b4" # anchor: bottle-macos-x64
    sha256 cellar: :any_skip_relocation, x86_64_linux:  "44214087d205c0324e93cb15a888d8513abdf3cb7c48d628a24867af95ffb7f0" # anchor: bottle-linux-x64
  end

  on_macos do
    on_arm do
      url "https://github.com/outlmd/outl/releases/download/v#{version}/outl-macos-arm64.tar.gz"
      sha256 "cb54267f06bd1eb6c7b51f05804888cb75a18893927d841bda7f0791e69951b1" # anchor: macos-arm64
    end
    on_intel do
      url "https://github.com/outlmd/outl/releases/download/v#{version}/outl-macos-x64.tar.gz"
      sha256 "1b88355ea43146964b1366cd8407264bc6c02023d1b0de88e679f08a1c457f55" # anchor: macos-x64
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/outlmd/outl/releases/download/v#{version}/outl-linux-x64.tar.gz"
      sha256 "995278e184e27837f3d9b975a9acd42ba10f6b0bf45da988baede3a76c938449" # anchor: linux-x64
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
