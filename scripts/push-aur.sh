#!/bin/sh
# Push packaging/aur/PKGBUILD to the AUR as ppu-cli-bin.
#
# Needs an AUR account with this machine's SSH key registered, and makepkg
# (Arch) to generate .SRCINFO. The first push creates the package.
set -eu

PKG=ppu-cli-bin
root=$(cd "$(dirname "$0")/.." && pwd)
pkgbuild="$root/packaging/aur/PKGBUILD"

fail() { printf 'aur: %s\n' "$*" >&2; exit 1; }

command -v makepkg >/dev/null || fail "makepkg not on PATH; .SRCINFO needs Arch tooling"
! grep -q "'SKIP'" "$pkgbuild" || fail "PKGBUILD still has SKIP checksums; run update-aur.py first"

ver=$(sed -n 's/^pkgver=//p' "$pkgbuild")
rel=$(sed -n 's/^pkgrel=//p' "$pkgbuild")

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT INT TERM
git clone -q "ssh://aur@aur.archlinux.org/$PKG.git" "$work" ||
	fail "could not clone $PKG from the AUR (is your SSH key registered at aur.archlinux.org?)"

cp "$pkgbuild" "$work/PKGBUILD"
(cd "$work" && makepkg --printsrcinfo > .SRCINFO)
git -C "$work" add PKGBUILD .SRCINFO
if git -C "$work" diff --cached --quiet; then
	printf 'AUR already at %s-%s, nothing to push\n' "$ver" "$rel"
	exit 0
fi
git -C "$work" commit -q -m "upgpkg: $PKG $ver-$rel"
git -C "$work" push -q origin HEAD:master

printf 'AUR updated: https://aur.archlinux.org/packages/%s\n' "$PKG"
printf 'verify: yay -S %s && ppu --version\n' "$PKG"
