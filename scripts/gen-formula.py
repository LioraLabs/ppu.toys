#!/usr/bin/env python3
"""Print the Homebrew formula for a release, from its SHA256SUMS.

    scripts/gen-formula.py v0.2.0 > Formula/ppu.rb

Reads the checksums the release workflow published, so nothing is hand-copied.
"""
import subprocess
import sys

REPO = "LioraLabs/ppu.toys"
TARGETS = {
    "aarch64-apple-darwin": ("on_macos", "on_arm"),
    "x86_64-apple-darwin": ("on_macos", "on_intel"),
    "aarch64-unknown-linux-musl": ("on_linux", "on_arm"),
    "x86_64-unknown-linux-musl": ("on_linux", "on_intel"),
}


def main():
    tag = sys.argv[1]
    version = tag.lstrip("v")
    url = f"https://github.com/{REPO}/releases/download/{tag}/SHA256SUMS"
    sums_text = subprocess.run(["curl", "-fsSL", url], capture_output=True, text=True, check=True).stdout
    sums = {}
    for line in sums_text.splitlines():
        digest, name = line.split()
        sums[name.lstrip("*")] = digest

    blocks = {"on_macos": [], "on_linux": []}
    for target, (os_block, arch_block) in TARGETS.items():
        archive = f"ppu-{tag}-{target}.tar.gz"
        if archive not in sums:
            sys.exit(f"{archive} missing from {url}")
        blocks[os_block].append(
            f"    {arch_block} do\n"
            f"      url \"https://github.com/{REPO}/releases/download/{tag}/{archive}\"\n"
            f"      sha256 \"{sums[archive]}\"\n"
            f"    end"
        )

    print(f'''# Generated from the {tag} SHA256SUMS by scripts/gen-formula.py in LioraLabs/ppu.toys.
class Ppu < Formula
  desc "Author SNES PPU toys for ppu.toys offline: new, check, render, pack, docs"
  homepage "https://ppu.toys"
  version "{version}"
  license "MIT"

  on_macos do
{chr(10).join(blocks["on_macos"])}
  end

  on_linux do
{chr(10).join(blocks["on_linux"])}
  end

  def install
    bin.install "ppu"
  end

  test do
    assert_match "ppu #{{version}}", shell_output("#{{bin}}/ppu --version")
  end
end''')


if __name__ == "__main__":
    main()
