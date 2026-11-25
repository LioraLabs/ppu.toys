extends RefCounted
## 8-bit indexed PNG IO. Pixelorama's own PNG open and save go through RGBA and lose
## indices, so SNES art never touches them.

static var _crc := PackedInt64Array()


static func crc32(data: PackedByteArray) -> int:
	if _crc.is_empty():
		_crc.resize(256)
		for n in 256:
			var c := n
			for k in 8:
				c = (0xEDB88320 ^ (c >> 1)) if (c & 1) else (c >> 1)
			_crc[n] = c
	var c := 0xFFFFFFFF
	for byte in data:
		c = _crc[(c ^ byte) & 0xFF] ^ (c >> 8)
	return c ^ 0xFFFFFFFF


static func _u32(b: PackedByteArray, p: int) -> int:
	return (b[p] << 24) | (b[p + 1] << 16) | (b[p + 2] << 8) | b[p + 3]


static func _be32(v: int) -> PackedByteArray:
	return PackedByteArray([(v >> 24) & 255, (v >> 16) & 255, (v >> 8) & 255, v & 255])


static func _chunk(t: String, d: PackedByteArray) -> PackedByteArray:
	var td := t.to_ascii_buffer() + d
	return _be32(d.size()) + td + _be32(crc32(td))


## -> {w, h, idx: PackedByteArray, plte: PackedColorArray, error: String}. A file that is not
## a 8-bit non-interlaced indexed PNG comes back with `error` set and `indexed` false when it is
## simply another PNG flavour (the caller opens those the normal way).
static func read_png(path: String) -> Dictionary:
	var b := FileAccess.get_file_as_bytes(path)
	if b.size() < 33 or b.slice(0, 8) != PackedByteArray([137, 80, 78, 71, 13, 10, 26, 10]):
		return {"error": "not a PNG file", "indexed": false}
	var w := 0
	var h := 0
	var idat := PackedByteArray()
	var plte := PackedColorArray()
	var p := 8
	while p + 12 <= b.size():
		var n := _u32(b, p)
		if p + 12 + n > b.size():
			return {"error": "truncated PNG", "indexed": true}
		var t := b.slice(p + 4, p + 8).get_string_from_ascii()
		var d := b.slice(p + 8, p + 8 + n)
		p += 12 + n
		match t:
			"IHDR":
				w = _u32(d, 0)
				h = _u32(d, 4)
				if d[8] != 8 or d[9] != 3:
					return {"error": "not 8-bit indexed (depth %d, colour type %d)" % [d[8], d[9]], "indexed": false}
				if d[12] != 0:
					return {"error": "interlaced PNGs are not supported", "indexed": true}
			"PLTE":
				for i in range(0, d.size() - 2, 3):
					plte.append(Color8(d[i], d[i + 1], d[i + 2]))
			"IDAT":
				idat.append_array(d)
	if w == 0 or h == 0:
		return {"error": "PNG has no IHDR", "indexed": false}
	var raw := idat.decompress((w + 1) * h, FileAccess.COMPRESSION_DEFLATE)
	if raw.size() != (w + 1) * h:
		return {"error": "PNG pixel data is damaged", "indexed": true}
	var idx := PackedByteArray()
	var prev := PackedByteArray()
	prev.resize(w)
	for y in h:
		var f := raw[y * (w + 1)]
		var line := raw.slice(y * (w + 1) + 1, (y + 1) * (w + 1))
		if f == 2:
			for x in w:
				line[x] = (line[x] + prev[x]) & 255
		elif f == 1 or f == 3 or f == 4:
			for x in w:
				var a := line[x - 1] if x > 0 else 0
				var up := prev[x]
				var ul := prev[x - 1] if x > 0 else 0
				var pr := a
				if f == 3:
					pr = (a + up) >> 1
				elif f == 4:
					var pa := absi(up - ul)
					var pb := absi(a - ul)
					var pc := absi(a + up - 2 * ul)
					pr = a if (pa <= pb and pa <= pc) else (up if pb <= pc else ul)
				line[x] = (line[x] + pr) & 255
		elif f != 0:
			return {"error": "PNG pixel data is damaged", "indexed": true}
		idx.append_array(line)
		prev = line
	return {"w": w, "h": h, "idx": idx, "plte": plte, "error": "", "indexed": true}


static func plte_bytes(colors: PackedColorArray) -> PackedByteArray:
	var out := PackedByteArray()
	for c in colors:
		out.append_array(PackedByteArray([c.r8, c.g8, c.b8]))
	return out


## tRNS: alpha 0 on the entries `tmask` (256 flags) marks transparent, opaque elsewhere.
static func trns_bytes(n: int, tmask: PackedByteArray) -> PackedByteArray:
	var t := PackedByteArray()
	for i in n:
		t.append(0 if i < 256 and tmask[i] == 1 else 255)
	return t


## Writes an 8-bit indexed PNG to a temp file and renames it over `path`, so a failed write
## never leaves half a PNG. Returns "" or the error text.
static func write_png(path: String, w: int, h: int, idx: PackedByteArray, colors: PackedColorArray, tmask: PackedByteArray) -> String:
	if idx.size() != w * h:
		return "pixel data does not match %dx%d" % [w, h]
	var raw := PackedByteArray()
	for y in h:
		raw.append(0)
		raw.append_array(idx.slice(y * w, (y + 1) * w))
	var out := PackedByteArray([137, 80, 78, 71, 13, 10, 26, 10])
	out += _chunk("IHDR", _be32(w) + _be32(h) + PackedByteArray([8, 3, 0, 0, 0]))
	out += _chunk("PLTE", plte_bytes(colors)) + _chunk("tRNS", trns_bytes(colors.size(), tmask))
	out += _chunk("IDAT", raw.compress(FileAccess.COMPRESSION_DEFLATE)) + _chunk("IEND", PackedByteArray())
	var tmp := path + ".snesart-tmp"
	var f := FileAccess.open(tmp, FileAccess.WRITE)
	if f == null:
		return "cannot write %s (%s)" % [tmp, error_string(FileAccess.get_open_error())]
	f.store_buffer(out)
	var write_err := f.get_error()
	f.close()
	if write_err != OK:
		DirAccess.remove_absolute(tmp)
		return "cannot write %s (%s)" % [tmp, error_string(write_err)]
	var err := DirAccess.rename_absolute(tmp, path)
	if err != OK:
		DirAccess.remove_absolute(tmp)
		return "cannot replace %s (%s)" % [path, error_string(err)]
	return ""
