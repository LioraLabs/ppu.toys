#!/bin/sh
# Regenerate the Homebrew formula for a release and push the tap.
#
#     scripts/update-tap.sh 0.2.0
#
# The tap (LioraLabs/homebrew-tap) holds generated files only; gen-formula.py
# reads the release's SHA256SUMS, so no checksum is ever hand-copied.
set -eu

TAP="${PPU_TAP:-LioraLabs/homebrew-tap}"

fail() { printf 'tap: %s\n' "$*" >&2; exit 1; }

[ $# -eq 1 ] || fail "usage: update-tap.sh <version>   e.g. 0.2.0"
version="${1#v}"
tag="v$version"
here=$(cd "$(dirname "$0")" && pwd)

command -v gh >/dev/null || fail "gh not on PATH"
gh release view "$tag" --repo LioraLabs/ppu.toys >/dev/null 2>&1 ||
	fail "no $tag release yet; wait for the Release workflow to finish"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT INT TERM

git clone -q "https://github.com/$TAP.git" "$work/tap" || fail "could not clone $TAP"
python3 "$here/gen-formula.py" "$tag" > "$work/ppu.rb" ||
	fail "formula generation failed (is SHA256SUMS attached to $tag?)"
mv "$work/ppu.rb" "$work/tap/Formula/ppu.rb"

if git -C "$work/tap" diff --quiet -- Formula/ppu.rb && git -C "$work/tap" ls-files --error-unmatch Formula/ppu.rb >/dev/null 2>&1; then
	printf 'tap already at %s, nothing to push\n' "$version"
	exit 0
fi

git -C "$work/tap" add Formula/ppu.rb
git -C "$work/tap" commit -q -m "ppu $version"
git -C "$work/tap" push -q origin HEAD

printf 'tap updated to %s\n' "$version"
printf 'verify: brew update && brew reinstall ppu && ppu --version\n'
