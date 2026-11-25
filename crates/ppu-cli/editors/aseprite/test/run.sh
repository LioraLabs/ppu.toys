#!/bin/sh
# Run the batch test with a ppu built from this checkout: sh test/run.sh [aseprite]
# Aseprite runs on a throwaway user folder, never ~/.config/aseprite.
set -e
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../../../.." && pwd)
( cd "$repo" && cargo build -p ppu-cli )
user=$(mktemp -d)
trap 'rm -rf "$user"' EXIT
ASEPRITE_USER_FOLDER="$user" PPU_BIN="$repo/target/debug/ppu" "${1:-aseprite}" -b --script "$here/snes-art-test.lua"
