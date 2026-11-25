"""File-level assertions for a run: what the extension wrote must match the pristine fixtures
except exactly the pixels the autotest edited. Merges them with the in-editor assertions.
Usage: verify.py <run dir>. Exits 1 when anything failed."""
import json
import os
import shutil
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pngio import read_png

R = sys.argv[1]
res_path = f"{R}/out/results.json"
if not os.path.exists(res_path):
    print(f"FAIL: no results.json in {R}/out (Pixelorama did not finish; see {R}/pixelorama.log)")
    sys.exit(1)
res = json.load(open(res_path))
checks = [(a["name"], a["ok"], a["detail"]) for a in res["assertions"]]


def check(name, ok, detail=""):
    checks.append((name, bool(ok), str(detail)))


def diff(a, b):
    return {i for i in range(len(a)) if a[i] != b[i]}


def compare(art, saved_path, edits, label):
    ref = read_png(f"{R}/ref/art/{art}.png")
    out = read_png(saved_path)
    check(f"{label}: 8-bit indexed PNG", (out["colortype"], out["depth"]) == (3, 8), (out["colortype"], out["depth"]))
    check(f"{label}: same size", (out["w"], out["h"]) == (ref["w"], ref["h"]))
    if out["idx"] is None:
        return
    w = ref["w"]
    changed = diff(ref["idx"], out["idx"])
    want = {y * w + x for x, y, v in edits if ref["idx"][y * w + x] != v}
    check(f"{label}: exactly the edited pixels differ", changed == want, f"{len(changed)} differ, {len(want)} expected")
    check(f"{label}: edited pixels hold the new index", all(out["idx"][y * w + x] == v for x, y, v in edits))
    check(f"{label}: tRNS = entry 0 of each row", out["trns"] == ref["trns"])
    return ref, out


for art, kind in (("bg", "bg"), ("track", "m7")):
    ref = read_png(f"{R}/ref/art/{art}.png")
    noedit = read_png(f"{R}/out/{art}-noedit.png")
    check(f"{art}: open + save, no edit = identical pixels", noedit["idx"] == ref["idx"])
    check(f"{art}: open + save, no edit = identical PLTE and tRNS", noedit["plte"] == ref["plte"] and noedit["trns"] == ref["trns"])
    edits = res["edits"].get(art, [])
    compare(art, f"{R}/out/{art}-edited.png", edits, f"{art} edited")
    if art == "bg":
        got = compare(art, f"{R}/proj/art/bg.png", edits, "bg Ctrl+S (the opened file)")
        if got:
            _, out = got
            check("bg: index 1 and 17 distinct in the saved file", out["idx"][1] == 1 and out["idx"][9] == 17, (out["idx"][1], out["idx"][9]))
        exp = read_png(f"{R}/out/bg-export.png")
        check("bg: SNES indexed PNG export = the saved PNG", exp["idx"] == read_png(f"{R}/out/bg-edited.png")["idx"] and exp["plte"] == read_png(f"{R}/out/bg-palette.png")["plte"])
        pal = read_png(f"{R}/out/bg-palette.png")
        check("bg: a put-back palette edit does not reach the saved PLTE", pal["plte"] == ref["plte"] and pal["trns"] == ref["trns"], pal["plte"][3])
        shutil.copytree(f"{R}/ref", f"{R}/importable")
        shutil.copy(f"{R}/out/bg-palette.png", f"{R}/importable/art/bg.png")
        ppu = subprocess.run([os.environ.get("PPU_BIN", "ppu"), "art", "check", f"{R}/importable/art/bg.png", "--json"], capture_output=True, text=True)
        rules = {f.get("rule") for f in json.loads(ppu.stdout).get("findings", [])} if ppu.stdout.strip() else {"no output: " + ppu.stderr}
        check("bg: the PNG saved after a palette edit passes the plte rule of ppu art check", "plte" not in rules and not any(str(r).startswith("no output") for r in rules), rules)
        check("bg: pixels untouched by a palette edit", pal["idx"] == read_png(f"{R}/out/bg-edited.png")["idx"])
    else:
        check("track: index 255 present in the saved file", 255 in noedit["idx"])
        check("track: index 0 present in the saved file", noedit["idx"][-1] == 0 and noedit["idx"][0] == 255)
        check("track: PLTE keeps all 256 entries", len(noedit["plte"]) == 256)

def gpl_colours(path):
    out = []
    for line in open(path).read().splitlines():
        f = line.split()
        if len(f) >= 3 and all(t.isdigit() for t in f[:3]):
            out.append(tuple(int(t) for t in f[:3]))
    return out


# The palette is locked (autotest _palette_lock): no palette change in the editor wrote a .gpl
for name in sorted(os.listdir(f"{R}/ref/palettes")):
    check(f"locked palette: {name} is byte-identical to the fixture", open(f"{R}/proj/palettes/{name}", "rb").read() == open(f"{R}/ref/palettes/{name}", "rb").read())


def ppu_check(png):
    out = subprocess.run([os.environ.get("PPU_BIN", "ppu"), "art", "check", png, "--json"], capture_output=True, text=True)
    return json.loads(out.stdout) if out.stdout.strip() else {"error": out.stderr}


# Set tile row, Fix tile rows (autotest _rows): the saved PNGs on disk, read without the editor's help
rw = res.get("rows")
check("the row tests ran", rw is not None)
if rw:
    x, y, w, h = rw["rect"]
    ref = read_png(f"{R}/ref/rows/art/bg.png")
    out = read_png(f"{R}/proj/rows/art/bg.png")
    W = ref["w"]
    inside = [out["idx"][j * W + i] for j in range(y, y + h) for i in range(x, x + w)]
    check("set row: every pixel of the touched tiles is on the row, on disk", all(v // 16 == rw["row"] for v in inside))
    rest = {j * W + i for j in range(len(ref["idx"]) // W) for i in range(W) if not (x <= i < x + w and y <= j < y + h)}
    check("set row: pixels outside are byte-identical on disk", all(out["idx"][k] == ref["idx"][k] for k in rest))
    check("set row: PLTE and tRNS unchanged", out["plte"] == ref["plte"] and out["trns"] == ref["trns"])
    fx_out = read_png(f"{R}/proj/rows/art/rowsfix.png")
    fx_ref = read_png(f"{R}/ref/rows/art/rowsfix.png")
    check("fix: the stray pixel moved, the leftover stayed, nothing else changed on disk",
          [k for k in range(len(fx_ref["idx"])) if fx_ref["idx"][k] != fx_out["idx"][k]] == [3 * 32 + 11] and fx_out["idx"][3 * 32 + 11] == 17)
    chk = ppu_check(f"{R}/proj/rows/art/rowsfix.png")
    mixed = [(f["x"], f["y"]) for f in chk.get("findings", []) if f["rule"] == "mixed-rows"]
    check("fix: the saved PNG has no mixed-rows finding but the leftover tile", mixed == [(2, 0)], mixed)

# New SNES art (autotest _new_art): the files ppu wrote, read without the editor's help
na = res.get("new_art")
check("the new art tests ran", na is not None)
if na:
    fresh = read_png(na["png"])
    gpl = gpl_colours(na["gpl"])
    check("new art: PNG, .gpl and the ppu.json entry exist and agree", fresh["plte"] == gpl and set(fresh["idx"]) == {0}, len(gpl))
    src = [s for s in json.load(open(na["json"]))["sources"] if s["name"] == "fresh"]
    check("new art: ppu.json lists the art with its palette", len(src) == 1 and src[0]["options"]["palette"] == "fresh_pal", src)
    check("new art: ppu art check is clean", ppu_check(na["png"]).get("ok") is True)
    check("new art: the refused runs left no other file", not os.path.exists(f"{R}/proj/rows/art/odd.png") and not os.path.exists(f"{R}/proj/rows/palettes/again_pal.gpl"))

# Convert to SNES art (autotest _convert): the files ppu wrote
cv = res.get("convert")
check("the convert tests ran", cv is not None)
if cv:
    check("convert: ppu art check is clean on the converted PNG", ppu_check(cv["png"]).get("ok") is True)
    backup = f"{R}/proj/rows/.ppu-backup/convert-1/art/full.png"
    check("convert: .ppu-backup/convert-1 holds the original", os.path.exists(backup) and open(backup, "rb").read() == open(f"{R}/ref/rows/art/full.png", "rb").read())
    src = [s for s in json.load(open(cv["json"]))["sources"] if s["name"] == "full"]
    check("convert: the .gpl exists and ppu.json lists it", os.path.exists(cv["gpl"]) and len(src) == 1 and src[0]["options"]["palette"] == "full_pal", src)
    # the refused runs (clash, another file) made no backup: the third and last one is the two-layer tab
    check("convert: the refused runs wrote no clash files", os.path.exists(f"{R}/proj/rows/.ppu-backup/convert-3") and not os.path.exists(f"{R}/proj/rows/.ppu-backup/convert-4"))

leftovers = [os.path.join(d, f) for d, _, fs in os.walk(f"{R}/proj") for f in fs if f.endswith((".snesart-tmp", ".pxo"))]
check("no temp or .pxo files left in the project", not leftovers, leftovers)

shots = sorted(os.listdir(f"{R}/shots"))
check("screenshots saved", len(shots) >= 3, shots)

failed = [c for c in checks if not c[1]]
for n, ok, d in checks:
    if not ok:
        print(f"FAIL {n}: {d}")
print(f"{len(checks) - len(failed)}/{len(checks)} assertions passed")
print("metrics:", json.dumps(res["metrics"], indent=1))
print("screenshots:", *[f"{R}/shots/{s}" for s in shots], sep="\n  ")
json.dump({"assertions": [{"name": n, "ok": ok, "detail": d} for n, ok, d in checks], "metrics": res["metrics"]},
          open(f"{R}/results-final.json", "w"), indent=1)
sys.exit(1 if failed else 0)
