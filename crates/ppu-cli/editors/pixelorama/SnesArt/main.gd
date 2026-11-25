extends Node
## SNES art in Pixelorama: opens a project PNG as indexed art when `ppu art check` says it is one,
## flags bad tiles live from the same check (no rule is decided here), and saves the indexed PNG.
##
## Pixelorama stores an indexed pixel as palette index + 1 in an R8 image (0 = "none"). Art with
## 16 or fewer colours per row keeps slot = PNG index. 256-colour art would need index 255 + 1,
## so there PNG index 0 is "none" and slot k is PNG index k + 1: the stored byte *is* the PNG index.
##
## Pixelorama rebuilds the whole index image from the pixels' RGBA after flips, rotations, effects,
## selection moves and image brushes, choosing the first swatch that matches exactly. So every opaque
## swatch shows a colour no other swatch shows (see display_colors) and the PNG's PLTE is written from
## the true colours, never from the swatches. Entry 0 of a row is Pixelorama's transparent (alpha 0), so
## masks, blends, lock alpha and effects leave it alone; which entry 0 a transparent pixel held is
## recovered from the last opened or saved PNG when the indices are written (canvas_indices).
## Which entries are transparent, the row size and the unit all come from the check's JSON.
##
## The palette is locked: a change in the tab is put back, and nothing here ever writes a .gpl. Colours are edited
## in the .gpl and taken when the PNG is reopened.

signal check_finished(result: Dictionary)
signal command_done(ok: bool)  ## a row command ended (ok: it changed the art, or had nothing to change)

const Png := preload("res://src/Extensions/SnesArt/png.gd")
const OverlayScript := preload("res://src/Extensions/SnesArt/overlay.gd")
const MIN_VERSION: Array[int] = [1, 2, 4]
const DEBOUNCE_SEC := 0.15
## The machine contract of `ppu art ... --json` this extension reads (docs: ppu docs cli).
const PROTOCOL := 1
const TOO_OLD := "this ppu is too old for SNES art (update ppu)"
## ppu's `reason` for a PNG that belongs to no project, or is no source's file: such a PNG opens the normal way.
const UNLISTED := ["not_project", "not_listed"]
## Seconds ppu gives one job before it kills it (the runner's own timeout; a convert can be slow).
const JOB_TIMEOUT := 120

var api: Node
var snes := {}  ## Project -> state dict (see _open_snes)
var overlay: Node2D
var status_panel: PanelContainer
var status_label: Label
var budget_label: Label
var debounce := Timer.new()
var ppu_bin := "ppu"
var tmp_dir := ""
var timing := {}  ## measurements for the test runner
var last_result := {}
var refused := ""
var checks_enabled := true  ## off: edits no longer start a check (used to measure what checks cost)
var last_error := ""  ## the last error dialog's text
var last_note := ""  ## the last toast's text
var ui_spawns := 0  ## ppu runs made on the UI thread (the open-time check only)
var plain_opens := 0  ## PNGs handed to Pixelorama's own open
var _job_seq := 0

var _guard := false
var _gen := 0  ## bumped on every edit; a result for an older generation is dropped
var _busy := false  ## one ppu job (a check or a row command) at a time, on _thread
var _rerun := false
var _thread := Thread.new()
var _menu_ids: Array[int] = []
var _cmd_ids: Array[int] = []  ## the Project menu's items
var _export_id := -1
var _exporter: RefCounted
var plain := {}  ## Project -> the PNG a plain (non-SNES) tab was opened from
var _last_plain := ""  ## the PNG handed to Pixelorama's own open last
var last_convert := ""  ## the last Convert report's text
var _plain_grid = null  ## the grid a non-SNES project had, restored when switching back


class MenuItem:
	extends RefCounted
	var cb: Callable

	func _init(c: Callable) -> void:
		cb = c

	func menu_item_clicked() -> void:
		cb.call()


class Exporter:
	extends RefCounted
	var main: Node

	func _init(m: Node) -> void:
		main = m

	func override_export(details: Dictionary) -> bool:
		return main.export_indexed(details)


## "" when `version` ("1.2.4", "1.3.0-dev", ...) is new enough, else the refusal text.
static func refusal_for(version: String) -> String:
	var parts := version.split("-")[0].split(".")
	for i in 3:
		var n := parts[i].to_int() if i < parts.size() else 0
		if n != MIN_VERSION[i]:
			if n > MIN_VERSION[i]:
				return ""
			return "SNES art needs Pixelorama 1.2.4 or newer (this is %s)." % version
	return ""


func _ready() -> void:
	api = get_node("/root/ExtensionsApi")
	refused = refusal_for(api.general.get_pixelorama_version())
	if refused != "":
		api.dialog.show_error(refused)
		return
	var bin := OS.get_environment("PPU_BIN")
	if bin != "":
		ppu_bin = bin
	tmp_dir = OS.get_user_data_dir().path_join("snesart")
	DirAccess.make_dir_recursive_absolute(tmp_dir)

	debounce.one_shot = true
	debounce.wait_time = DEBOUNCE_SEC
	debounce.timeout.connect(_start_check)
	add_child(debounce)

	overlay = OverlayScript.new()
	overlay.z_index = 100
	api.general.get_canvas().add_child(overlay)
	_build_status()

	api.signals.signal_project_data_changed(_on_changed)
	api.signals.signal_project_switched(_on_switched)
	Global.palette_panel_updated.connect(_on_palette_panel_updated)
	Palettes.palette_selected.connect(_on_palette_selected)
	api.import.add_open_callback("png", open_png)
	for item in [["New SNES art…", func(): new_art_dialog()],
			["Save SNES art", func(): save_snes(Global.current_project)]]:
		_menu_ids.append(api.menu.add_menu_item(api.menu.FILE, item[0], MenuItem.new(item[1])))
	# Pixelorama has no Sprite menu (Aseprite's home of these): the Project menu
	for item in [["SNES art: set tile row", func(): set_tile_row(Global.current_project)],
			["SNES art: fix tile rows", func(): fix_tile_rows(Global.current_project)],
			["Convert to SNES art…", func(): convert_dialog()]]:
		_cmd_ids.append(api.menu.add_menu_item(api.menu.PROJECT, item[0], MenuItem.new(item[1])))
	_exporter = Exporter.new(self)
	_export_id = api.export.add_export_option(
		{"extension": ".png", "description": "SNES indexed PNG"}, _exporter, api.export.ExportTab.IMAGE, false
	)
	if OS.get_environment("PPU_ART_TEST") != "":
		var script = load("res://src/Extensions/SnesArt/autotest.gd")
		if script != null:
			var t: Node = script.new()
			add_child(t)
			t.run.call_deferred(self)


func _exit_tree() -> void:
	if refused != "":
		return
	api.import.remove_open_callback("png")
	api.signals.signal_project_data_changed(_on_changed, true)
	api.signals.signal_project_switched(_on_switched, true)
	Global.palette_panel_updated.disconnect(_on_palette_panel_updated)
	Palettes.palette_selected.disconnect(_on_palette_selected)
	for id in _menu_ids:
		api.menu.remove_menu_item(api.menu.FILE, id)
	for id in _cmd_ids:
		api.menu.remove_menu_item(api.menu.PROJECT, id)
	api.export.remove_export_option(_export_id)
	if _thread.is_started():
		_thread.wait_to_finish()
	overlay.queue_free()
	status_panel.queue_free()



## Ctrl+S on an SNES project writes the indexed PNG; Pixelorama's own save would write a .pxo.
func _input(event: InputEvent) -> void:
	if refused == "" and event.is_action_pressed(&"save_file") and snes.has(Global.current_project) \
			and not _typing_or_modal():
		get_viewport().set_input_as_handled()
		save_snes(Global.current_project)


## A text field has focus or a dialog holds the input: Ctrl+S belongs to them, not to the canvas.
func _typing_or_modal() -> bool:
	var focus := get_viewport().gui_get_focus_owner()
	if focus is LineEdit or focus is TextEdit:
		return true
	for w in get_tree().root.find_children("*", "Window", true, false):
		if w.visible and w.exclusive:
			return true
	return false


## A row of its own between the tabs and the canvas: visible in every layout, and it never lies over
## the art, so no mark can hide under it. Findings are in its tooltip.
func _build_status() -> void:
	status_panel = PanelContainer.new()
	status_panel.name = "SNES art"
	status_panel.mouse_filter = Control.MOUSE_FILTER_PASS
	var row := HBoxContainer.new()
	row.add_theme_constant_override(&"separation", 16)
	var title := Label.new()
	title.text = "SNES art"
	title.add_theme_color_override(&"font_color", Color(0.6, 0.85, 1))
	status_label = _strip_label()
	budget_label = Label.new()
	for l in [title, status_label, budget_label]:
		row.add_child(l)
	status_panel.add_child(row)
	var canvas_column: Node = Global.tabs.get_parent().get_parent()
	canvas_column.add_child(status_panel)
	canvas_column.move_child(status_panel, Global.tabs.get_parent().get_index() + 1)
	status_panel.visible = false


## A long text is cut with an ellipsis inside the strip's width; the tooltip holds all of it.
func _strip_label() -> Label:
	var l := Label.new()
	l.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	l.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	return l


func _error(text: String) -> void:
	last_error = text
	api.dialog.show_error(text)


func _toast(text: String) -> void:
	last_note = text
	Global.notification_label(text)


# --- open -------------------------------------------------------------------

## Runs `ppu <args> --json` through ppu's own job runner (`ppu art job`: no shell, nothing to quote) in the
## fresh folder `dir`: the args go in `job.args` (one per line, written last), `stdin` (bytes; null for none) in
## `job.in`; the runner leaves `job.out`, `job.err` and, last, `job.code`. The folder is removed again. Thread-safe.
## -> {"code", "out", "err", "answer", "bytes", "notes"}: `answer` is the JSON answer ({} when there is none), `err`
## the refusal's message (ppu's own words), `bytes` the answer's `pixels` decoded, `notes` its lines.
## code -1 means ppu could not run, -2 the job could not be written, -3 ppu ran but left no result.
static func run_cmd(ppu_bin: String, args: Array, dir: String, stdin = null) -> Dictionary:
	var r := {"code": -2, "out": "", "err": "", "answer": {}, "bytes": PackedByteArray(), "notes": []}
	var lines := PackedStringArray()
	for a in args:
		var line := str(a)
		if line.contains("\n") or line.contains("\r"):
			r["err"] = "an argument with a line break cannot be passed to ppu: " + line
			return r
		lines.append(line)
	if not args.has("--json"):
		lines.append("--json")
	if DirAccess.make_dir_recursive_absolute(dir) != OK:
		r["err"] = "cannot create " + dir
		return r
	var wrote := true
	if stdin != null:
		var f := FileAccess.open(dir.path_join("job.in"), FileAccess.WRITE)
		wrote = f != null
		if wrote:
			f.store_buffer(stdin)
			f.close()
	var af := FileAccess.open(dir.path_join("job.args"), FileAccess.WRITE) if wrote else null  # last: the runner takes a job when it appears
	if af == null:
		r["err"] = "cannot write the job files in " + dir
		_remove_dir(dir)
		return r
	af.store_string("\n".join(lines) + "\n")
	af.close()
	var runner_out := []
	var exit := OS.execute(ppu_bin, ["art", "job", dir, "--timeout", str(JOB_TIMEOUT)], runner_out, true)
	var code_text := FileAccess.get_file_as_string(dir.path_join("job.code")).strip_edges()
	if code_text == "":  # no .code: the runner never ran (-1), or this ppu has no `art job`
		r["code"] = -1 if exit == -1 or exit == 127 else -3  # Godot reports a program it cannot start as -1 or 127
		r["err"] = str(runner_out[0]).strip_edges() if runner_out.size() > 0 else ""
	else:
		r["code"] = int(code_text)
		r["out"] = FileAccess.get_file_as_string(dir.path_join("job.out"))
		r["err"] = FileAccess.get_file_as_string(dir.path_join("job.err"))
		var parsed = JSON.parse_string(r["out"]) if r["out"].strip_edges() != "" else null
		if typeof(parsed) == TYPE_DICTIONARY:
			r["answer"] = parsed
			if parsed.get("ok", true) == false and parsed.has("error"):
				r["err"] = str(parsed["error"])
			if parsed.get("reason") == "unmatched" and parsed.get("unmatched") is Array:
				r["err"] = "\n".join(PackedStringArray(parsed["unmatched"]))
			if parsed.get("pixels") is String:
				r["bytes"] = parsed["pixels"].hex_decode()
			if parsed.get("notes") is Array:
				r["notes"] = parsed["notes"]
	_remove_dir(dir)
	return r


## The job folder and what is in it (the runner leaves only flat files).
static func _remove_dir(dir: String) -> void:
	for f in DirAccess.get_files_at(dir):
		DirAccess.remove_absolute(dir.path_join(f))
	DirAccess.remove_absolute(dir)


## What went wrong in a failed run, as one line.
static func failure_text(r: Dictionary) -> String:
	if r["code"] == -1 or r["code"] == 127:
		return "ppu is not on PATH (set PPU_BIN)"
	var text: String = r["err"] if r["err"].strip_edges() != "" else r["out"]
	return "ppu failed (exit %d)%s" % [r["code"], ": " + text.strip_edges().left(600) if text.strip_edges() != "" else ""]


## A `ppu art check --json` run as the check's JSON, or {"error", "missing"?, "reason"?} when there is none.
## An answer without the protocol number is a ppu too old for this extension.
static func parse_check(r: Dictionary) -> Dictionary:
	if r["code"] == -1 or r["code"] == 127:
		return {"error": "ppu is not on PATH (set PPU_BIN)", "missing": true}
	var parsed: Dictionary = r["answer"]
	if parsed.is_empty():
		return {"error": "ppu gave no result (exit %d): %s%s" % [
			r["code"], (r["err"] if r["err"].strip_edges() != "" else r["out"]).left(600),
			" (is ppu up to date?)" if r["code"] == -3 else ""]}
	var protocol = parsed.get("protocol")
	if not (protocol is float) or protocol < PROTOCOL:
		return {"error": TOO_OLD, "reason": "too_old"}
	if protocol > PROTOCOL:
		return {"error": "this extension is too old for this ppu: run ppu art install pixelorama again", "reason": "extension_too_old"}
	return parsed


static func check_args(png: String, raw := false, width := 0, height := 0) -> Array:
	var args := ["art", "check", png, "--json"]
	if raw:  # raw index bytes on stdin instead of the PNG's pixels
		args.append_array(["--stdin", "--width", width, "--height", height])
	return args


## Runs `ppu art check`; returns its JSON, or {"error", "missing"?} when it cannot be had. Thread-safe.
static func run_check(ppu_bin: String, png: String, dir: String) -> Dictionary:
	return parse_check(run_cmd(ppu_bin, check_args(png), dir))


## Open callback for every .png. One synchronous check in file mode (not the stroke path) says what the
## manifest makes of this PNG; its verdict decides: SNES art here, Pixelorama's own open otherwise.
func open_png(path: String) -> void:
	_job_seq += 1
	ui_spawns += 1
	var v := run_check(ppu_bin, path, tmp_dir.path_join("open-%d-%d" % [OS.get_process_id(), _job_seq]))
	if v.has("error"):
		if v.get("missing", false):
			_toast("SNES art check needs ppu on PATH: opened as a plain image")
			_open_plain(path)
			return
		if v.get("reason") in UNLISTED:
			_open_plain(path)
			return
		_error(str(v["error"]))
		return
	if v.get("path") != "keep":  # the import would quantize it: it is not SNES art yet, so it opens plain
		_toast("%s is not SNES art yet: opened as a plain image. Project > Convert to SNES art… makes it SNES art." % v.get("source", path.get_file()))
		_open_plain(path)
		return
	var png := Png.read_png(path)
	if not png["indexed"]:
		_open_plain(path)
		return
	if png["error"] != "":
		_error("%s: %s" % [path.get_file(), png["error"]])
		return
	var err := _open_snes(path, v, png)
	if err != "":
		_error("%s: %s" % [path.get_file(), err])


func _open_plain(path: String) -> void:
	plain_opens += 1
	var img := Image.load_from_file(path)
	if img == null:
		_error("Can't load file '%s'." % path.get_file())
		return
	_last_plain = path  # Pixelorama's import dialog may open the tab later: _on_switched names it
	api.import.open_save_autoload().handle_loading_image(path, img)


## Builds the project from the check's verdict `v`; returns "" or why the PNG cannot be shown
## without losing indices.
func _open_snes(path: String, v: Dictionary, png: Dictionary) -> String:
	var t0 := Time.get_ticks_usec()
	var colors: PackedColorArray = png["plte"]
	if colors.is_empty():
		return "the PNG has no PLTE"
	var row := int(v["row"])
	var shifted := row == 256
	var first := 1 if shifted else 0
	var tmask := PackedByteArray()
	tmask.resize(256)
	var transparent := []
	for t in v["transparent"]:
		var i := int(t)
		if i >= 0 and i < 256:
			tmask[i] = 1
			transparent.append(i)
	var shown := display_colors(colors, first, tmask)
	if shown.is_empty():
		return "the palette repeats one colour too often to give every entry its own swatch"
	var w: int = png["w"]
	var h: int = png["h"]
	var idx: PackedByteArray = png["idx"]
	if not shifted and idx.find(255) >= 0:
		return "uses palette index 255, which only 256-colour art can hold"

	var project := Project.new([], path.get_file().get_basename(), Vector2i(w, h))
	var layer := PixelLayer.new(project)
	project.layers.append(layer)
	var frame := Frame.new()
	frame.cels.append(PixelCel.new(ImageExtended.create_custom(w, h, false, Image.FORMAT_RGBA8)))
	project.frames.append(frame)
	Global.projects.append(project)
	Global.tabs.current_tab = Global.tabs.get_tab_count() - 1  # switches Global.current_project

	# One swatch row = one sub-palette; a 256-colour block shows 16 wide.
	var width := mini(row, 16)
	var slots := colors.size() - first
	var pal_name: String = str(v.get("palette", path.get_file().get_basename()))
	var pal := Palette.new(pal_name, width, ceili(slots / float(width)))
	for k in slots:
		pal.colors[k] = Palette.PaletteColor.new(shown[k], k)
	Palettes.add_palette_as_project_palette(pal)
	Palettes.select_palette(pal.name)
	project.color_mode = Project.INDEXED_MODE

	var stored := idx.duplicate()
	if not shifted:
		for i in stored.size():
			stored[i] = stored[i] + 1
	var img: ImageExtended = frame.cels[0].get_image()
	img.indices_image.set_data(w, h, false, Image.FORMAT_R8, stored)
	img.convert_indexed_to_rgb()
	frame.cels[0].update_texture()

	var unit: Dictionary = v["unit"] if v.get("unit") is Dictionary else {}
	snes[project] = {
		"png": path, "kind": str(v.get("kind", "")), "name": str(v.get("source", path.get_file())), "row": row,
		"shifted": shifted, "tile": Vector2i(int(unit.get("width", 8)), int(unit.get("height", 8))),
		"palette": pal, "pname": pal.name, "colors": colors, "first": first, "display": shown, "reverted": 0, "base": idx,
		"tmask": tmask, "transparent": transparent, "pw": pal.width, "ph": pal.height,
		# only entry 0 transparent in 256-colour art: a "none" byte is already the PNG's index 0
		"plain_shift": shifted and transparent.size() <= 1,
		"verdict": v, "project": project,
	}
	pal.data_changed.connect(_on_palette_changed.bind(project))
	_apply_grid(project)
	_prune()
	timing["open_ms"] = (Time.get_ticks_usec() - t0) / 1000.0
	status_panel.visible = true
	_start_check()
	_refit.call_deferred(project)
	return ""


## The strip took its row after the camera fitted the art to the viewport: fit again.
func _refit(project: Project) -> void:
	await get_tree().process_frame
	if project == Global.current_project and Global.canvas.has_method("camera_zoom"):
		Global.canvas.camera_zoom()


## Closed projects leave the state.
func _prune() -> void:
	for p in snes.keys():
		if not Global.projects.has(p):
			snes.erase(p)
	for p in plain.keys():
		if not Global.projects.has(p):
			plain.erase(p)

func _apply_grid(project: Project) -> void:
	# The grid is global in Pixelorama, so it follows the current project.
	if snes.has(project):
		if _plain_grid == null:
			_plain_grid = [Global.grids[0].grid_size, Global.draw_grid]
		Global.grids[0].grid_size = snes[project]["tile"]
		Global.draw_grid = true
	elif _plain_grid != null:
		Global.grids[0].grid_size = _plain_grid[0]
		Global.draw_grid = _plain_grid[1]
		_plain_grid = null
	Global.grid_updated.emit()


# --- pixels -----------------------------------------------------------------

## What the worker needs: the stored bytes of the current frame's first cel, as a cheap
## copy-on-write handle, plus where that cel sits on the canvas.
func snapshot(project: Project) -> Dictionary:
	var cel: PixelCel = project.frames[project.current_frame].cels[0]
	var img: ImageExtended = cel.get_image()
	var s: Dictionary = snes[project]
	return {
		"data": img.indices_image.get_data(), "iw": img.get_width(), "ih": img.get_height(),
		"ox": cel.offset.x, "oy": cel.offset.y, "w": project.size.x, "h": project.size.y,
		"shifted": s["shifted"], "plain_shift": s["plain_shift"], "base": s["base"], "tmask": s["tmask"],
	}


## Canvas-sized PNG indices from a snapshot. Cels are sparse with offsets in 1.2.4, so what the
## cel does not cover is "none". A "none" pixel is written as the transparent entry the baseline held
## there, else index 0. Thread-safe.
static func canvas_indices(s: Dictionary) -> PackedByteArray:
	var d: PackedByteArray = s["data"]
	var w: int = s["w"]
	var h: int = s["h"]
	var iw: int = s["iw"]
	var ih: int = s["ih"]
	var ox: int = s["ox"]
	var oy: int = s["oy"]
	var shifted: bool = s["shifted"]
	var base: PackedByteArray = s["base"]
	var tmask: PackedByteArray = s["tmask"]
	if ox == 0 and oy == 0 and iw == w and ih == h:
		if s["plain_shift"]:
			return d
		var out := d.duplicate()
		for i in out.size():
			var v := out[i]
			if v == 0:
				out[i] = _transparent_at(base, i, tmask)
			elif not shifted:
				out[i] = v - 1
		return out
	var out := PackedByteArray()
	out.resize(w * h)
	for y in range(h):
		for x in range(w):
			var v := 0
			if x >= ox and x < ox + iw and y >= oy and y < oy + ih:
				v = d[(y - oy) * iw + (x - ox)]
			var i := y * w + x
			out[i] = _transparent_at(base, i, tmask) if v == 0 else (v if shifted else v - 1)
	return out


static func _transparent_at(base: PackedByteArray, i: int, tmask: PackedByteArray) -> int:
	return base[i] if i < base.size() and tmask[base[i]] == 1 else 0


## One swatch colour per slot, no two opaque ones alike, so a whole-image RGB -> index remap finds
## every index again. A transparent entry stays alpha 0 (Pixelorama's "none": remaps skip it);
## a repeated opaque colour moves one step in blue, then green, then red. [] when a colour repeats
## too often to tell apart. Slot k is PNG index k + first.
static func display_colors(colors: PackedColorArray, first: int, tmask: PackedByteArray) -> PackedColorArray:
	var seen := {}
	var out := PackedColorArray()
	for k in colors.size() - first:
		var c := colors[k + first]
		if k + first < 256 and tmask[k + first] == 1:
			out.append(Color8(c.r8, c.g8, c.b8, 0))
			continue
		var shown = _unique_colour(c, seen)
		if shown == null:
			return PackedColorArray()
		seen[shown.to_rgba32()] = true
		out.append(shown)
	return out


## `c` opaque, or the nearest colour no entry of `seen` (rgba32 -> true) shows: one step in blue, then
## green, then red, growing outwards. null when none is free.
static func _unique_colour(c: Color, seen: Dictionary) -> Variant:
	var rgba := [c.r8, c.g8, c.b8, 255]
	var shown := Color8(rgba[0], rgba[1], rgba[2], 255)
	var n := 0
	while seen.has(shown.to_rgba32()):
		n += 1
		if n > 3 * 255 * 2:
			return null
		var v := rgba.duplicate()
		var ch := 2 - (n - 1) % 3  # blue, green, red
		var d := (n + 5) / 6 * (1 if n % 2 == 1 else -1)
		v[ch] = clampi(v[ch] + d, 0, 255)
		shown = Color8(v[0], v[1], v[2], 255)
	return shown


# --- live check -------------------------------------------------------------

func _on_switched() -> void:
	var cur := Global.current_project
	if _last_plain != "" and not snes.has(cur) and not plain.has(cur) and cur.name == _last_plain.get_file():
		plain[cur] = _last_plain
		_last_plain = ""
	_prune()
	_prune.call_deferred()  # a closed tab leaves Global.projects only after this signal
	overlay.rects.clear()
	overlay.outline = Rect2()
	overlay.queue_redraw()
	_apply_grid(Global.current_project)
	status_panel.visible = snes.has(Global.current_project)
	if status_panel.visible:
		_show_checking()  # never the previous project's verdict, even while a check is still running
		_start_check()


func _show_checking() -> void:
	status_label.text = "Checking…"
	budget_label.text = ""
	status_panel.tooltip_text = ""


func _on_changed(project: Project) -> void:
	if snes.has(project) and not _guard and checks_enabled:
		_gen += 1
		timing["t_change"] = Time.get_ticks_usec()
		debounce.start()


## Snapshots on the main thread (a handle copy), then converts and runs the check on a worker.
func _start_check() -> void:
	var project := Global.current_project
	if not snes.has(project):
		return
	if _busy:
		_rerun = true
		return
	_rerun = false
	_lock_palette(project)
	status_label.text = "Checking…"
	status_panel.visible = true
	var snap := snapshot(project)
	var s: Dictionary = snes[project]
	_submit({
		"argv": check_args(s["png"], true, snap["w"], snap["h"]), "snap": snap, "done": _check_done,
		"gen": _gen, "project": project, "t0": Time.get_ticks_usec(),
	})


## Runs one ppu job on the worker thread; the caller has checked that none is running. A job is a Dictionary:
##   "argv"  ppu's arguments (Array)
##   "snap"  a snapshot whose canvas indices are fed to ppu on stdin
##   "done"  Callable(job), run on the UI thread once the worker is joined and _busy is false;
##           job["result"] is run_cmd's dictionary
## plus any keys the caller wants back in `done`.
func _submit(job: Dictionary) -> void:
	_busy = true
	_job_seq += 1
	job["seq"] = _job_seq
	job["t_start"] = Time.get_ticks_usec()
	if _thread.is_started():
		_thread.wait_to_finish()
	_thread.start(_work.bind(job))


## A job names its own files (process, instance, job counter): a result is never read from another job's.
func _work(job: Dictionary) -> void:
	var dir := tmp_dir.path_join("job-%d-%d-%d" % [OS.get_process_id(), get_instance_id(), job["seq"]])
	var stdin = null
	if job.has("snap"):
		stdin = canvas_indices(job["snap"])
		job["t_conv"] = Time.get_ticks_usec()
	job["result"] = run_cmd(ppu_bin, job["argv"], dir, stdin)
	job["t_done"] = Time.get_ticks_usec()
	_finish.call_deferred(job)


func _finish(job: Dictionary) -> void:
	if not _thread.is_started():  # the extension was disabled while the job ran
		return
	_thread.wait_to_finish()
	if not is_inside_tree():
		return
	_busy = false
	await job["done"].call(job)
	_pump()


## What waits for the worker: a check an edit asked for meanwhile
## (a stale check result is left to the debounce timer of the newer edit).
func _pump() -> void:
	if not _busy and _rerun:
		_start_check()

func _check_done(job: Dictionary) -> void:
	var project: Project = job["project"]
	if job["gen"] == _gen and project == Global.current_project and snes.has(project):
		var result := parse_check(job["result"])
		_apply(project, result)
		await RenderingServer.frame_post_draw
		var now := Time.get_ticks_usec()
		timing["convert_ms"] = (job["t_conv"] - job["t0"]) / 1000.0
		timing["worker_ms"] = (job["t_done"] - job["t0"]) / 1000.0
		timing["since_change_ms"] = (now - timing["t_change"]) / 1000.0 if timing.has("t_change") else -1.0
		last_result = result
		check_finished.emit(result)


## Shows the check's answer: overlay marks, status and budget in the strip, findings in its tooltip.
## Findings come only from the check.
func _apply(project: Project, r: Dictionary) -> void:
	overlay.rects.clear()
	overlay.outline = Rect2()
	status_panel.visible = true
	if r.has("error"):
		status_label.text = "Check failed: " + str(r["error"])
		budget_label.text = ""
		status_panel.tooltip_text = status_label.text
		overlay.queue_redraw()
		return
	var unit := Vector2(8, 8)
	if r.get("unit") is Dictionary:
		unit = Vector2(r["unit"].get("width", 8), r["unit"].get("height", 8))
	var findings: Array = r["findings"] if r.get("findings") is Array else []
	var lines := PackedStringArray()
	for f in findings:
		if typeof(f) != TYPE_DICTIONARY:
			continue
		var where := " [%d,%d]" % [f["x"], f["y"]] if f.has("x") and f.has("y") else ""
		lines.append("%s%s: %s" % [f.get("rule", "finding"), where, str(f.get("message", f.get("rule", "finding"))).left(90)])
		if f.has("x") and f.has("y"):
			overlay.rects.append(Rect2(Vector2(f["x"], f["y"]) * unit, unit))
		elif f.get("rule") == "plte":
			overlay.outline = Rect2(Vector2.ZERO, Vector2(project.size))
	overlay.queue_redraw()
	if lines.is_empty():
		status_label.text = "No findings: the art would import."
	else:
		status_label.text = "%d finding%s" % [lines.size(), "" if lines.size() == 1 else "s"]
	var b = r.get("budgets")
	budget_label.text = "Tiles: -    Palettes: -"
	if b is Dictionary:
		budget_label.text = "Tiles: %d / %d    Palettes: %d / %d" % [
			b["tiles"]["used"], b["tiles"]["max"], b["palettes"]["used"], b["palettes"]["max"]]
	var tip := lines.slice(0, 12)
	if lines.size() > 12:
		tip.append("… and %d more" % (lines.size() - 12))
	status_panel.tooltip_text = "\n".join(tip)


# --- palette ------------------------------------------------------------------

const LOCK_NOTE := "The palette is locked for SNES art: put back."

## Open as SNES art and its tab still there (a closed background tab sends no project_switched, so snes alone lies).
func _is_live(project: Project) -> bool:
	return snes.has(project) and Global.projects.has(project)


## The palette is locked whole, and never written: any change (a colour, undo, redo, order, size, a global copy,
## another palette picked) puts it back to its display colours and size and selects the project's own palette
## again. Colours are edited in the .gpl and taken when the PNG is reopened. Pixelorama remaps the canvas from
## RGB after flips, moves and effects, which only keeps every index while the swatches are unique colours
## (display_colors). `Palette.edit` (a resize) raises no palette signal: the panel's update signal and every
## check start look again.
func _on_palette_changed(project: Project) -> void:
	if not _guard and snes.has(project):
		_lock_palette(project)


func _on_palette_panel_updated() -> void:
	_on_palette_changed(Global.current_project)


func _on_palette_selected(_name: String) -> void:
	_on_palette_changed(Global.current_project)


## True when the palette is listed once, under the name it opened with (an undo of Pixelorama's palette delete
## lists the same palette again under "<name> copy").
func _registered(project: Project, s: Dictionary) -> bool:
	var pal: Palette = s["palette"]
	if pal.name != s["pname"] or project.palettes.get(s["pname"]) != pal:
		return false
	for k in project.palettes:
		if k != s["pname"] and project.palettes[k] == pal:
			return false
	return true


## Puts the palette back to its display colours and size, with the toast, unless it already is.
func _lock_palette(project: Project) -> void:
	var s: Dictionary = snes[project]
	var pal: Palette = s["palette"]
	var shown: PackedColorArray = s["display"]
	# a background tab's palette is updated in place: only the current project owns the selection
	var current := project == Global.current_project
	var same: bool = (Palettes.current_palette == pal or not current) and _registered(project, s) and pal.width == s["pw"] and pal.height == s["ph"] \
			and pal.colors.size() == shown.size()
	for k in shown.size() if same else 0:
		var c = pal.get_color(k)
		same = same and c != null and c.to_rgba32() == shown[k].to_rgba32()
	if same:
		return
	_guard = true
	var pname: String = s["pname"]
	if not _registered(project, s):
		for k in project.palettes.keys():
			if project.palettes[k] == pal:
				project.palettes.erase(k)
		project.palettes[pname] = pal
	if current and (Palettes.current_palette != pal or project.project_current_palette_name != pname):
		# by object: a deleted project palette is no longer listed, and a global one may share its name;
		# select_palette's last_palette write is done here too (the palette_selected signal clears the selection)
		project.project_current_palette_name = pname
		Palettes.current_palette = pal
		Global.config_cache.set_value("data", "last_palette", pname)
		Palettes.palette_selected.emit(pname)
	pal.edit(pname, s["pw"], s["ph"], pal.comment)
	for k in pal.colors.keys():
		if k >= shown.size():
			pal.colors.erase(k)
	for k in shown.size():
		pal.colors[k] = Palette.PaletteColor.new(shown[k], k)
	# the last write raises data_changed: Pixelorama redraws the canvas RGB from the indices, once
	pal.set_color(0, shown[0])
	_guard = false
	s["reverted"] += 1
	_toast(LOCK_NOTE)


# --- tile rows ----------------------------------------------------------------

## Why the row commands cannot run on `project` now, or "": one layer and one frame, no 8 bpp or Mode 7 art
## (one 256-colour block has no rows).
func _command_refusal(project: Project) -> String:
	if not _is_live(project):
		return "this project was not opened as SNES art (open a PNG that belongs to a project)"
	if snes[project]["row"] == 256:
		return "tile rows work on 4 and 16 colour rows, not on 8 bpp or Mode 7 art (one 256-colour block has no rows)."
	if project.layers.size() != 1 or project.frames.size() != 1:
		return "this works on art of one layer and one frame (this has %d layers, %d frames): flatten it first." % [
			project.layers.size(), project.frames.size()]
	var cel: PixelCel = project.frames[0].cels[0]
	if cel.offset != Vector2i.ZERO or cel.get_image().get_size() != project.size:
		return "the art's cel does not cover the canvas exactly: this works on a whole-canvas cel."
	return ""


## True (message shown) when the commands cannot run on `project` now.
func _refused(project: Project, title: String) -> bool:
	var why := _command_refusal(project)
	if why != "":
		_error("%s: %s" % [title, why])  # no command_done: another command may be awaiting its own
	return why != ""


## ppu's own words for a refused run (its answer's `error`), else the generic failure line.
func _why(r: Dictionary) -> String:
	var t: String = r["err"].strip_edges()
	return t if r["code"] in [1, 2] and t != "" else failure_text(r)


## Runs `ppu art <args>` on the current canvas indices as one job, once the worker is free; `done` gets the
## job. Returns false (message shown) without running when `project` cannot take it.
func _command(project: Project, title: String, args: Array, done: Callable, extra := {}) -> bool:
	while _busy:  # a check: the worker is one
		await get_tree().process_frame
	if _refused(project, title):
		return false
	var snap := snapshot(project)
	var job := {
		"argv": ["art"] + args, "snap": snap, "done": done,
		"project": project, "title": title, "was": canvas_indices(snap),
	}
	job.merge(extra)
	_submit(job)
	return true


func _size_args(project: Project) -> Array:
	return ["--width", project.size.x, "--height", project.size.y]


## Moves the tiles or cells the selection touches to the row of the left colour. Awaitable: -> ok.
func set_tile_row(project: Project) -> bool:
	var title := "Set tile row"
	while _busy:
		await get_tree().process_frame
	if _refused(project, title):
		return false
	if not project.has_selection:
		_error("%s: select the tiles or cells to move first (the rectangle select tool)." % title)
		return false
	var s: Dictionary = snes[project]
	var picked := Palettes.left_selected_color
	if picked < 0:
		_error("%s: pick a colour of the row you want as the left colour first." % title)
		return false
	var rect := project.selection_map.get_selection_rect(project).intersection(Rect2i(Vector2i.ZERO, project.size))
	if rect.size.x <= 0 or rect.size.y <= 0:
		_error("%s: the selection is outside the art." % title)
		return false
	var row: int = (picked + s["first"]) / maxi(1, s["row"])
	var rect_arg := "%d,%d,%d,%d" % [rect.position.x, rect.position.y, rect.size.x, rect.size.y]
	var args := ["rows", s["png"], "--set", row, "--rect", rect_arg] + _size_args(project)
	if not await _command(project, title, args, _rows_done, {"note": "Set tile row %d." % row}):
		return false
	return await command_done


func fix_tile_rows(project: Project) -> bool:
	while _busy:
		await get_tree().process_frame
	if _refused(project, "Fix tile rows"):
		return false
	var args := ["rows", snes[project]["png"], "--fix"] + _size_args(project)
	if not await _command(project, "Fix tile rows", args, _rows_done, {"note": "Fixed tile rows."}):
		return false
	return await command_done


## The row commands' result: ppu's index bytes laid over the cel as one undo step (none when nothing moved).
func _rows_done(job: Dictionary) -> void:
	var project: Project = job["project"]
	var r: Dictionary = job["result"]
	var ok := false
	if not _is_live(project):
		pass  # the tab closed while ppu ran
	elif r["code"] != 0:
		_error("%s: %s" % [job["title"], _why(r)])
	elif r["bytes"].size() != job["was"].size():
		_error("%s: ppu gave %d bytes for %d pixels: nothing changed." % [job["title"], r["bytes"].size(), job["was"].size()])
	elif canvas_indices(snapshot(project)) != job["was"]:
		_error("%s: the art changed while ppu ran: nothing changed, try again." % job["title"])
	else:
		ok = true
		if r["bytes"] != job["was"]:
			_lay_indices(project, job["title"], r["bytes"])
			snes[project]["base"] = r["bytes"]  # what a save would make of it: a later remap restores the new row's entry 0
		var lines := PackedStringArray(r["notes"])
		# `--fix` lists the tiles it could not move in its notes: the status carries them
		var note: String = job["note"] if lines.is_empty() else job["note"] + " " + " ".join(lines.slice(0, 3)) + (" …" if lines.size() > 3 else "")
		_toast(note)
	command_done.emit(ok)


## Lays canvas-sized PNG indices over the art's cel in one Pixelorama undo step, the way its own tools do.
func _lay_indices(project: Project, title: String, canvas: PackedByteArray) -> void:
	var cel: PixelCel = project.frames[0].cels[0]
	var img: ImageExtended = cel.get_image()
	var stored := canvas.duplicate()
	for i in stored.size():
		stored[i] += 1  # not 256-colour art: stored byte = PNG index + 1
	var undo_data := {}
	project.serialize_cel_undo_data([cel], undo_data)
	_guard = true
	img.indices_image.set_data(project.size.x, project.size.y, false, Image.FORMAT_R8, stored)
	img.convert_indexed_to_rgb()
	cel.update_texture()
	_guard = false
	var redo_data := {}
	project.serialize_cel_undo_data([cel], redo_data)
	project.undo_redo.create_action(title)
	project.deserialize_cel_undo_data(redo_data, undo_data)
	project.undo_redo.add_do_method(Global.undo_or_redo.bind(false, project.current_frame, 0))
	project.undo_redo.add_undo_method(Global.undo_or_redo.bind(true, project.current_frame, 0))
	project.undo_redo.commit_action()
	project.has_changed = true


# --- New SNES art -------------------------------------------------------------

## The dialog only collects fields; `ppu art new` writes every file and refuses what is not legal, so its
## message is the answer.
func new_art_dialog() -> void:
	var d := ConfirmationDialog.new()
	d.title = "New SNES art"
	d.ok_button_text = "Create"
	var grid := GridContainer.new()
	grid.columns = 2
	d.add_child(grid)
	var start := ""
	if snes.has(Global.current_project):
		start = snes[Global.current_project]["png"].get_base_dir().path_join("new_art.png")
	var png := _path_row(grid, _label("PNG (inside a project)"), "png", start, FileDialog.FILE_MODE_SAVE_FILE, "*.png ; PNG image")
	var kind := OptionButton.new()
	for k in ["bg", "sheet", "obj", "m7"]:
		kind.add_item(k)
	grid.add_child(_label("Kind"))
	grid.add_child(kind)
	var size := _field(grid, "Size (WxH)", "128x64")
	var bpp := OptionButton.new()
	for b in ["kind's default", "2", "4", "8"]:
		bpp.add_item(b)
	grid.add_child(_label("Bit depth"))
	grid.add_child(bpp)
	var cell := _field(grid, "Obj cell size (0: default)", "0")
	# one palette choice, as Aseprite's radio: only the chosen field is enabled and only its value is passed to ppu
	var group := ButtonGroup.new()
	var new_pick := _pick("pick_new", "New palette named", group)
	new_pick.button_pressed = true
	var new_pal := _path_row(grid, new_pick, "new_palette", "")
	var old_pick := _pick("pick_old", "Existing .gpl", group)
	var pal := _path_row(grid, old_pick, "gpl", "", FileDialog.FILE_MODE_OPEN_FILE, "*.gpl ; GIMP palette")
	var choose := func():
		new_pal.editable = new_pick.button_pressed
		pal.editable = old_pick.button_pressed
		pal.get_parent().get_node("browse_gpl").disabled = not old_pick.button_pressed
	new_pick.toggled.connect(func(_on): choose.call())
	choose.call()
	d.confirmed.connect(func():
		var f := {"png": png.text.strip_edges(), "kind": kind.get_item_text(kind.selected), "size": size.text.strip_edges()}
		if bpp.selected > 0:
			f["bpp"] = bpp.get_item_text(bpp.selected)
		if cell.text.to_int() > 0:
			f["cell"] = cell.text.to_int()
		if old_pick.button_pressed:
			f["palette"] = pal.text.strip_edges()
		else:
			f["new_palette"] = new_pal.text.strip_edges()
		new_art(f))
	d.visibility_changed.connect(func(): if not d.visible: d.queue_free.call_deferred())
	api.dialog.get_dialogs_parent_node().add_child(d)
	d.popup_centered()


func _label(text: String) -> Label:
	var l := Label.new()
	l.text = text
	return l


func _pick(node_name: String, text: String, group: ButtonGroup) -> CheckBox:
	var c := CheckBox.new()
	c.name = node_name
	c.text = text
	c.button_group = group
	return c


## `label` and a text field in the grid; with a file `mode` also a Browse… button (its own FileDialog, filling the
## field). Both are named after the field (`<name>`, `browse_<name>`) so a test finds them.
func _path_row(grid: Node, label: Control, field: String, value: String, mode := -1, filter := "") -> LineEdit:
	grid.add_child(label)
	var row := HBoxContainer.new()
	grid.add_child(row)
	var e := LineEdit.new()
	e.name = field
	e.text = value
	e.custom_minimum_size.x = 300
	e.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_child(e)
	if mode >= 0:
		var b := Button.new()
		b.name = "browse_" + field
		b.text = "Browse…"
		b.pressed.connect(func():
			var d := FileDialog.new()
			d.file_mode = mode
			d.access = FileDialog.ACCESS_FILESYSTEM
			d.filters = PackedStringArray([filter])
			d.use_native_dialog = false
			var cur := e.text.strip_edges()
			if cur != "":
				d.current_dir = cur.get_base_dir()
			d.file_selected.connect(func(path: String):
				e.text = path
				e.text_submitted.emit(path))  # as if typed: dialogs that refill from the path see it
			d.visibility_changed.connect(func(): if not d.visible: d.queue_free.call_deferred())
			api.dialog.get_dialogs_parent_node().add_child(d)
			d.popup_centered_ratio(0.6))
		row.add_child(b)
	return e


func _field(grid: Node, text: String, value: String) -> LineEdit:
	var e := LineEdit.new()
	e.text = value
	e.custom_minimum_size.x = 260
	grid.add_child(_label(text))
	grid.add_child(e)
	return e


## `ppu art new` from `fields` (png, kind, size; bpp, cell; one of palette or new_palette), then the PNG opens as
## SNES art. ppu's refusal is shown as it is and nothing is written. Awaitable: -> ok.
func new_art(fields: Dictionary) -> bool:
	while _busy:
		await get_tree().process_frame
	var args := ["art", "new", str(fields.get("png", "")), "--kind", str(fields.get("kind", "")), "--size", str(fields.get("size", ""))]
	for k in ["bpp", "cell"]:
		if fields.has(k):
			args.append_array(["--" + k, str(fields[k])])
	if fields.get("palette", "") != "":
		args.append_array(["--palette", fields["palette"]])
	elif fields.get("new_palette", "") != "":
		args.append_array(["--new-palette", fields["new_palette"]])
	_submit({"argv": args, "done": _new_art_done, "png": fields.get("png", "")})
	return await command_done


func _new_art_done(job: Dictionary) -> void:
	var r: Dictionary = job["result"]
	var ok := false
	if r["code"] != 0:
		_error("New SNES art: " + _why(r))
	else:
		var count := Global.projects.size()
		last_error = ""
		open_png(job["png"])
		ok = Global.projects.size() == count + 1 and snes.has(Global.current_project)
		if ok:
			_toast("Created %s." % job["png"].get_file())
		elif last_error == "":
			_error("ppu made %s but it did not open as SNES art" % job["png"].get_file())
	command_done.emit(ok)


# --- Convert to SNES art ------------------------------------------------------

## The flags `ppu art convert` reads per kind (cli.md); the plugin passes no other and decides nothing else.
const CONVERT_READS := {
	"bg": ["bpp", "dither", "dither_strength", "alpha_threshold", "max_palettes"],
	"sheet": ["bpp", "dither", "dither_strength", "alpha_threshold", "max_palettes"],
	"obj": ["bpp", "cell", "dither", "dither_strength", "alpha_threshold"],
	"m7": ["bpp"],
}
const DITHERS := ["none", "bayer", "diffusion"]

## What `ppu art check` says of `png`'s listing: {"kind", "options"} ({} when no project lists it, or ppu cannot say).
func _listed(png: String) -> Dictionary:
	_job_seq += 1
	var v := run_check(ppu_bin, png, tmp_dir.path_join("conv-%d-%d" % [OS.get_process_id(), _job_seq]))
	return v if v.has("kind") and not v.has("error") else {}


## Collects the fields; `ppu art convert` does the work and refuses what is not legal. Only fields the kind reads
## are enabled, and only those that differ from what ppu.json lists (or the converter's default) are passed.
func convert_dialog() -> void:
	var project := Global.current_project
	var d := ConfirmationDialog.new()
	d.title = "Convert to SNES art"
	d.ok_button_text = "Convert"
	var grid := GridContainer.new()
	grid.columns = 2
	d.add_child(grid)
	var start: String = snes[project]["png"] if snes.has(project) else plain.get(project, "")
	var png := _path_row(grid, _label("PNG (inside a project)"), "png", start, FileDialog.FILE_MODE_SAVE_FILE, "*.png ; PNG image")
	var kind := OptionButton.new()
	kind.name = "kind"
	for k in CONVERT_READS:
		kind.add_item(k)
	grid.add_child(_label("Kind"))
	grid.add_child(kind)
	var bpp := OptionButton.new()
	bpp.name = "bpp"
	for b in ["kind's default", "2", "4", "8"]:
		bpp.add_item(b)
	grid.add_child(_label("Bit depth"))
	grid.add_child(bpp)
	var cell := _field(grid, "Obj cell size", "8")
	cell.name = "cell"
	var dither := OptionButton.new()
	dither.name = "dither"
	for x in DITHERS:
		dither.add_item(x)
	grid.add_child(_label("Dither"))
	grid.add_child(dither)
	var strength := _field(grid, "Dither strength (0-100)", "50")
	strength.name = "dither_strength"
	var alpha := _field(grid, "Alpha threshold (0-255)", "128")
	alpha.name = "alpha_threshold"
	var maxpal := _field(grid, "Max palettes (1-8)", "8")
	maxpal.name = "max_palettes"
	var pal := _field(grid, "New palette named", "")
	pal.name = "new_palette"
	var fields := {"bpp": bpp, "cell": cell, "dither": dither, "dither_strength": strength, "alpha_threshold": alpha, "max_palettes": maxpal}
	var pre := {}  # what the fields held when filled: a field is passed only when it differs
	var listed := {}
	var filled_for := "\u0000"  # the path the fields were last filled for: they refill only when it changes
	var show := func():
		var reads: Array = CONVERT_READS[kind.get_item_text(kind.selected)]
		for f in fields:
			if fields[f] is OptionButton:
				fields[f].disabled = not (f in reads)
			else:
				fields[f].editable = f in reads
	var value := func(f: String) -> String:
		var w = fields[f]
		return w.get_item_text(w.selected) if w is OptionButton else w.text.strip_edges()
	var fill := func():
		filled_for = png.text.strip_edges()
		listed = _listed(filled_for) if filled_for != "" else {}
		var o: Dictionary = listed.get("options", {})
		if listed.has("kind") and listed["kind"] in CONVERT_READS:
			kind.select(CONVERT_READS.keys().find(listed["kind"]))
		kind.disabled = not listed.is_empty()
		var bd := str(int(o.get("bit_depth", 0)))
		bpp.select(maxi(0, ["0", "2", "4", "8"].find(bd)))
		cell.text = str(int(o.get("cell_size", 8)))
		dither.select(maxi(0, DITHERS.find(str(o.get("dither", "none")))))
		strength.text = str(int(o.get("dither_strength", 50)))
		alpha.text = str(int(o.get("alpha_threshold", 128)))
		maxpal.text = str(int(o.get("max_palettes", 8)))
		if png.text.strip_edges() != "":
			pal.text = png.text.strip_edges().get_file().get_basename() + "_pal"
		show.call()
		for f in fields:
			pre[f] = value.call(f)
	kind.item_selected.connect(func(_i): show.call())
	var refill := func(): if png.text.strip_edges() != filled_for: fill.call()
	png.text_submitted.connect(func(_t): refill.call())
	png.focus_exited.connect(func(): refill.call())
	fill.call()
	d.confirmed.connect(func():
		var f := {"png": png.text.strip_edges(), "new_palette": pal.text.strip_edges()}
		if listed.is_empty():
			f["kind"] = kind.get_item_text(kind.selected)
		for k in CONVERT_READS[kind.get_item_text(kind.selected)]:
			var v: String = value.call(k)
			if v != pre[k] and not (k == "bpp" and bpp.selected == 0):
				f[k] = v
		convert_art(f))
	d.visibility_changed.connect(func(): if not d.visible: d.queue_free.call_deferred())
	api.dialog.get_dialogs_parent_node().add_child(d)
	d.popup_centered()


## The tab's first layer, current frame, on a canvas-sized RGBA image.
func _tab_pixels(project: Project) -> Image:
	var cel: PixelCel = project.frames[project.current_frame].cels[0]  # callers check `is PixelCel`
	var src: Image = cel.get_image().duplicate()
	src.convert(Image.FORMAT_RGBA8)
	var img := Image.create(project.size.x, project.size.y, false, Image.FORMAT_RGBA8)
	img.blit_rect(src, Rect2i(Vector2i.ZERO, src.get_size()), cel.offset)
	return img


## Saves the tab's pixels to `png` unless the file already holds them; "" or the error.
func _save_tab(project: Project, png: String) -> String:
	var img := _tab_pixels(project)
	var cur := Image.load_from_file(png) if FileAccess.file_exists(png) else null
	if cur != null:
		cur.convert(Image.FORMAT_RGBA8)
		if cur.get_size() == img.get_size() and cur.get_data() == img.get_data():
			return ""
	return "" if img.save_png(png) == OK else "cannot write %s" % png


## `ppu art convert` on the tab's art, then the PNG opens as SNES art. `fields`: png, new_palette; kind (an unlisted
## PNG); bpp, cell, dither, dither_strength, alpha_threshold, max_palettes (as the dialog found them differing).
## ppu's refusal is shown as it is. The tab's pixels are saved to the PNG first (a SNES art tab is not: ppu refuses
## it). Awaitable: -> ok.
func convert_art(fields: Dictionary) -> bool:
	while _busy:
		await get_tree().process_frame
	var project := Global.current_project
	var png: String = str(fields.get("png", ""))
	if png == "":
		_error("Convert to SNES art: choose the PNG to convert (inside a project)")
		command_done.emit.call_deferred(false)
		return false
	if not snes.has(project):
		if not project.frames[project.current_frame].cels[0] is PixelCel:
			_error("Convert to SNES art: the first layer is not a pixel layer")
			command_done.emit.call_deferred(false)  # deferred: a caller that awaits after the call still hears it
			return false
		if plain.get(project, "") != png and FileAccess.file_exists(png):  # never overwrite another file with this tab
			_error("Convert to SNES art: %s exists and is not this tab's PNG: open it and convert it there" % png)
			command_done.emit.call_deferred(false)
			return false
		var err := _save_tab(project, png)
		if err != "":
			_error("Convert to SNES art: " + err)
			command_done.emit.call_deferred(false)
			return false
	var args := ["art", "convert", png]
	if fields.has("kind"):
		args.append_array(["--kind", str(fields["kind"])])
	for k in ["bpp", "cell", "dither", "dither_strength", "alpha_threshold", "max_palettes"]:
		if fields.has(k):
			args.append_array(["--" + k.replace("_", "-"), str(fields[k])])
	if str(fields.get("new_palette", "")) != "":
		args.append_array(["--new-palette", str(fields["new_palette"])])
	_submit({"argv": args, "done": _convert_done, "png": png, "project": project})
	return await command_done


func _convert_done(job: Dictionary) -> void:
	var r: Dictionary = job["result"]
	var j: Dictionary = r["answer"]
	var ok := false
	if r["code"] != 0:
		_error("Convert to SNES art: " + _why(r))
	elif not j.get("ok", false):
		_error("Convert to SNES art: ppu gave no result (exit 0): " + r["out"].left(600))
	else:
		var old: Project = job["project"]
		last_error = ""
		open_png(job["png"])
		ok = snes.has(Global.current_project) and snes[Global.current_project]["png"] == job["png"]
		if ok:
			var made := Global.current_project
			var left := ""
			if Global.projects.has(old) and not snes.has(old):  # the plain tab it came from
				if old.layers.size() == 1 and old.frames.size() == 1:
					old.has_changed = false
					Global.tabs._on_tab_close_pressed(Global.projects.find(old))
					Global.tabs.current_tab = Global.projects.find(made)
				else:  # only its first layer of this frame was converted: the rest stays in its tab
					left = "\nThe tab you converted from has more than one layer or frame and stays open; only its first layer of the current frame was converted."
			last_convert = "%s is SNES art now.\nColours: %d\nTiles: %d / %d    Palettes: %d / %d\nThe original is in %s" % [
				job["png"].get_file(), j["colours"], j["tiles"]["used"], j["tiles"]["max"], j["palettes"]["used"], j["palettes"]["max"], j["backup"]]
			for n in j.get("notes", []):
				last_convert += "\n" + str(n)
			last_convert += left
			var info := AcceptDialog.new()
			info.title = "Converted to SNES art"
			info.dialog_text = last_convert
			info.visibility_changed.connect(func(): if not info.visible: info.queue_free.call_deferred())
			api.dialog.get_dialogs_parent_node().add_child(info)
			info.popup_centered()
		elif last_error == "":
			_error("ppu converted %s but it did not open as SNES art" % job["png"].get_file())
	command_done.emit(ok)


# --- save and export ----------------------------------------------------------

## Writes the indexed PNG (default: the file it was opened from). Returns "" or the error.
func write_snes(project: Project, path: String) -> String:
	var s: Dictionary = snes[project]
	return Png.write_png(path, project.size.x, project.size.y, canvas_indices(snapshot(project)), s["colors"], s["tmask"])


func save_snes(project: Project, path := "") -> String:
	if not snes.has(project):
		_error("Save SNES art: this project was not opened as SNES art (open a PNG that belongs to a project)")
		return "not SNES art"
	var target: String = path if path != "" else snes[project]["png"]
	var err := write_snes(project, target)
	if err != "":
		_error("Save SNES art: " + err)
		return err
	if path == "":
		project.has_changed = false
		snes[project]["base"] = canvas_indices(snapshot(project))
	_toast("Saved " + target.get_file())
	return ""


## The "SNES indexed PNG" export. May run on Pixelorama's export thread, so no scene access.
func export_indexed(details: Dictionary) -> bool:
	var project: Project = details["project"]
	if not snes.has(project):
		(func(): _error("SNES indexed PNG: this project was not opened as SNES art")).call_deferred()
		return false
	var paths: Array = details["export_paths"]
	if paths.size() != 1:
		(func(): _error("SNES indexed PNG exports one image: choose the current frame")).call_deferred()
		return false
	var err := write_snes(project, paths[0])
	if err != "":
		(func(): _error("SNES indexed PNG: " + err)).call_deferred()
		return false
	return true
