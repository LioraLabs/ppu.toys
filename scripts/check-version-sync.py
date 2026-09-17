#!/usr/bin/env python3
"""Assert every file that claims a ppu version agrees with Cargo.toml.

[workspace.package] version is canonical. The ppu-* members must inherit it,
ppu-cli's pin on ppu-core must equal it, Cargo.lock must carry it, and the AUR
PKGBUILD must name it. Run by `cook version-sync`; gates `cook release`.
"""

import pathlib
import re
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent


def main():
    cargo = tomllib.loads((ROOT / "Cargo.toml").read_text())
    canonical = cargo["workspace"]["package"]["version"]
    problems = []

    for member in cargo["workspace"]["members"]:
        if not member.startswith("crates/ppu-"):
            continue
        pkg = tomllib.loads((ROOT / member / "Cargo.toml").read_text())["package"]
        if pkg.get("version") != {"workspace": True}:
            problems.append(f"{member}/Cargo.toml sets version = {pkg.get('version')!r}; "
                            "use `version.workspace = true`")

    cli = tomllib.loads((ROOT / "crates/ppu-cli/Cargo.toml").read_text())
    pin = cli["dependencies"]["ppu-core"].get("version")
    if pin != canonical:
        problems.append(f"ppu-cli pins ppu-core version = {pin!r}, expected {canonical!r}")

    lock = tomllib.loads((ROOT / "Cargo.lock").read_text())
    for pkg in lock["package"]:
        if pkg.get("source") is None and pkg["name"].startswith("ppu-") and pkg["version"] != canonical:
            problems.append(f"Cargo.lock has {pkg['name']} {pkg['version']}, expected {canonical} "
                            "(run: cargo check --workspace)")

    pkgbuild = (ROOT / "packaging/aur/PKGBUILD").read_text()
    found = re.search(r"^pkgver=(.+)$", pkgbuild, re.M)
    if not found:
        problems.append("packaging/aur/PKGBUILD has no pkgver= line")
    elif found.group(1).strip() != canonical:
        problems.append(f"packaging/aur/PKGBUILD pkgver={found.group(1).strip()}, expected {canonical}")

    if problems:
        print(f"version drift; Cargo.toml says {canonical}:", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        print(f"\nrun: cook bump {canonical}", file=sys.stderr)
        return 1
    print(f"version {canonical} consistent across every claim site")
    return 0


if __name__ == "__main__":
    sys.exit(main())
