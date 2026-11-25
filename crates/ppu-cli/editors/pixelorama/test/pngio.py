"""Indexed-PNG and .gpl helpers for the runner (stdlib only)."""
import struct
import zlib


def _chunk(t, d):
    return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)


def write_png(path, w, h, idx, plte, trns):
    """idx: flat list/bytes of w*h indices; plte: list of (r,g,b); trns: bytes."""
    raw = b"".join(b"\0" + bytes(idx[y * w:(y + 1) * w]) for y in range(h))
    out = b"\x89PNG\r\n\x1a\n" + _chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 3, 0, 0, 0))
    out += _chunk(b"PLTE", bytes(c for rgb in plte for c in rgb))
    out += _chunk(b"tRNS", bytes(trns)) + _chunk(b"IDAT", zlib.compress(raw, 6)) + _chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(out)


def write_rgba_png(path, w, h, rgba):
    raw = b"".join(b"\0" + bytes(rgba[y * w * 4:(y + 1) * w * 4]) for y in range(h))
    out = b"\x89PNG\r\n\x1a\n" + _chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
    out += _chunk(b"IDAT", zlib.compress(raw)) + _chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(out)


def read_png(path):
    """-> dict(w, h, colortype, depth, idx (bytes) or None, plte [(r,g,b)], trns bytes)."""
    b = open(path, "rb").read()
    assert b[:8] == b"\x89PNG\r\n\x1a\n", "not a png"
    p, idat, plte, trns = 8, b"", [], b""
    w = h = ct = depth = 0
    while p < len(b):
        n, = struct.unpack(">I", b[p:p + 4])
        t, d = b[p + 4:p + 8], b[p + 8:p + 8 + n]
        p += 12 + n
        if t == b"IHDR":
            w, h, depth, ct = struct.unpack(">IIBB", d[:10])
        elif t == b"PLTE":
            plte = [tuple(d[i:i + 3]) for i in range(0, n, 3)]
        elif t == b"tRNS":
            trns = d
        elif t == b"IDAT":
            idat += d
    idx = None
    if ct == 3 and depth == 8:
        raw = zlib.decompress(idat)
        rows, prev = [], bytearray(w)
        for y in range(h):
            f, line = raw[y * (w + 1)], bytearray(raw[y * (w + 1) + 1:(y + 1) * (w + 1)])
            if f == 2:
                line = bytearray((line[x] + prev[x]) & 255 for x in range(w))
            elif f:
                for x in range(w):
                    a = line[x - 1] if x else 0
                    c = prev[x - 1] if x else 0
                    if f == 1:
                        pr = a
                    elif f == 3:
                        pr = (a + prev[x]) // 2
                    else:
                        pa, pb, pc = abs(prev[x] - c), abs(a - c), abs(a + prev[x] - 2 * c)
                        pr = a if pa <= pb and pa <= pc else (prev[x] if pb <= pc else c)
                    line[x] = (line[x] + pr) & 255
            rows.append(bytes(line))
            prev = line
        idx = b"".join(rows)
    return {"w": w, "h": h, "colortype": ct, "depth": depth, "idx": idx, "plte": plte, "trns": trns}


def write_gpl(path, name, colors, columns):
    lines = ["GIMP Palette", f"Name: {name}", f"Columns: {columns}", "#"]
    lines += [f"{r:3d} {g:3d} {b:3d}\tc{i}" for i, (r, g, b) in enumerate(colors)]
    with open(path, "w") as f:
        f.write("\n".join(lines) + "\n")
