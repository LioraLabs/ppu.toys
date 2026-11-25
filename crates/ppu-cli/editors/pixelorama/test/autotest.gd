extends Node
## Scripted run inside Pixelorama (loaded only under PPU_ART_TEST). Drives the editor like a
## user would, records assertions and measurements in results.json, saves files for verify.py.

var m: Node
var out_dir := ""
var proj := ""
var res := {"assertions": [], "metrics": {}, "edits": {}}


func _ok(name: String, cond: bool, detail: Variant = "") -> void:
	res["assertions"].append({"name": name, "ok": cond, "detail": str(detail)})
	print("[%s] %s %s" % ["pass" if cond else "FAIL", name, detail if not cond else ""])


func _shot(name: String) -> void:
	await RenderingServer.frame_post_draw
	get_viewport().get_texture().get_image().save_png(out_dir.path_join("../shots/%s.png" % name))


func _frames(n: int) -> void:
	for i in n:
		await get_tree().process_frame


func _stroke(points: Array, swatch: int) -> void:
	Tools.assign_tool("Pencil", MOUSE_BUTTON_LEFT)
	Palettes.current_palette_select_color(MOUSE_BUTTON_LEFT, swatch)
	var down := InputEventMouseButton.new()
	down.button_index = MOUSE_BUTTON_LEFT
	down.pressed = true
	Tools.handle_draw(points[0], down)
	for p in points.slice(1):
		var mv := InputEventMouseMotion.new()
		mv.pressure = 1.0
		Tools.handle_draw(p, mv)
	var up := InputEventMouseButton.new()
	up.button_index = MOUSE_BUTTON_LEFT
	up.pressed = false
	Tools.handle_draw(points[-1], up)


## The next finished check, or {"timeout": true} after 5 s.
func _wait_check() -> Dictionary:
	var box := [{}]
	var cb := func(r): box[0] = r
	m.check_finished.connect(cb, CONNECT_ONE_SHOT)
	var t := Time.get_ticks_msec()
	while box[0].is_empty() and Time.get_ticks_msec() - t < 5000:
		await get_tree().process_frame
	if box[0].is_empty():
		m.check_finished.disconnect(cb)
		return {"timeout": true}
	return box[0]


func _px(p: Project, x: int, y: int) -> int:
	return m.canvas_indices(m.snapshot(p))[y * p.size.x + x]


func _ctrl_s() -> void:
	var ev := InputEventKey.new()
	ev.keycode = KEY_S
	ev.physical_keycode = KEY_S
	ev.ctrl_pressed = true
	ev.pressed = true
	Input.parse_input_event(ev)
	await _frames(3)


func _read_json(path: String) -> Variant:
	return JSON.parse_string(FileAccess.get_file_as_string(path))


func _med(a: Array) -> float:
	var b := a.duplicate()
	b.sort()
	return b[b.size() / 2]


func run(main: Node) -> void:
	m = main
	out_dir = OS.get_environment("PPU_ART_OUT")
	proj = OS.get_environment("PPU_ART_PROJ")
	await _frames(30)
	var fx = _read_json(proj.path_join("../fixtures.json"))
	res["pixelorama"] = m.api.general.get_pixelorama_version()
	res["godot"] = Engine.get_version_info()["string"]
	await _version_checks()
	await _open_plain()
	await _bg()
	await _extbg_open()
	await _rows()
	await _carried_minors()
	await _new_art()
	await _new_art_dialog()
	await _convert()
	await _m7()
	await _remaps()
	await _transparency()
	await _latency()
	await _switching()
	_extbg_units()
	await _failures()
	var f := FileAccess.open(out_dir.path_join("results.json"), FileAccess.WRITE)
	f.store_string(JSON.stringify(res, "  "))
	f.close()
	print("AUTOTEST DONE")
	get_tree().quit()


func _version_checks() -> void:
	_ok("old Pixelorama refused with the message", m.refusal_for("1.2.3") == "SNES art needs Pixelorama 1.2.4 or newer (this is 1.2.3).", m.refusal_for("1.2.3"))
	_ok("1.1.9 refused", m.refusal_for("1.1.9") != "")
	_ok("1.2.4 accepted", m.refusal_for("1.2.4") == "")
	_ok("1.3.0-dev accepted", m.refusal_for("1.3.0-dev") == "")
	_ok("2.0.0 accepted", m.refusal_for("2.0.0") == "")
	_ok("this Pixelorama is not refused", m.refused == "", m.refused)
	var has_item := false
	var file_menu: PopupMenu = Global.top_menu_container.file_menu
	for i in file_menu.item_count:
		has_item = has_item or file_menu.get_item_text(i) == "Save SNES art"
	_ok("File > Save SNES art exists", has_item)
	_ok("export format SNES indexed PNG registered", Export.custom_file_formats.has("SNES_INDEXED_PNG"), Export.custom_file_formats.keys())


func _open_plain() -> void:
	var before := Global.projects.size()
	OpenSave.handle_loading_file(proj.path_join("unlisted/indexed.png"))
	await _frames(5)
	var p: Project = Global.current_project
	_ok("unlisted indexed PNG opens the normal way", not m.snes.has(p) and p.size == Vector2i(16, 16) and not p.is_indexed(), "%s %s" % [p.size, p.is_indexed()])
	_ok("normal open adds no SNES state", m.snes.is_empty())
	var spawns: int = m.ui_spawns
	var err0: String = m.last_error
	var outside := out_dir.path_join("outside.png")
	DirAccess.copy_absolute(proj.path_join("unlisted/rgba.png"), outside)
	var plain: int = m.plain_opens
	OpenSave.handle_loading_file(outside)  # no ppu.json above it: exit 2, "no ppu.json in any directory above"
	await _frames(5)
	_ok("a PNG outside any project opens the normal way, silently", m.plain_opens == plain + 1 and m.snes.is_empty() and m.last_error == err0, m.last_error)
	_close_dialogs()
	OpenSave.handle_loading_file(proj.path_join("art/quant.png"))  # listed, no palette, not SNES art: path "quantize"
	await _frames(5)
	_ok("a listed PNG the import would quantize opens plain, with the Convert offer", m.plain_opens == plain + 2 and m.snes.is_empty() and m.last_error == err0 and m.last_note.contains("Project > Convert to SNES art…"), m.last_note)
	_close_dialogs()
	_ok("each open ran ppu once on the UI thread", m.ui_spawns == spawns + 2, m.ui_spawns - spawns)


func _bg() -> void:
	var t0 := Time.get_ticks_usec()
	OpenSave.handle_loading_file(proj.path_join("art/bg.png"))
	var r0 := await _wait_check()
	var bg: Project = Global.current_project
	res["metrics"]["bg_open_to_first_check_ms"] = (Time.get_ticks_usec() - t0) / 1000.0
	_ok("bg opens as SNES art", m.snes.has(bg))
	_ok("bg is indexed", bg.is_indexed())
	var pal: Palette = Palettes.current_palette
	_ok("palette is 16 wide, 64 entries", pal.width == 16 and pal.colors.size() == 64, "%d x %d, %d" % [pal.width, pal.height, pal.colors.size()])
	_ok("entry 0 of each row is Pixelorama's transparent", pal.get_color(0).a == 0.0 and pal.get_color(16).a == 0.0 and pal.get_color(1).a == 1.0)
	var seen := {}
	for k in pal.colors.size():
		if k % 16 != 0:
			seen[pal.get_color(k).to_rgba32()] = true
	_ok("every opaque swatch shows a colour of its own", seen.size() == pal.colors.size() - 4, "%d of %d" % [seen.size(), pal.colors.size() - 4])
	_ok("grid is the 8x8 tile", Global.grids[0].grid_size == Vector2i(8, 8) and Global.draw_grid)
	_ok("index 1 and 17 stay distinct", _px(bg, 1, 0) == 1 and _px(bg, 9, 0) == 17, "%d %d" % [_px(bg, 1, 0), _px(bg, 9, 0)])
	var fx = _read_json(proj.path_join("../fixtures.json"))
	_ok("first check flags exactly the 3 mixed tiles", r0.get("findings", []).size() == 3, r0)
	var want := []
	for t in fx["mixed_tiles"]:
		want.append(Rect2(t[0] * 8, t[1] * 8, 8, 8))
	var got: Array = m.overlay.rects.duplicate()
	var same := got.size() == 3
	for r in want:
		same = same and got.has(r)
	_ok("overlay marks those tiles", same, got)
	var status: String = m.status_label.text
	var rx := RegEx.create_from_string("^Tiles: \\d+ / \\d+    Palettes: \\d+ / \\d+$")
	_ok("status line counts findings", status == "3 findings", status)
	_ok("budget line reads Tiles: used / max", rx.search(m.budget_label.text) != null, m.budget_label.text)
	var f0: Dictionary = r0["findings"][0]
	_ok("finding line is rule [x,y]: message, in the strip's tooltip", m.status_panel.tooltip_text.begins_with("%s [%d,%d]: %s" % [f0["rule"], f0["x"], f0["y"], f0["message"]]), m.status_panel.tooltip_text)
	var panel: Control = m.status_panel
	var win := Rect2(Vector2.ZERO, Vector2(get_viewport().get_visible_rect().size))
	_ok("budgets are visible in the default layout", panel.is_visible_in_tree() and win.encloses(panel.get_global_rect()) and panel.size.x > 50, "%s in %s" % [panel.get_global_rect(), win])
	_ok("the strip is one line: status and budget only", m.budget_label.get_global_rect().position.y == m.status_label.get_global_rect().position.y and panel.size.y < 50, panel.size)
	await _shot("01-bg-open-default-layout")

	# Save with no edit: identical pixels.
	_ok("write without edits succeeds", m.write_snes(bg, out_dir.path_join("bg-noedit.png")) == "")

	# Scripted edits: single pixels in their own tile's row, one stroke each.
	var edits := []
	for i in 10:
		var x := 100 + i * 7
		var y := 100 + i * 5
		var row := ((x / 8) + (y / 8)) % 4
		var idx := row * 16 + 1 + (i % 15)
		_stroke([Vector2i(x, y)], idx)
		edits.append([x, y, idx])
	await _frames(2)
	for e in edits:
		_ok("pixel %d,%d holds index %d" % [e[0], e[1], e[2]], _px(bg, e[0], e[1]) == e[2], _px(bg, e[0], e[1]))
	res["edits"]["bg"] = edits
	await _ctrl_s()
	_ok("Ctrl+S opened no Save dialog", not Global.control.save_sprite_dialog.visible)
	_ok("Ctrl+S left no .pxo path", bg.save_path == "", bg.save_path)
	_ok("Ctrl+S cleared the unsaved mark", not bg.has_changed)
	m.write_snes(bg, out_dir.path_join("bg-edited.png"))

	# Live check after a stroke, undo, redo.
	m.debounce.stop()
	await _wait_check_idle()
	_stroke([Vector2i(3, 3)], 21)  # tile 0,0 lives in row 0; paint row 1: the top left corner of the art
	await _wait_check()
	_stroke([Vector2i(2 * 8 + 3, 2 * 8 + 3)], 21)  # tile 2,2 lives in row 0; paint row 1
	var r1 := await _wait_check()
	_ok("stroke makes the tile a finding", _has_tile(r1, 2, 2) and m.overlay.rects.has(Rect2(16, 16, 8, 8)), m.overlay.rects)
	bg.undo_redo.undo()
	var r2 := await _wait_check()
	_ok("undo clears the mark", not _has_tile(r2, 2, 2) and not m.overlay.rects.has(Rect2(16, 16, 8, 8)))
	bg.undo_redo.redo()
	var r3 := await _wait_check()
	_ok("redo marks it again", _has_tile(r3, 2, 2) and m.overlay.rects.has(Rect2(16, 16, 8, 8)))
	_ok("a tile in the top left corner is marked too", _has_tile(r3, 0, 0) and m.overlay.rects.has(Rect2(0, 0, 8, 8)))
	await _frames(2)
	_marks_clear_of_strip(bg)
	await _shot("02-bg-after-stroke-marks")
	bg.undo_redo.undo()
	await _wait_check()
	bg.undo_redo.undo()
	await _wait_check()

	# A placeless plte finding outlines the image; an unknown rule still counts and shows.
	m._apply(bg, {"ok": false, "source": "sky", "kind": "bg", "findings": [
		{"rule": "plte", "index": 3, "message": "palette entry 3 differs"},
		{"rule": "from-the-future", "message": "something new"}]})
	_ok("plte finding outlines the image", m.overlay.outline == Rect2(Vector2.ZERO, Vector2(bg.size)))
	_ok("unknown rule counts as a finding", m.status_label.text == "2 findings" and m.status_panel.tooltip_text == "plte: palette entry 3 differs\nfrom-the-future: something new", m.status_panel.tooltip_text)
	await _wait_check_idle()
	m._start_check()
	await _wait_check()

	# Palette: locked whole. A change is put back in full, with one toast, and no file is written.
	var files_before := _file_hashes(proj)
	Palettes.current_palette_set_color(16, Color8(1, 1, 1))
	await _frames(2)
	var c16: Color = Palettes.current_palette.get_color(16)
	_ok("edit of entry 0 is reverted", c16.to_rgba32() == m.snes[bg]["display"][16].to_rgba32() and m.snes[bg]["reverted"] == 1, c16)
	_ok("the put-back toast", m.last_note == LOCK_TOAST, m.last_note)
	m.write_snes(bg, out_dir.path_join("bg-palette.png"))
	await _palette_lock(bg)
	var files_after := _file_hashes(proj)
	_ok("no palette change wrote a file: every .gpl and PNG keeps its bytes", files_after == files_before and files_before.size() > 10, "%d files" % files_before.size())
	await _long_error(bg)

	# Ctrl+S belongs to a focused text field.
	bg.has_changed = true
	var le := LineEdit.new()
	add_child(le)
	le.grab_focus()
	await _frames(2)
	await _ctrl_s()
	_ok("Ctrl+S in a text field does not save", bg.has_changed)
	le.queue_free()
	await _frames(2)
	var dialog: Window = Global.control.save_sprite_dialog  # Pixelorama's own exclusive dialog
	dialog.popup_centered()
	await _frames(3)
	await _ctrl_s()
	_ok("Ctrl+S with a modal dialog open does not save", bg.has_changed and dialog.visible)
	dialog.hide()
	await _frames(3)
	await _ctrl_s()
	_ok("Ctrl+S saves again once they are gone", not bg.has_changed)

	# Export format writes the same indexed PNG.
	_ok("export writes the indexed PNG", m.export_indexed({"project": bg, "export_paths": [out_dir.path_join("bg-export.png")]}))


## path -> content hash of every .gpl and .png under `dir`.
func _file_hashes(dir: String) -> Dictionary:
	var out := {}
	for f in DirAccess.get_files_at(dir):
		if f.ends_with(".gpl") or f.ends_with(".png"):
			out[dir.path_join(f)] = FileAccess.get_sha256(dir.path_join(f))
	for d in DirAccess.get_directories_at(dir):
		out.merge(_file_hashes(dir.path_join(d)))
	return out


## Pixelorama's own PNG import dialog stays up until answered; the runner answers it by closing it.
func _close_dialogs() -> void:
	for w in get_tree().root.find_children("*", "Window", true, false):
		if w.visible and w.exclusive:
			w.hide()


func _wait_check_idle() -> void:
	var t := Time.get_ticks_msec()
	while (m._busy or not m.debounce.is_stopped()) and Time.get_ticks_msec() - t < 5000:
		await get_tree().process_frame


func _has_tile(r: Dictionary, x: int, y: int) -> bool:
	for f in r.get("findings", []):
		if f.get("x") == x and f.get("y") == y:
			return true
	return false


## Where a canvas-space rect is on screen: the canvas lives in a SubViewport inside main_viewport.
func _screen_rect(r: Rect2) -> Rect2:
	var xf: Transform2D = m.overlay.get_global_transform_with_canvas()
	var t: Rect2 = xf * r
	return Rect2(Global.main_viewport.get_global_rect().position + t.position, t.size)


## The strip never lies over the art: no mark's on-screen rect meets it, at the zoom the project opened at.
func _marks_clear_of_strip(p: Project) -> void:
	var strip: Rect2 = m.status_panel.get_global_rect()
	var view: Rect2 = Global.main_viewport.get_global_rect()
	var hit := 0
	var seen := 0
	for r in m.overlay.rects:
		var sr := _screen_rect(r)
		hit += 1 if sr.intersects(strip) else 0
		seen += 1 if view.encloses(sr) else 0
	_ok("no mark meets the strip (marks at [0,0] and [2,2] among %d)" % m.overlay.rects.size(), m.overlay.rects.has(Rect2(0, 0, 8, 8)) and m.overlay.rects.has(Rect2(16, 16, 8, 8)) and hit == 0, "%d hit, strip %s" % [hit, strip])
	_ok("every mark is on screen in the canvas", seen == m.overlay.rects.size(), "%d of %d in %s" % [seen, m.overlay.rects.size(), view])
	_ok("the strip does not overlap the canvas viewport", not strip.intersects(view), "%s vs %s" % [strip, view])
	_ok("the art is fully shown at the opened zoom", view.grow(2).encloses(_screen_rect(Rect2(Vector2.ZERO, Vector2(p.size)))), "%s in %s" % [_screen_rect(Rect2(Vector2.ZERO, Vector2(p.size))), view])


const LOCK_TOAST := "The palette is locked for SNES art: put back."


## Every palette change ends with the palette equal to its open-time display colours; the next check
## starts and the whole-image remap every flip, move and effect ends with keeps every index.
func _palette_lock(back: Project) -> void:
	var cases := [
		["edit of a transparent entry", func(pal): pal.set_color(16, Color.RED)],
		["colour edit of an opaque entry", func(pal): pal.set_color(3, Color8(10, 20, 30))],
		["colour edit of an opaque entry, then undo and redo", func(pal):
			pal.set_color(3, Color8(10, 20, 30))
			Global.current_project.undo_redo.undo()
			Global.current_project.undo_redo.redo()],
		["alpha edit of an opaque entry", func(pal): pal.set_color(3, Color(1, 0, 0, 0.5))],
		["swap 6 and 7", func(pal): pal.swap_colors(6, 7)],
		["sort by hue", func(pal): pal.sort(Palettes.SortOptions.HUE)],
		["reverse", func(pal): pal.reverse_colors()],
		["insert a colour", func(pal): pal.insert_color(5, Color.RED)],
		["delete an opaque entry", func(pal): pal.remove_color(17)],
		["delete entry 16", func(pal): pal.remove_color(16)],
		["resize to 8 x 8", func(pal): pal.edit(pal.name, 8, 8, pal.comment)],
		["global edit 32 x 4", func(_pal): Palettes.current_palette_edit(Palettes.current_palette.name, "", 32, 4, true)],
		["global edit 32 x 2", func(_pal): Palettes.current_palette_edit(Palettes.current_palette.name, "", 32, 2, true)],
		["global edit 16 x 8", func(_pal): Palettes.current_palette_edit(Palettes.current_palette.name, "", 16, 8, true)],
		["switch to another palette", func(_pal): Palettes.select_palette("Default")],
		["delete the project palette", func(pal): Palettes.palette_delete_and_reselect(true, pal)],
		["a global palette with the same name", func(pal):
			var twin := Palette.new(pal.name, 8, 1)
			Palettes.palettes[pal.name] = twin
			Palettes.select_palette(pal.name)],
	]
	for c in cases:
		OpenSave.handle_loading_file(proj.path_join("art/bg.png"))
		await _wait_check()
		var p: Project = Global.current_project
		var s: Dictionary = m.snes[p]
		var shown: PackedColorArray = s["display"]
		var before: PackedByteArray = m.canvas_indices(m.snapshot(p))
		m.last_note = ""
		c[1].call(s["palette"])
		await _frames(3)
		m._start_check()  # what the next stroke does
		await _wait_check()
		await _frames(3)
		var pal: Palette = Palettes.current_palette
		var same: bool = pal == s["palette"] and pal.width == 16 and pal.height == 4 and pal.colors.size() == shown.size()
		for k in shown.size():
			same = same and pal.get_color(k) != null and pal.get_color(k).to_rgba32() == shown[k].to_rgba32()
		_ok("palette %s: equals the open-time colours" % c[0], same, "%d colours, %dx%d" % [pal.colors.size(), pal.width, pal.height])
		_ok("palette %s: the toast says it was put back" % c[0], m.last_note == LOCK_TOAST, m.last_note)
		var reverted: int = s["reverted"]
		m._start_check()
		await _wait_check()
		_ok("palette %s: the next check puts nothing back" % c[0], s["reverted"] == reverted, "%d -> %d" % [reverted, s["reverted"]])
		Palettes.palettes.erase(s["palette"].name)  # the same-name twin, if a case made one
		p.frames[0].cels[0].get_image().convert_rgb_to_indexed()
		await _frames(2)
		var after: PackedByteArray = m.canvas_indices(m.snapshot(p))
		var n := 0
		for i in before.size():
			n += 1 if before[i] != after[i] else 0
		_ok("palette %s: a remap changes no index" % c[0], n == 0, "%d differ" % n)
	Global.tabs.current_tab = Global.projects.find(back)
	await _frames(2)


## A long "Check failed" is cut inside the strip's width, and the tooltip keeps all of it.
func _long_error(p: Project) -> void:
	var long_text := "ppu gave no result (exit 3): " + "/very/long/path/to/the/project/".repeat(20)
	m._apply(p, {"error": long_text})
	await _frames(3)
	var strip: Rect2 = m.status_panel.get_global_rect()
	var column: Rect2 = Global.tabs.get_parent().get_parent().get_global_rect()
	_ok("a long Check failed line stays inside the strip", m.status_label.text.begins_with("Check failed: ") and column.encloses(strip) and strip.encloses(m.status_label.get_global_rect()), "%s in %s" % [strip, column])
	_ok("the strip's tooltip holds the whole failure", m.status_panel.tooltip_text == "Check failed: " + long_text)
	m._start_check()
	await _wait_check()


func _m7() -> void:
	var t0 := Time.get_ticks_usec()
	OpenSave.handle_loading_file(proj.path_join("art/track.png"))
	var r := await _wait_check()
	var p: Project = Global.current_project
	res["metrics"]["m7_open_to_first_check_ms"] = (Time.get_ticks_usec() - t0) / 1000.0
	_ok("m7 opens as SNES art", m.snes.has(p) and p.is_indexed() and m.snes[p]["shifted"])
	_ok("m7 index 255 survives open", _px(p, 0, 0) == 255 and _px(p, 63, 63) == 255, _px(p, 0, 0))
	_ok("m7 index 0 survives open", _px(p, 1023, 1023) == 0)
	_ok("m7 check ran clean of errors", not r.has("error") and not r.has("timeout"), r)
	_ok("m7 palette holds colours 1..255", Palettes.current_palette.colors.size() == 255)
	_ok("m7 write without edits succeeds", m.write_snes(p, out_dir.path_join("track-noedit.png")) == "")
	var edits := []
	for i in 6:
		var x := 300 + i * 11
		var y := 500 + i * 3
		var idx := 254 + (i % 2)  # PNG index 254 / 255 via slots 253 / 254
		_stroke([Vector2i(x, y)], idx - 1)
		edits.append([x, y, idx])
	await _frames(2)
	for e in edits:
		_ok("m7 pixel %d,%d holds index %d" % [e[0], e[1], e[2]], _px(p, e[0], e[1]) == e[2], _px(p, e[0], e[1]))
	res["edits"]["track"] = edits
	m.write_snes(p, out_dir.path_join("track-edited.png"))
	await _shot("03-m7-open")


func _remap_case(label: String, p: Project, op: Callable) -> void:
	var img: ImageExtended = p.frames[0].cels[0].get_image()
	var before: PackedByteArray = m.canvas_indices(m.snapshot(p))
	op.call(img)
	img.convert_rgb_to_indexed()  # what every one of these Pixelorama operations ends with
	await _frames(2)
	var after: PackedByteArray = m.canvas_indices(m.snapshot(p))
	var n := 0
	var first := ""
	for i in before.size():
		if before[i] != after[i]:
			n += 1
			if first == "":
				first = "%d->%d@%d" % [before[i], after[i], i]
	_ok("%s: zero index diffs" % label, n == 0 and before.size() == after.size(), "%d differ, first %s" % [n, first])


## Pixelorama redoes the index image from RGB after these; every index must come back as it was.
func _remaps() -> void:
	for art in ["bg", "m7"]:
		OpenSave.handle_loading_file(proj.path_join("art/bg.png" if art == "bg" else "art/track.png"))
		await _wait_check()
		var p: Project = Global.current_project
		var cel: PixelCel = p.frames[0].cels[0]
		var region := Rect2i(8, 8, 64, 64)
		await _remap_case(art + " flip twice", p, func(img): img.flip_x(); img.flip_x())
		await _remap_case(art + " flip vertically twice", p, func(img): img.flip_y(); img.flip_y())
		await _remap_case(art + " rotate 4 x 90", p, func(img):
			for i in 4:
				img.rotate_90(CLOCKWISE))
		await _remap_case(art + " move a selection by 0", p, func(img):
			var r: Image = img.get_region(region)
			img.blit_rect_mask(r, r, Rect2i(Vector2i.ZERO, r.get_size()), region.position))
		await _remap_case(art + " delete an empty selection", p, func(_img): pass)
		await _remap_case(art + " image brush over itself, overwriting", p, func(img):
			img.blit_rect(img.get_region(region), Rect2i(Vector2i.ZERO, region.size), region.position))
		await _remap_case(art + " image brush over itself, blending", p, func(img):
			img.blend_rect(img.get_region(region), Rect2i(Vector2i.ZERO, region.size), region.position))
		# An image brush that paints one existing colour onto a pixel that held another.
		var img: ImageExtended = cel.get_image()
		var before: PackedByteArray = m.canvas_indices(m.snapshot(p))
		var w := p.size.x
		var src := Vector2i(5, 5)
		var dst := Vector2i(100, 100)
		var brush: Image = img.get_region(Rect2i(src, Vector2i(1, 1)))
		img.blit_rect(brush, Rect2i(0, 0, 1, 1), dst)
		img.convert_rgb_to_indexed()
		await _frames(2)
		var after: PackedByteArray = m.canvas_indices(m.snapshot(p))
		var diffs := 0
		for i in before.size():
			diffs += 1 if before[i] != after[i] else 0
		_ok(art + " brush paints exactly the index of its colour", after[dst.y * w + dst.x] == before[src.y * w + src.x] and diffs <= 1, "%d vs %d, %d diffs" % [after[dst.y * w + dst.x], before[src.y * w + src.x], diffs])

		# A cel smaller than the canvas (a selection deleted on a mostly empty layer).
		var keep := Rect2i(40, 50, 30, 20)
		var full: PackedByteArray = m.canvas_indices(m.snapshot(p))
		var want := PackedByteArray()
		want.resize(full.size())
		var base: PackedByteArray = m.snes[p]["base"]
		var row: int = m.snes[p]["row"]
		for i in want.size():  # outside the kept box: "none", written as the baseline's entry 0 there
			want[i] = base[i] if base.size() > i and base[i] % row == 0 else 0
		for y in range(keep.position.y, keep.end.y):
			for x in range(keep.position.x, keep.end.x):
				want[y * w + x] = full[y * w + x]
		var kept: Image = img.get_region(keep)
		img.fill(Color(0, 0, 0, 0))
		img.blit_rect(kept, Rect2i(Vector2i.ZERO, keep.size), keep.position)
		img.convert_rgb_to_indexed()
		cel.shrink_to_content()
		await _frames(2)
		var small: ImageExtended = cel.get_image()
		_ok(art + " sparse cel is smaller than the canvas", cel.offset == keep.position and small.get_size() == keep.size, "%s %s" % [cel.offset, small.get_size()])
		_ok(art + " sparse cel saves the canvas indices", m.canvas_indices(m.snapshot(p)) == want)
		await _remap_case(art + " flip twice on the sparse cel", p, func(i2): i2.flip_x(); i2.flip_x())
		await _remap_case(art + " rotate 4 x 90 on the sparse cel", p, func(i2):
			for i in 4:
				i2.rotate_90(CLOCKWISE))


func _open_art(rel: String) -> Project:
	OpenSave.handle_loading_file(proj.path_join(rel))
	await _wait_check()
	return Global.current_project


## The real selection move: cut, bake through the viewport, blit with the alpha mask.
func _viewport_move(p: Project, region: Rect2i, shift: Vector2i) -> void:
	var img: ImageExtended = p.frames[0].cels[0].get_image()
	var src := Image.new()
	src.copy_from(img.get_region(region))
	img.blit_rect(Image.create(region.size.x, region.size.y, false, img.get_format()), Rect2i(Vector2i.ZERO, region.size), region.position)
	var algo: int = Global.canvas.selection.transformation_handles.transformation_algorithm
	DrawingAlgos.transform_image_with_viewport(src, Transform2D.IDENTITY, Vector2(region.size) / 2.0, algo, Rect2i(Vector2i.ZERO, region.size))
	img.blit_rect_mask(src, src, Rect2i(Vector2i.ZERO, src.get_size()), region.position + shift)
	img.convert_rgb_to_indexed()
	await _frames(2)


## The 16 x 16 box whose copy `shift` px to the right lies over the most opaque art under its transparent pixels.
func _pick_region(idx: PackedByteArray, w: int, h: int, row: int, shift: int) -> Rect2i:
	var best := Rect2i(0, 0, 16, 16)
	var most := 0
	for y in range(0, h - 16 + 1, 2):
		for x in range(0, w - 16 - shift + 1, 2):
			var under := 0
			for j in 16:
				for i in 16:
					var s_i := (y + j) * w + x + i
					if idx[s_i] % row == 0 and idx[s_i + shift] % row != 0:
						under += 1
			if under > most:
				most = under
				best = Rect2i(x, y, 16, 16)
	return best


## Transparent pixels (entry 0 of a row) must behave like transparent in every Pixelorama tool, and
## come back out as the entry 0 they were.
func _transparency() -> void:
	for rel in ["art/bg.png", "art/hero.png"]:
		var art: String = rel.get_file().get_basename()
		var p := await _open_art(rel)
		var w := p.size.x
		var row: int = m.snes[p]["row"]
		var before: PackedByteArray = m.canvas_indices(m.snapshot(p))
		var img: ImageExtended = p.frames[0].cels[0].get_image()
		var colors: int = (m.snes[p]["colors"] as PackedColorArray).size()
		var n_clear := 0
		var rows_seen := {}
		for v in before:
			if v % row == 0:
				n_clear += 1
				rows_seen[v] = true
		_ok(art + ": the art has transparent pixels", n_clear > 0 and (rows_seen.size() > 1 or art != "bg"), rows_seen.keys())

		# A selection moved by a non-zero offset: only its opaque pixels land.
		var shift := Vector2i(21 if art == "bg" else 16, 0)  # bg repeats every 16 px: a multiple lines holes up with holes
		var region := _pick_region(before, w, p.size.y, row, shift.x)
		await _viewport_move(p, region, shift)
		var after: PackedByteArray = m.canvas_indices(m.snapshot(p))
		var wrong := 0
		var under := 0
		for y in p.size.y:
			for x in w:
				var i := y * w + x
				var want: int = before[i]
				if region.has_point(Vector2i(x, y)):
					wrong += 0 if after[i] % row == 0 else 1
					continue
				var rel_pt := Vector2i(x, y) - region.position - shift
				if Rect2i(Vector2i.ZERO, region.size).has_point(rel_pt):
					var s_i := rel_pt.y * w + rel_pt.x + region.position.y * w + region.position.x
					if before[s_i] % row != 0:
						want = before[s_i]
					elif before[i] % row != 0:
						under += 1
				if after[i] != want:
					wrong += 1
		_ok(art + ": a moved selection leaves the art under its transparent pixels (%d of them)" % under, wrong == 0 and under >= 12, "%d wrong" % wrong)

		# A brush with transparent areas blended over opaque art.
		p = await _open_art(rel)
		before = m.canvas_indices(m.snapshot(p))
		img = p.frames[0].cels[0].get_image()
		var clear := Image.create(32, 32, false, img.get_format())
		img.blend_rect(clear, Rect2i(0, 0, 32, 32), Vector2i(8, 8))
		img.convert_rgb_to_indexed()
		await _frames(2)
		after = m.canvas_indices(m.snapshot(p))
		_ok(art + ": blending a fully transparent brush changes nothing", after == before)
		var mixed: Image = img.get_region(region)
		img.blend_rect(mixed, Rect2i(0, 0, 16, 16), region.position + shift)
		img.convert_rgb_to_indexed()
		await _frames(2)
		after = m.canvas_indices(m.snapshot(p))
		var diffs := 0
		for y in 16:
			for x in 16:
				var s_i := (region.position.y + y) * w + region.position.x + x
				var d_i := s_i + shift.x
				var want: int = before[s_i] if before[s_i] % row != 0 else before[d_i]
				diffs += 1 if after[d_i] != want else 0
		for i in before.size():
			var at := Vector2i(i % w, i / w)
			if not Rect2i(region.position + shift, region.size).has_point(at):
				diffs += 1 if after[i] != before[i] else 0
		_ok(art + ": blending a brush with transparent areas lands only its opaque pixels", diffs == 0, "%d differ" % diffs)

		# Colour effects (Invert) keep alpha: transparent stays, nothing lands past the palette.
		p = await _open_art(rel)
		before = m.canvas_indices(m.snapshot(p))
		img = p.frames[0].cels[0].get_image()
		for y in p.size.y:
			for x in w:
				var c := img.get_pixel(x, y)
				img.set_pixel(x, y, Color(1.0 - c.r, 1.0 - c.g, 1.0 - c.b, c.a))
		img.convert_rgb_to_indexed()
		await _frames(2)
		after = m.canvas_indices(m.snapshot(p))
		var moved := 0
		var past := 0
		var unique := {}
		for i in before.size():
			if before[i] % row == 0:
				moved += 1 if after[i] != before[i] else 0
			else:
				past += 1 if after[i] >= colors else 0
				unique[after[i]] = true
		_ok(art + ": Invert keeps transparent pixels exactly", moved == 0, "%d moved" % moved)
		_ok(art + ": Invert sends no opaque pixel past the palette", past == 0, "%d past %d entries" % [past, colors])
		_ok(art + ": Invert still tells colours apart", unique.size() > 3, unique.size())

		# Lock alpha protects transparent pixels.
		p = await _open_art(rel)
		before = m.canvas_indices(m.snapshot(p))
		img = p.frames[0].cels[0].get_image()
		var clear_at := Vector2i(-1, -1)
		var opaque_at := Vector2i(-1, -1)
		for i in before.size():
			if before[i] % row == 0 and clear_at.x < 0:
				clear_at = Vector2i(i % w, i / w)
			if before[i] % row != 0 and opaque_at.x < 0:
				opaque_at = Vector2i(i % w, i / w)
		Tools.alpha_locked = true
		_ok(art + ": lock alpha holds a transparent pixel", Tools.check_alpha_lock(img, clear_at), clear_at)
		_ok(art + ": lock alpha lets an opaque pixel paint", not Tools.check_alpha_lock(img, opaque_at), opaque_at)
		_stroke([clear_at], 5)
		await _frames(2)
		Tools.alpha_locked = false
		after = m.canvas_indices(m.snapshot(p))
		_ok(art + ": a locked stroke leaves the transparent pixel", after == before)

	# Flip and rotate move transparent pixels; untouched ones keep their row's entry 0 in the file.
	var p := await _open_art("art/bg.png")
	var before: PackedByteArray = m.canvas_indices(m.snapshot(p))
	var img: ImageExtended = p.frames[0].cels[0].get_image()
	img.flip_x()
	img.convert_rgb_to_indexed()
	await _frames(2)
	var after: PackedByteArray = m.canvas_indices(m.snapshot(p))
	var w := p.size.x
	var flipped_opaque := 0
	var moved_clear := 0
	var became_opaque := 0
	var kept_rows := {}
	for y in p.size.y:
		for x in w:
			var i := y * w + x
			var j := y * w + (w - 1 - x)
			if before[i] % 16 != 0:
				flipped_opaque += 1 if after[j] != before[i] else 0
			elif before[j] % 16 != 0:
				became_opaque += 1 if after[i] % 16 == 0 else 0  # the mirrored opaque pixel landed here
			else:
				moved_clear += 1 if after[i] != before[i] else 0
				kept_rows[after[i]] = true
	_ok("bg flip: every opaque pixel lands mirrored", flipped_opaque == 0, flipped_opaque)
	_ok("bg flip: transparent over transparent keeps its entry 0 (rows %s)" % str(kept_rows.keys()), moved_clear == 0 and kept_rows.has(16) and kept_rows.has(32) and kept_rows.has(48), moved_clear)
	_ok("bg flip: opaque pixels land where transparent ones were", became_opaque == 0, became_opaque)
	img.flip_x()
	img.convert_rgb_to_indexed()
	await _frames(2)
	var out_png := out_dir.path_join("bg-flip-twice.png")
	_ok("bg flip twice saves", m.write_snes(p, out_png) == "")
	var saved: Dictionary = m.Png.read_png(out_png)
	_ok("bg flip twice: saved PNG indices equal the opened ones", saved["idx"] == before)


## Stroke-end -> highlight, and what a check costs the next stroke, on bg and m7.
func _latency() -> void:
	for art in ["bg", "m7"]:
		OpenSave.handle_loading_file(proj.path_join("art/bg.png" if art == "bg" else "art/track.png"))
		await _wait_check()
		var p: Project = Global.current_project
		await _wait_check_idle()
		var swatch := 21 if art == "bg" else 40
		var base := []  # stroke cost with checks off
		m.checks_enabled = false
		for i in 12:
			var t := Time.get_ticks_usec()
			_stroke([Vector2i(40 + i * 5, 60), Vector2i(44 + i * 5, 62)], swatch + (i % 2))
			base.append((Time.get_ticks_usec() - t) / 1000.0)
			await _frames(2)
		var base_frames := await _frame_times(30)
		m.checks_enabled = true
		var with := []  # strokes 120 ms apart: the previous check is still debouncing or running
		var frame_ms := []
		var overlap := 0
		for i in 12:
			if m._busy:
				overlap += 1
			var t := Time.get_ticks_usec()
			_stroke([Vector2i(40 + i * 5, 90), Vector2i(44 + i * 5, 92)], swatch + (i % 2))
			with.append((Time.get_ticks_usec() - t) / 1000.0)
			var tend := Time.get_ticks_msec()
			while Time.get_ticks_msec() - tend < 120:
				var d0 := Time.get_ticks_usec()
				await get_tree().process_frame
				frame_ms.append((Time.get_ticks_usec() - d0) / 1000.0)
		await _wait_check_idle()
		# Stroke end -> highlight drawn, one stroke at a time.
		var e2h := []
		for i in 8:
			await _wait_check_idle()
			await _frames(3)
			var t := Time.get_ticks_usec()
			_stroke([Vector2i(60 + i * 9, 150)], swatch + 3 + (i % 2))
			var te := Time.get_ticks_usec()
			var r := await _wait_check()
			e2h.append((Time.get_ticks_usec() - te) / 1000.0)
			res["metrics"]["%s_last_worker_ms" % art] = m.timing.get("worker_ms")
			res["metrics"]["%s_last_convert_ms" % art] = m.timing.get("convert_ms")
		res["metrics"][art + "_latency"] = {
			"stroke_ms_checks_off_median": _med(base), "stroke_ms_checks_off_max": base.max(),
			"stroke_ms_checks_on_median": _med(with), "stroke_ms_checks_on_max": with.max(),
			"strokes_started_while_a_check_ran": overlap,
			"frame_ms_idle_max": base_frames.max(), "frame_ms_idle_median": _med(base_frames),
			"frame_ms_during_checks_max": frame_ms.max(), "frame_ms_during_checks_median": _med(frame_ms),
			"stroke_end_to_highlight_ms_median": _med(e2h), "stroke_end_to_highlight_ms_max": e2h.max(),
			"stroke_end_to_highlight_ms_all": e2h,
		}
		_ok("%s: a check never blocks the UI beyond a frame budget" % art, frame_ms.max() < 100.0, frame_ms.max())


func _frame_times(n: int) -> Array:
	var a := []
	for i in n:
		var t := Time.get_ticks_usec()
		await get_tree().process_frame
		a.append((Time.get_ticks_usec() - t) / 1000.0)
	return a


## Switching project: the strip shows the new project's state at once, not the old one's verdict.
func _switching() -> void:
	var hero := await _open_art("art/hero.png")
	var bg := await _open_art("art/bg.png")
	await _wait_check_idle()
	var tabs := Global.projects.size()
	var hero_tab := Global.projects.find(hero)
	var bg_tab := Global.projects.find(bg)
	Global.tabs.current_tab = hero_tab
	await _wait_check()
	_ok("the strip shows hero's verdict", m.status_label.text == "No findings: the art would import.", m.status_label.text)
	var spawns: int = m.ui_spawns
	m._start_check()  # a check is now running for hero
	_ok("a check is busy", m._busy)
	Global.tabs.current_tab = bg_tab
	_ok("switching shows Checking… at once, even while a check is busy", m.status_label.text == "Checking…" and m.budget_label.text == "" and m.status_panel.tooltip_text == "", "%s | %s" % [m.status_label.text, m.budget_label.text])
	await _wait_check()
	await _wait_check_idle()
	_ok("then it shows the new project's verdict", m.status_label.text == "3 findings" and m.overlay.rects.size() == 3, m.status_label.text)
	Global.tabs.current_tab = hero_tab
	await _frames(2)
	Global.tabs.current_tab = bg_tab
	await _wait_check_idle()
	_ok("tab switches spawn no ppu on the UI thread", m.ui_spawns == spawns, m.ui_spawns - spawns)

	# A closed project leaves the plugin's state.
	var extra := await _open_art("art/hero.png")
	var held: int = m.snes.size()
	Global.tabs.current_tab = Global.projects.find(extra)
	await _frames(2)
	Global.tabs.delete_tab(Global.projects.find(extra))
	await _frames(5)
	_ok("a closed project is dropped from the state", not Global.projects.has(extra) and not m.snes.has(extra) and m.snes.size() == held - 1, "%d -> %d" % [held, m.snes.size()])


## Entries beyond entry 0 of a row: 256-colour art with Mode 7 EXTBG also makes entry 128 transparent.
func _extbg_units() -> void:
	var tmask := PackedByteArray()
	tmask.resize(256)
	tmask[0] = 1
	tmask[128] = 1
	var snap := {"data": PackedByteArray([0, 3, 0, 255]), "iw": 4, "ih": 1, "ox": 0, "oy": 0, "w": 4, "h": 1,
		"shifted": true, "plain_shift": false, "base": PackedByteArray([128, 3, 0, 255]), "tmask": tmask}
	_ok("256-colour art: a transparent pixel is written as the transparent entry it held (128 stays 128)", m.canvas_indices(snap) == PackedByteArray([128, 3, 0, 255]), m.canvas_indices(snap))
	var colors := PackedColorArray()
	for i in 256:
		colors.append(Color8(i, 255 - i, 7))
	var shown: PackedColorArray = m.display_colors(colors, 1, tmask)
	_ok("256-colour art: entry 128 is shown transparent, the others are not", shown.size() == 255 and shown[127].a == 0.0 and shown[0].a == 1.0 and shown[128].a == 1.0)
	var png: PackedByteArray = m.Png.trns_bytes(256, tmask)
	_ok("tRNS marks exactly the transparent entries", png[0] == 0 and png[128] == 0 and png[1] == 255 and png[255] == 255)


## Mode 7 EXTBG art with a 128-colour .gpl opens (its PLTE is the .gpl mirrored to 256), and entries 0 and 128 are transparent.
func _extbg_open() -> void:
	OpenSave.handle_loading_file(proj.path_join("art/world.png"))
	await _wait_check()
	var p: Project = Global.current_project
	_ok("EXTBG art with a 128-colour .gpl opens as SNES art", m.snes.has(p), m.last_error)
	if not m.snes.has(p):
		return
	var s: Dictionary = m.snes[p]
	_ok("EXTBG art's real verdict makes exactly entries 0 and 128 transparent", Array(s["transparent"]).map(func(x): return int(x)) == [0, 128] and s["tmask"][0] == 1 and s["tmask"][128] == 1 and Array(s["tmask"]).count(1) == 2, "%s %d" % [s["transparent"], Array(s["tmask"]).count(1)])


func _row_of(p: Project, x: int, y: int) -> int:
	return _px(p, x, y) / 16


## True (and the dialog closed) when the last command refused with `part` in its message.
func _refused_with(part: String) -> bool:
	var hit: bool = m.last_error.contains(part)
	_close_dialogs()
	return hit


## Set tile row and Fix tile rows: one undo step each, ppu's bytes, the refusals of Aseprite.
func _rows() -> void:
	var rd := proj.path_join("rows")
	OpenSave.handle_loading_file(rd.path_join("art/bg.png"))
	await _wait_check()
	var p: Project = Global.current_project
	var s: Dictionary = m.snes[p]
	var before: PackedByteArray = m.canvas_indices(m.snapshot(p))
	var w := p.size.x
	var png_disk := FileAccess.get_file_as_bytes(s["png"])

	# refusals change nothing
	m.last_error = ""
	var ok: bool = await m.set_tile_row(p)
	_ok("set tile row with no selection refuses with a message", not ok and _refused_with("select the tiles"), m.last_error)
	p.selection_map.select_rect(Rect2i(8, 8, 16, 8))
	p.selection_map_changed()
	Palettes.left_selected_color = -1
	m.last_error = ""
	ok = await m.set_tile_row(p)
	_ok("set tile row with no colour picked refuses", not ok and _refused_with("pick a colour"), m.last_error)
	_ok("refusals change no pixel and leave the worker free", m.canvas_indices(m.snapshot(p)) == before and not m._busy)
	var extra := PixelLayer.new(p)
	p.layers.append(extra)
	m.last_error = ""
	ok = await m.fix_tile_rows(p)
	p.layers.erase(extra)
	_ok("more than one layer is refused", not ok and _refused_with("one layer and one frame"), m.last_error)
	m.last_error = ""
	var plain: Project = Global.projects.filter(func(q): return not m.snes.has(q))[0]  # the plain-opened PNG's tab
	ok = await m.fix_tile_rows(plain)
	_ok("a project that is not SNES art is refused", not ok and m.last_error.contains("not opened as SNES art"), "%s | %s" % [m.last_error, Global.projects.size()])
	_close_dialogs()
	Global.tabs.current_tab = Global.projects.find(p)
	await _frames(3)

	# set the row of two tiles: pick a colour of row 3
	Palettes.current_palette_select_color(MOUSE_BUTTON_LEFT, 3 * 16 + 5)
	await _frames(2)
	m.last_note = ""
	ok = await m.set_tile_row(p)
	await _wait_check_idle()
	var after: PackedByteArray = m.canvas_indices(m.snapshot(p))
	var inside := true
	var outside := true
	for y in p.size.y:
		for x in w:
			var in_rect := x >= 8 and x < 24 and y >= 8 and y < 16
			if in_rect:
				inside = inside and after[y * w + x] / 16 == 3
			else:
				outside = outside and after[y * w + x] == before[y * w + x]
	_ok("set tile row: ok, every pixel of the touched tiles is on row 3", ok and inside, m.last_error)
	_ok("set tile row: nothing outside those tiles moved", outside)
	var base: PackedByteArray = s["base"]
	var clear := 0
	var base_ok := true
	for y in range(8, 16):
		for x in range(8, 24):
			if after[y * w + x] % 16 == 0:
				clear += 1
				base_ok = base_ok and base[y * w + x] == after[y * w + x]
	_ok("set tile row: the baseline holds the new row's transparent entries, as a save would (M3)", clear > 0 and base_ok, "%d transparent pixels" % clear)
	_ok("set tile row: its toast and undo name", m.last_note.begins_with("Set tile row 3.") and p.undo_redo.get_current_action_name() == "Set tile row", "%s | %s" % [m.last_note, p.undo_redo.get_current_action_name()])
	p.undo_redo.undo()
	await _frames(2)
	_ok("set tile row: one undo restores every pixel", m.canvas_indices(m.snapshot(p)) == before)
	p.undo_redo.redo()
	await _frames(2)
	_ok("set tile row: redo lays it again", m.canvas_indices(m.snapshot(p)) == after)
	await _wait_check_idle()
	_ok("set tile row: Save SNES art writes it", m.save_snes(p) == "" and m.canvas_indices(m.snapshot(p)) == after)
	_ok("set tile row: the PNG on disk changed", FileAccess.get_file_as_bytes(s["png"]) != png_disk)
	res["rows"] = {"rect": [8, 8, 16, 8], "row": 3}

	# fix tile rows: one fixable stray pixel, one with no colour in its tile's row
	OpenSave.handle_loading_file(rd.path_join("art/rowsfix.png"))
	var r0 := await _wait_check()
	var f: Project = Global.current_project
	var fb: PackedByteArray = m.canvas_indices(m.snapshot(f))
	var mixed0 := Array(r0.get("findings", [])).filter(func(x): return x.get("rule") == "mixed-rows").map(func(x): return Vector2i(x["x"], x["y"]))
	_ok("the fixture starts with two mixed tiles", mixed0.size() == 2 and mixed0.has(Vector2i(1, 0)) and mixed0.has(Vector2i(2, 0)), mixed0)
	m.last_note = ""
	ok = await m.fix_tile_rows(f)
	var r1 := await _wait_check()
	var fa: PackedByteArray = m.canvas_indices(m.snapshot(f))
	var mixed1 := Array(r1.get("findings", [])).filter(func(x): return x.get("rule") == "mixed-rows").map(func(x): return Vector2i(x["x"], x["y"]))
	_ok("fix: the stray pixel with its colour in the row moved there", ok and fa[3 * 32 + 11] == 17, fa[3 * 32 + 11])
	_ok("fix: the stray pixel with no colour in the row stayed", fa[3 * 32 + 19] == 7, fa[3 * 32 + 19])
	_ok("fix: the check no longer finds the fixable tile, only the leftover", mixed1 == [Vector2i(2, 0)], mixed1)
	_ok("fix: the leftover tile is listed", m.last_note.contains("no colour in row 3") and m.last_note.contains("2,0"), m.last_note)
	_ok("fix: it is one undo step named Fix tile rows", f.undo_redo.get_current_action_name() == "Fix tile rows")
	f.undo_redo.undo()
	await _frames(2)
	_ok("fix: one undo restores every pixel", m.canvas_indices(m.snapshot(f)) == fb)
	f.undo_redo.redo()
	await _frames(2)
	await _wait_check_idle()
	var action: int = f.undo_redo.get_current_action()
	ok = await m.fix_tile_rows(f)
	await _frames(2)
	_ok("fix: when nothing moves it succeeds and adds no undo step", ok and f.undo_redo.get_current_action() == action and m.canvas_indices(m.snapshot(f)) == fa, "%s %d vs %d" % [ok, f.undo_redo.get_current_action(), action])
	_ok("fix: save writes the fixed art", m.save_snes(f) == "")

	# 8 bpp / Mode 7 art has no rows
	OpenSave.handle_loading_file(rd.path_join("art/track.png"))
	await _wait_check()
	var t: Project = Global.current_project
	t.selection_map.select_rect(Rect2i(0, 0, 8, 8))
	t.selection_map_changed()
	m.last_error = ""
	var tb: PackedByteArray = m.canvas_indices(m.snapshot(t))
	ok = await m.fix_tile_rows(t)
	_ok("fix on Mode 7 art is refused", not ok and _refused_with("Mode 7"))
	m.last_error = ""
	Palettes.current_palette_select_color(MOUSE_BUTTON_LEFT, 5)
	ok = await m.set_tile_row(t)
	_ok("set tile row on Mode 7 art is refused", not ok and _refused_with("Mode 7") and m.canvas_indices(m.snapshot(t)) == tb)
	_ok("no job file is left by the row commands", Array(DirAccess.get_files_at(m.tmp_dir)).is_empty(), DirAccess.get_files_at(m.tmp_dir))
	# ppu answering with something else than index bytes is refused, nothing changes
	var good: String = m.ppu_bin
	var junk := out_dir.path_join("junk_ppu.sh")
	var jf := FileAccess.open(junk, FileAccess.WRITE)
	# a ppu whose job answers with two index bytes: `art job <dir>` leaves the three result files itself
	jf.store_string("#!/bin/sh\nd=$3\nprintf '{\"ok\":true,\"protocol\":1,\"pixels\":\"7878\",\"notes\":[]}' > \"$d/job.out\"\n: > \"$d/job.err\"\necho 0 > \"$d/job.code\"\n")
	jf.close()
	OS.execute("chmod", ["+x", junk])
	Global.tabs.current_tab = Global.projects.find(f)
	await _frames(3)
	m.ppu_bin = junk
	m.last_error = ""
	var fb2: PackedByteArray = m.canvas_indices(m.snapshot(f))
	ok = await m.fix_tile_rows(f)
	m.ppu_bin = good
	_ok("short or unknown ppu output is refused with a message, nothing changes", not ok and _refused_with("bytes for") and m.canvas_indices(m.snapshot(f)) == fb2 and not m._busy, m.last_error)


## New SNES art: ppu writes the PNG, the .gpl and the ppu.json entry; the PNG then opens as SNES art. A ppu refusal is shown
## as it is and nothing is written.
func _new_art() -> void:
	var rd := proj.path_join("rows")
	var png_path := rd.path_join("art/fresh.png")
	var json_path := rd.path_join("ppu.json")
	var tabs := Global.projects.size()
	m.last_error = ""
	var ok: bool = await m.new_art({"png": png_path, "kind": "bg", "size": "64x32", "new_palette": "fresh_pal"})
	await _wait_check()
	await _wait_check_idle()
	var p: Project = Global.current_project
	_ok("new art: ppu made the files and the PNG opened as SNES art", ok and Global.projects.size() == tabs + 1 and m.snes.has(p) and m.snes[p]["png"] == png_path, m.last_error)
	_ok("new art: the check is clean and the canvas is 64x32", m.status_label.text == "No findings: the art would import." and p.size == Vector2i(64, 32), m.status_label.text)
	_ok("new art: no job files left", Array(DirAccess.get_files_at(m.tmp_dir)).is_empty(), DirAccess.get_files_at(m.tmp_dir))
	var json_before := FileAccess.get_file_as_string(json_path)
	var png_before := FileAccess.get_file_as_bytes(png_path)
	tabs = Global.projects.size()
	m.last_error = ""
	ok = await m.new_art({"png": png_path, "kind": "bg", "size": "64x32", "new_palette": "again_pal"})
	_ok("new art: ppu's refusal of an existing PNG is shown verbatim, nothing changes", not ok and m.last_error.contains("exists") and Global.projects.size() == tabs
		and FileAccess.get_file_as_string(json_path) == json_before and FileAccess.get_file_as_bytes(png_path) == png_before
		and not FileAccess.file_exists(rd.path_join("palettes/again_pal.gpl")), m.last_error)
	_close_dialogs()
	m.last_error = ""
	ok = await m.new_art({"png": rd.path_join("art/odd.png"), "kind": "bg", "size": "30x30", "new_palette": "odd_pal"})
	_ok("new art: a size off the grid is refused by ppu, nothing written", not ok and m.last_error.contains("New SNES art: ") and not FileAccess.file_exists(rd.path_join("art/odd.png")), m.last_error)
	_close_dialogs()
	res["new_art"] = {"png": png_path, "json": json_path, "gpl": rd.path_join("palettes/fresh_pal.gpl")}


## A plain open of a PNG with other tabs open asks Pixelorama's import question; the default answer is a new tab.
func _accept_import() -> void:
	for w in get_tree().root.find_children("*", "ConfirmationDialog", true, false):
		if w.visible and w.title != "Convert to SNES art":
			w.confirmed.emit()
			w.hide()
	await _frames(5)


func _convert_dialog() -> ConfirmationDialog:
	m.convert_dialog()
	await _frames(3)
	var d: ConfirmationDialog = null
	for w in get_tree().root.find_children("*", "ConfirmationDialog", true, false):
		if w.visible and w.title == "Convert to SNES art":
			d = w
	return d


## Project > Convert to SNES art…: the dialog fills from ppu.json, Convert runs ppu art convert once and the PNG reopens as SNES art.
func _convert() -> void:
	var rd := proj.path_join("rows")
	var json_path := rd.path_join("ppu.json")
	# a full-colour PNG the project lists opens plain with the offer; its tab is what gets converted
	var full := rd.path_join("art/full.png")
	m.last_note = ""
	OpenSave.handle_loading_file(full)
	await _frames(5)
	await _accept_import()
	var old: Project = Global.current_project
	_ok("convert: a listed full-colour PNG opens plain, with the Convert offer", not m.snes.has(old) and m.plain.get(old) == full and m.last_note.contains("Project > Convert to SNES art…"), m.last_note)
	var d := await _convert_dialog()
	_ok("convert: the dialog opens", d != null)
	if d == null:
		return
	await _shot("convert-dialog")
	var kind: OptionButton = d.find_child("kind", true, false)
	_ok("convert: filled from the tab and ppu.json: path, kind bg fixed, palette name, cell off", d.find_child("png", true, false).text == full and kind.get_item_text(kind.selected) == "bg" and kind.disabled
		and d.find_child("new_palette", true, false).text == "full_pal" and not d.find_child("cell", true, false).editable and d.find_child("max_palettes", true, false).editable)
	var tabs := Global.projects.size()
	d.confirmed.emit()
	var ok: bool = await m.command_done
	await _wait_check()
	await _wait_check_idle()
	var p: Project = Global.current_project
	_ok("convert: the PNG is SNES art, the plain tab is gone", ok and m.snes.has(p) and m.snes[p]["png"] == full and not Global.projects.has(old) and Global.projects.size() == tabs, "%s %s %d/%d" % [m.last_error, ok, Global.projects.size(), tabs])
	_ok("convert: the check passes", m.status_label.text == "No findings: the art would import.", m.status_label.text)
	_ok("convert: the report names colours, tiles, palettes and the backup", m.last_convert.contains("Colours: ") and m.last_convert.contains("Tiles: ") and m.last_convert.contains(".ppu-backup"), m.last_convert)
	_close_dialogs()
	# an unlisted PNG: the kind is chosen in the dialog and only what that kind reads is passed
	var photo := rd.path_join("unlisted/photo.png")
	OpenSave.handle_loading_file(photo)
	await _frames(5)
	await _accept_import()
	d = await _convert_dialog()
	kind = d.find_child("kind", true, false)
	_ok("convert: an unlisted PNG: kind is free", not kind.disabled and d.find_child("png", true, false).text == photo)
	kind.select(2)  # obj
	kind.item_selected.emit(2)
	d.find_child("cell", true, false).text = "16"
	d.find_child("dither", true, false).select(1)  # bayer
	_ok("convert: obj turns the bg-only max palettes off", not d.find_child("max_palettes", true, false).editable)
	d.confirmed.emit()
	ok = await m.command_done
	await _wait_check()
	await _wait_check_idle()
	var j = JSON.parse_string(FileAccess.get_file_as_string(json_path))
	var src: Array = j["sources"].filter(func(x): return x["name"] == "photo")
	var o: Dictionary = src[0]["options"] if src.size() == 1 else {}
	_ok("convert: listed as obj with its cell and dither, nothing the kind does not read", ok and src.size() == 1 and src[0]["kind"] == "obj" and o.get("cell_size") == 16 and o.get("dither") == "bayer"
		and not o.has("max_palettes") and not o.has("extbg") and not o.has("tile_size"), "%s %s" % [m.last_error, src])
	_close_dialogs()
	# a refusal (a new palette name a source already has): ppu's words, no file changed
	var clash := rd.path_join("unlisted/clash.png")
	OpenSave.handle_loading_file(clash)
	await _frames(5)
	await _accept_import()
	var before := _file_hashes(rd)
	var json_before := FileAccess.get_file_as_bytes(json_path)
	tabs = Global.projects.size()
	d = await _convert_dialog()
	d.find_child("kind", true, false).select(0)
	d.find_child("new_palette", true, false).text = "sprites_pal"
	m.last_error = ""
	d.confirmed.emit()
	ok = await m.command_done
	_ok("convert: a refusal shows ppu's message, nothing changes", not ok and m.last_error.begins_with("Convert to SNES art: ") and m.last_error.contains("sprites_pal")
		and _file_hashes(rd) == before and FileAccess.get_file_as_bytes(json_path) == json_before and Global.projects.size() == tabs and not m.snes.has(Global.current_project), m.last_error)
	_close_dialogs()
	# a typed path to some other existing PNG is refused and that file is left byte-identical
	var other := rd.path_join("art/bg.png")
	var other_bytes := FileAccess.get_file_as_bytes(other)
	d = await _convert_dialog()
	d.find_child("png", true, false).text = other
	m.last_error = ""
	d.confirmed.emit()
	ok = await m.command_done
	_ok("convert: a typed path to another existing PNG is refused, that file untouched", not ok and m.last_error.contains("bg.png") and m.last_error.contains("is not this tab's PNG")
		and FileAccess.get_file_as_bytes(other) == other_bytes and _file_hashes(rd) == before and Global.projects.size() == tabs, m.last_error)
	_close_dialogs()
	# a plain tab with two layers converts and stays open: only its first layer was converted
	var layers := rd.path_join("unlisted/layers.png")
	OpenSave.handle_loading_file(layers)
	await _frames(5)
	await _accept_import()
	var two: Project = Global.current_project
	two.layers.append(PixelLayer.new(two))
	for fr in two.frames:
		fr.cels.append(two.layers[1].new_empty_cel())
	tabs = Global.projects.size()
	d = await _convert_dialog()
	d.find_child("kind", true, false).select(0)
	d.confirmed.emit()
	ok = await m.command_done
	_ok("convert: a two-layer plain tab converts and stays open", ok and Global.projects.has(two) and not m.snes.has(two) and Global.projects.size() == tabs + 1
		and m.snes.has(Global.current_project) and m.snes[Global.current_project]["png"] == layers and m.last_convert.contains("stays open"), "%s %s %s" % [m.last_error, ok, m.last_convert])
	_close_dialogs()
	res["convert"] = {"png": full, "json": json_path, "gpl": rd.path_join("palettes/full_pal.gpl")}


## The New SNES art dialog: a Browse… beside each path, the palette choice exclusive, only the chosen value reaches ppu (M2).
func _new_art_dialog() -> void:
	var rd := proj.path_join("rows")
	m.new_art_dialog()
	await _frames(3)
	var d: ConfirmationDialog = null
	for w in get_tree().root.find_children("*", "ConfirmationDialog", true, false):
		if w.visible and w.title == "New SNES art":
			d = w
	_ok("new art dialog: it opens", d != null)
	if d == null:
		return
	await _shot("snes-m2-new-art-dialog")
	var png: LineEdit = d.find_child("png", true, false)
	var name_f: LineEdit = d.find_child("new_palette", true, false)
	var gpl: LineEdit = d.find_child("gpl", true, false)
	var b_png: Button = d.find_child("browse_png", true, false)
	var b_gpl: Button = d.find_child("browse_gpl", true, false)
	var new_pick: CheckBox = d.find_child("pick_new", true, false)
	var old_pick: CheckBox = d.find_child("pick_old", true, false)
	_ok("new art dialog: a Browse… beside the PNG path and the .gpl path", b_png != null and b_gpl != null)
	_ok("new art dialog: 'new palette' is chosen: its name field is on, the .gpl field and its Browse… are off", new_pick.button_pressed and name_f.editable and not gpl.editable and b_gpl.disabled)
	old_pick.button_pressed = true
	await _frames(2)
	_ok("new art dialog: choosing the existing .gpl swaps them", not new_pick.button_pressed and not name_f.editable and gpl.editable and not b_gpl.disabled)
	new_pick.button_pressed = true
	await _frames(2)
	_ok("new art dialog: and back", name_f.editable and not gpl.editable and b_gpl.disabled)
	# Browse… opens a file dialog (save for the PNG) whose pick fills the field
	b_png.pressed.emit()
	await _frames(3)
	var fd: FileDialog = null
	for w in get_tree().root.find_children("*", "FileDialog", true, false):
		if w.visible:
			fd = w
	_ok("new art dialog: Browse… on the PNG opens a save dialog", fd != null and fd.file_mode == FileDialog.FILE_MODE_SAVE_FILE)
	if fd != null:
		fd.file_selected.emit("/somewhere/else.png")
		fd.hide()
		_ok("new art dialog: the picked path fills the PNG field", png.text == "/somewhere/else.png", png.text)
	b_gpl.disabled = false
	b_gpl.pressed.emit()
	await _frames(3)
	fd = null
	for w in get_tree().root.find_children("*", "FileDialog", true, false):
		if w.visible:
			fd = w
	_ok("new art dialog: Browse… on the .gpl opens an open dialog filtered to *.gpl", fd != null and fd.file_mode == FileDialog.FILE_MODE_OPEN_FILE and Array(fd.filters)[0].begins_with("*.gpl"))
	if fd != null:
		fd.hide()
	# both palette fields filled, "new" chosen: a new .gpl is made, the existing one is not used
	png.text = rd.path_join("art/chosen.png")
	name_f.text = "chosen_pal"
	gpl.text = rd.path_join("palettes/sprites_pal.gpl")
	var tabs := Global.projects.size()
	d.confirmed.emit()
	await _wait_check()
	await _wait_check_idle()
	var j = JSON.parse_string(FileAccess.get_file_as_string(rd.path_join("ppu.json")))
	var src: Array = j["sources"].filter(func(x): return x["name"] == "chosen")
	_ok("new art dialog: both filled, 'new' chosen: the new palette is made and used", FileAccess.file_exists(rd.path_join("palettes/chosen_pal.gpl")) and src.size() == 1 and src[0]["options"]["palette"] == "chosen_pal" and Global.projects.size() == tabs + 1, "%s %s" % [m.last_error, src])
	_close_dialogs()
	# the other way round: the existing .gpl is used, the typed name makes no file
	m.new_art_dialog()
	await _frames(3)
	d = null
	for w in get_tree().root.find_children("*", "ConfirmationDialog", true, false):
		if w.visible and w.title == "New SNES art":
			d = w
	d.find_child("png", true, false).text = rd.path_join("art/chosen2.png")
	d.find_child("new_palette", true, false).text = "ignored_pal"
	d.find_child("gpl", true, false).text = rd.path_join("palettes/sprites_pal.gpl")
	d.find_child("pick_old", true, false).button_pressed = true
	await _frames(2)
	d.confirmed.emit()
	await _wait_check()
	await _wait_check_idle()
	j = JSON.parse_string(FileAccess.get_file_as_string(rd.path_join("ppu.json")))
	src = j["sources"].filter(func(x): return x["name"] == "chosen2")
	_ok("new art dialog: 'existing .gpl' chosen: that palette is used, the typed name makes no file", not FileAccess.file_exists(rd.path_join("palettes/ignored_pal.gpl")) and src.size() == 1 and src[0]["options"]["palette"] == "sprites_pal", "%s %s" % [m.last_error, src])
	_close_dialogs()


## Failures are shown, not swallowed: ppu missing, a manifest error, a refusal to open.
func _failures() -> void:
	OpenSave.handle_loading_file(proj.path_join("art/hero.png"))
	await _wait_check()
	var p: Project = Global.current_project
	_ok("obj opens with a 16x16 cell grid", m.snes.has(p) and Global.grids[0].grid_size == Vector2i(16, 16), Global.grids[0].grid_size)
	_ok("obj shows its budget line", m.budget_label.text.begins_with("Tiles: "), m.status_label.text)
	var good: String = m.ppu_bin
	m.ppu_bin = "/nonexistent/ppu"
	_stroke([Vector2i(1, 1)], 3)
	var r := await _wait_check()
	_ok("missing ppu is reported", m.status_label.text == "Check failed: ppu is not on PATH (set PPU_BIN)", m.status_label.text)
	m.ppu_bin = good

	var manifest := proj.path_join("ppu.json")
	var saved := FileAccess.get_file_as_string(manifest)
	var j = JSON.parse_string(saved)
	j["sources"] = j["sources"].filter(func(s): return s["name"] != "hero")
	var f := FileAccess.open(manifest, FileAccess.WRITE)
	f.store_string(JSON.stringify(j))
	f.close()
	_stroke([Vector2i(2, 2)], 4)
	r = await _wait_check()
	_ok("a manifest error (exit 2) is shown, not a crash", r.has("error") and m.status_label.text.begins_with("Check failed: ") and "not the file of any source" in m.status_label.text, m.status_label.text)
	f = FileAccess.open(manifest, FileAccess.WRITE)
	f.store_string(saved)
	f.close()

	# A listed PNG whose palette cannot be read is refused with a message, no project is made.
	var bad := proj.path_join("bad")
	DirAccess.make_dir_recursive_absolute(bad.path_join("art"))
	FileAccess.open(bad.path_join("art/x.png"), FileAccess.WRITE).store_buffer(FileAccess.get_file_as_bytes(proj.path_join("art/hero.png")))
	var bf := FileAccess.open(bad.path_join("ppu.json"), FileAccess.WRITE)
	bf.store_string('{"title":"t","files":["a"],"sources":[{"name":"x","kind":"obj","file":"art/x.png","options":{"palette":"nope"}}]}')
	bf.close()
	var n := Global.projects.size()
	m.open_png(bad.path_join("art/x.png"))
	await _frames(3)
	_ok("any other exit-2 error is shown with its message and opens nothing", "nope" in m.last_error and Global.projects.size() == n, m.last_error)
	# Non-indexed listed PNG falls to the normal open.
	var png: Dictionary = m.Png.read_png(proj.path_join("unlisted/rgba.png"))
	_ok("a full-colour PNG is not read as indexed", not png["indexed"], png)
	_ok("no leftover temp files", Array(DirAccess.get_files_at(proj.path_join("art"))).filter(func(x): return x.ends_with(".snesart-tmp")).is_empty())
	_ok("no leftover job files", Array(DirAccess.get_files_at(m.tmp_dir)).is_empty(), DirAccess.get_files_at(m.tmp_dir))

	# ppu cannot run: the PNG opens as a plain image, with a toast; nothing is claimed about it.
	m.ppu_bin = "/nonexistent/ppu"
	n = Global.projects.size()
	var plain_n: int = m.plain_opens
	var snes_n: int = m.snes.size()
	m.last_error = ""
	m.open_png(proj.path_join("art/hero.png"))
	await _frames(5)
	_close_dialogs()
	_ok("without ppu a PNG opens plain with a toast", m.plain_opens == plain_n + 1 and m.snes.size() == snes_n and m.last_note.begins_with("SNES art check needs ppu on PATH") and m.last_error == "", m.last_note)
	m.ppu_bin = good

	# A ppu whose answers carry no protocol number: refused, no guessing.
	var old := out_dir.path_join("old_ppu.sh")
	var sf := FileAccess.open(old, FileAccess.WRITE)
	sf.store_string("#!/bin/sh\n'%s' \"$@\"\nsed -i 's/\"protocol\":1,//' \"$3/job.out\"\n" % good)
	sf.close()
	OS.execute("chmod", ["+x", old])
	m.ppu_bin = old
	n = Global.projects.size()
	m.open_png(proj.path_join("art/hero.png"))
	await _frames(5)
	_ok("a ppu too old for SNES art is refused with the message", m.last_error == "this ppu is too old for SNES art (update ppu)" and Global.projects.size() == n, m.last_error)
	m.ppu_bin = good
	await _shot("04-obj-final")

## The palette stays registered once under its open-time name, and a put-back does what picking a palette does.
func _carried_minors() -> void:
	OpenSave.handle_loading_file(proj.path_join("art/bg.png"))
	await _wait_check()
	var p: Project = Global.current_project
	var s: Dictionary = m.snes[p]
	var pal: Palette = s["palette"]
	var pname: String = pal.name
	# a deleted project palette brought back by an undo is listed once, under its name
	Palettes.palette_delete_and_reselect(false, pal)
	await _frames(2)
	p.undo_redo.undo()
	await _frames(3)
	m._start_check()
	await _wait_check()
	var holds := p.palettes.keys().filter(func(k): return p.palettes[k] == pal)
	_ok("undo of a palette delete: the palette is listed once, under its open-time name", holds == [pname] and p.palettes.size() == 1 and pal.name == pname, "%s %s" % [p.palettes.keys(), pal.name])
	_ok("undo of a palette delete: it is the current palette, its colours are the saved ones", Palettes.current_palette == pal and p.project_current_palette_name == pname and _swatches_are_saved(s, pal))
	# the by-object put-back after another palette was picked and a swatch of it selected: it clears the selection
	# and writes last_palette as picking the palette would
	Palettes.palettes["other_pal"] = Palette.new("other_pal", 8, 1)
	Palettes.palettes["other_pal"].add_color(Color.RED)
	m._guard = true
	Palettes.select_palette("other_pal")
	Palettes.current_palette_select_color(MOUSE_BUTTON_LEFT, 0)
	m._guard = false
	var picked := Global.config_cache.get_value("data", "last_palette", "")
	var picked_swatch := Palettes.left_selected_color
	m._start_check()
	await _wait_check()
	await _frames(3)
	_ok("another palette was current: it is picked and a swatch of it selected", picked == "other_pal" and picked_swatch == 0, "%s %d" % [picked, picked_swatch])
	_ok("the by-object put-back is the project's palette again", Palettes.current_palette == pal and p.project_current_palette_name == pname)
	_ok("the by-object put-back clears the selected swatch, as picking a palette does", Palettes.left_selected_color == -1, Palettes.left_selected_color)
	_ok("the by-object put-back writes last_palette, as picking a palette does", Global.config_cache.get_value("data", "last_palette", "") == pname, Global.config_cache.get_value("data", "last_palette", ""))
	Palettes.palettes.erase("other_pal")
	# a stroke in a colour that is not on the palette: Pixelorama 1.2.4 snaps it to a swatch, adds none, and no index leaves the palette
	var before: PackedByteArray = m.canvas_indices(m.snapshot(p))
	var reverted: int = s["reverted"]
	Tools.assign_tool("Pencil", MOUSE_BUTTON_LEFT)
	Tools.assign_color(Color(0.123, 0.456, 0.789), MOUSE_BUTTON_LEFT)
	var down := InputEventMouseButton.new()
	down.button_index = MOUSE_BUTTON_LEFT
	down.pressed = true
	Tools.handle_draw(Vector2i(40, 40), down)
	var mv := InputEventMouseMotion.new()
	mv.pressure = 1.0
	Tools.handle_draw(Vector2i(44, 40), mv)
	var up := InputEventMouseButton.new()
	up.button_index = MOUSE_BUTTON_LEFT
	up.pressed = false
	Tools.handle_draw(Vector2i(44, 40), up)
	await _wait_check()
	await _frames(3)
	var after: PackedByteArray = m.canvas_indices(m.snapshot(p))
	var drawn := 0
	for i in after.size():
		drawn += 1 if after[i] != before[i] else 0
	_ok("an off-palette colour stroke: pixels were drawn, with indices inside the palette", drawn > 0 and Array(after).max() < s["display"].size(), "%d drawn, max %d" % [drawn, Array(after).max()])
	_ok("an off-palette colour stroke: no swatch was added and nothing was put back", pal.colors.size() == s["display"].size() and s["reverted"] == reverted, "%d swatches, %d -> %d put-backs" % [pal.colors.size(), reverted, s["reverted"]])


func _swatches_are_saved(s: Dictionary, pal: Palette) -> bool:
	for k in s["display"].size():
		var c = pal.get_color(k)
		if c == null or c.to_rgba32() != s["display"][k].to_rgba32():
			return false
	return pal.colors.size() == s["display"].size()

