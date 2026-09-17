#!/usr/bin/env python3
"""Rewrite every version claim in the tree, then refresh Cargo.lock.

    scripts/bump-version.py 0.2.0

Edits are textual and narrow on purpose: a TOML round-trip would reflow the
manifests. Run by `cook bump`; `cook version-sync` proves it caught everything.
vendor/piccolo (piccolo-ppu) versions on its own and is not touched.
"""

import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SEMVER = re.compile(r"^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$")


def replace_once(path, pattern, repl, label):
    text = path.read_text()
    new, count = re.subn(pattern, repl, text, flags=re.M)
    if count == 0:
        print(f"  !  {label}: no match for {pattern!r}", file=sys.stderr)
        return False
    if new != text:
        path.write_text(new)
        print(f"  ✓  {label}")
    else:
        print(f"  =  {label} already current")
    return True


def main():
    if len(sys.argv) != 2:
        print("usage: bump-version.py <version>   e.g. 0.2.0", file=sys.stderr)
        return 2
    version = sys.argv[1].lstrip("v")
    if not SEMVER.match(version):
        print(f"not a semver version: {version!r}", file=sys.stderr)
        return 2

    print(f"bumping ppu to {version}")
    ok = True
    ok &= replace_once(ROOT / "Cargo.toml", r'^version = "[^"]+"$',
                       f'version = "{version}"', "Cargo.toml [workspace.package]")
    ok &= replace_once(ROOT / "crates/ppu-cli/Cargo.toml",
                       r'^(ppu-core = \{ path = "\.\./ppu-core", version = )"[^"]+"',
                       rf'\1"{version}"', "ppu-cli's ppu-core pin")
    ok &= replace_once(ROOT / "packaging/aur/PKGBUILD", r"^pkgver=.+$",
                       f"pkgver={version}", "PKGBUILD pkgver")
    ok &= replace_once(ROOT / "packaging/aur/PKGBUILD", r"^pkgrel=.+$",
                       "pkgrel=1", "PKGBUILD pkgrel")
    if not ok:
        print("\nsome sites did not match; fix them before releasing", file=sys.stderr)
        return 1

    print("  …  refreshing Cargo.lock")
    result = subprocess.run(["cargo", "check", "--workspace", "--quiet"], cwd=ROOT)
    if result.returncode != 0:
        return result.returncode
    print(f"\nppu is now {version}. Next:")
    print("  cook check")
    print(f"  git commit -am 'v{version}'")
    print(f"  cook release {version}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
