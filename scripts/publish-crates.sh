#!/bin/sh
# Publish the workspace to crates.io, bottom-up: piccolo-ppu, ppu-core, ppu-cli.
#
# `cargo publish` blocks until each crate appears in the index, so the next one
# resolves. Already-published versions are skipped, so a partial run is safe to
# re-run.
set -eu

ORDER="piccolo-ppu ppu-core ppu-cli"

fail() { printf 'publish: %s\n' "$*" >&2; exit 1; }

command -v cargo >/dev/null || fail "cargo not on PATH"
[ -n "${CARGO_REGISTRY_TOKEN:-}" ] || [ -f "$HOME/.cargo/credentials.toml" ] || [ -f "$HOME/.cargo/credentials" ] ||
	fail "no crates.io token found. Mint one at https://crates.io/settings/tokens (publish-new, publish-update), then: cargo login"

metadata=$(cargo metadata --format-version 1 --no-deps)

for crate in $ORDER; do
	version=$(printf '%s' "$metadata" |
		python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"]==sys.argv[1]))' "$crate")
	# crates.io 403s requests without a real User-Agent, so every probe sets one.
	code=$(curl -s -o /dev/null -w '%{http_code}' -A "ppu-release/$version" \
		"https://crates.io/api/v1/crates/$crate/$version")
	case "$code" in
		200) printf '  =  %s %s already published\n' "$crate" "$version"; continue ;;
		404) ;;
		*) fail "crates.io returned $code for $crate/$version; refusing to guess" ;;
	esac
	printf '  →  %s %s\n' "$crate" "$version"
	cargo publish -p "$crate"
done

printf '\nall crates are on crates.io; cargo install ppu-cli now works.\n'
