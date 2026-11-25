#!/usr/bin/env bash
# Headless end-to-end run: Pixelorama + the SnesArt extension inside its own sway (never the
# live desktop) on fixture projects. Exits non-zero when any assertion fails.
#   PIXELORAMA  the Pixelorama 1.2.4 binary (default: pixelorama on PATH)
#   PPU_BIN     the ppu binary (default: ppu/target/release/ppu, else ppu on PATH)
#   SNESART_OUT the run directory (default: a fresh temp dir); it is printed at the end
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
R=${SNESART_OUT:-$(mktemp -d "${TMPDIR:-/tmp}/snesart-test.XXXXXX")}
mkdir -p "$R"/{home/data/pixelorama/extensions,home/config,home/cache,out,shots}
RT=$(mktemp -d /tmp/snesart-rt.XXXXXX)  # short path: unix socket paths are capped at ~100 bytes
PIXELORAMA=${PIXELORAMA:-pixelorama}
PPU_BIN=${PPU_BIN:-$HERE/../../../../../target/release/ppu}
[ -x "$PPU_BIN" ] || PPU_BIN=$(command -v ppu)

python3 "$HERE/make_fixtures.py" "$R/proj" >"$R/fixtures.json"
rm -rf "$R/ref" && cp -r "$R/proj" "$R/ref"   # pristine copy the verifier compares against

# The extension zip is the one `ppu art install pixelorama` ships, plus the autotest (loaded only under PPU_ART_TEST).
"$PPU_BIN" art install pixelorama --dir "$R/home/data/pixelorama" >/dev/null
python3 - "$HERE/autotest.gd" "$R/home/data/pixelorama/extensions/SnesArt.zip" <<'PY'
import sys, zipfile
with zipfile.ZipFile(sys.argv[2], "a") as z:
    z.write(sys.argv[1], "src/Extensions/SnesArt/autotest.gd")
PY
printf '[extensions]\nSnesArt=true\n[preferences]\nstartup=false\n' > "$R/home/data/pixelorama/config.ini"
printf 'output HEADLESS-1 resolution 1600x1000\n' > "$R/sway.conf"

export XDG_RUNTIME_DIR=$RT
env -u DISPLAY -u WAYLAND_DISPLAY -u SWAYSOCK WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 \
  sway -c "$R/sway.conf" --unsupported-gpu >"$R/sway.log" 2>&1 &
SWAY=$!
trap 'kill $SWAY 2>/dev/null || true; rm -rf "$RT"' EXIT
for _ in $(seq 100); do ls "$RT" | grep -qE "^wayland-[0-9]+$" && break; sleep 0.1; done
WD=$(cd "$RT" && ls | grep -E '^wayland-[0-9]+$' | head -1)

echo "run dir $R (sway $WD)"
set +e
env -u DISPLAY WAYLAND_DISPLAY=$WD XDG_DATA_HOME=$R/home/data XDG_CONFIG_HOME=$R/home/config XDG_CACHE_HOME=$R/home/cache \
  PPU_ART_TEST=1 PPU_ART_OUT=$R/out PPU_ART_PROJ=$R/proj PPU_BIN=$PPU_BIN \
  timeout "${SNESART_TIMEOUT:-300}" "$PIXELORAMA" --display-driver wayland >"$R/pixelorama.log" 2>&1
echo "pixelorama exit $?"
set -e
PPU_BIN=$PPU_BIN python3 "$HERE/verify.py" "$R"
