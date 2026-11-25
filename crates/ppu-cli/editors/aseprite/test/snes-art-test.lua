-- Batch test: aseprite -b --script test/snes-art-test.lua   (PPU_BIN = a built ppu, see run.sh)
-- Builds a tiny project in a temp folder, opens its PNGs through SnesArt and asserts what the
-- plugin promises. Fails (error exit) if snes-art.lua is absent or any promise breaks.
local here = app.fs.filePath(app.params.script or debug.getinfo(1, "S").source:sub(2))
dofile(app.fs.joinPath(app.fs.filePath(here), "snes-art.lua"))
assert(SnesArt, "snes-art.lua did not define SnesArt")
SnesArt.watch_saves() -- the save hook init() installs (init also opens a panel, and a batch run has no Dialog)

local function check(cond, msg) if not cond then error("FAIL: " .. msg, 2) end print("ok   " .. msg) end

local root = app.fs.joinPath(app.fs.tempPath, "ppu-art-test")
os.execute('rm -rf "' .. root .. '"')
app.fs.makeAllDirectories(app.fs.joinPath(root, "palettes"))

local function x5(v) return (v << 3) | (v >> 2) end -- RGB555 expanded as ppu does
local function rgb(i) return x5(i % 32), x5((i * 3) % 32), x5((i * 7) % 32) end

-- palette: 2 rows of 16; entry 0 of each row is the row's transparent colour
local gpl = { "GIMP Palette", "Name: bg_pal", "Columns: 16", "#" }
for i = 0, 31 do local r, g, b = rgb(i); gpl[#gpl + 1] = string.format("%3d %3d %3d\tc%d", r, g, b, i) end
local f = io.open(app.fs.joinPath(root, "palettes/bg_pal.gpl"), "w"); f:write(table.concat(gpl, "\n"), "\n"); f:close()
-- a 128-colour Mode 7 palette: an EXTBG PNG holds it mirrored to 256, entry 128 transparent
local m7gpl = { "GIMP Palette", "Name: m7_pal", "Columns: 256", "#" }
for i = 0, 127 do local r, g, b = rgb(i); m7gpl[#m7gpl + 1] = string.format("%3d %3d %3d\tc%d", r, g, b, i) end
f = io.open(app.fs.joinPath(root, "palettes/m7_pal.gpl"), "w"); f:write(table.concat(m7gpl, "\n"), "\n"); f:close()

local function write_manifest(sources)
  local m = io.open(app.fs.joinPath(root, "ppu.json"), "w")
  m:write(json.encode { title = "t", files = { "main.luau" }, sources = sources }); m:close()
end
write_manifest {
  { name = "bg_pal", kind = "palette", file = "palettes/bg_pal.gpl", options = { bit_depth = 4 } },
  { name = "bg", kind = "bg", file = "art/bg.png", options = { bit_depth = 4, tile_size = 8, palette = "bg_pal" } },
  { name = "hero", kind = "obj", file = "art/hero.png", options = { bit_depth = 4, cell_size = 16, palette = "bg_pal" } },
  { name = "ghost", kind = "bg", file = "art/ghost.png", options = { palette = "nope" } },
  { name = "weird", kind = "frobnicate", file = "art/weird.png" },
}
app.fs.makeAllDirectories(app.fs.joinPath(root, "art"))

-- A 32x32 indexed PNG whose PLTE is the .gpl but fully opaque (as a plain editor would save it).
-- mixed = list of {tx,ty}: tile gets one pixel of row 1 inside row 0.
local function make_png(name, mixed, mode)
  local spr = Sprite(32, 32, mode or ColorMode.INDEXED)
  if (mode or ColorMode.INDEXED) == ColorMode.INDEXED then
    local pal = Palette(32)
    for i = 0, 31 do local r, g, b = rgb(i); pal:setColor(i, Color { r = r, g = g, b = b }) end
    spr:setPalette(pal)
  end
  local img = spr.cels[1].image
  for y = 0, 31 do for x = 0, 31 do img:putPixel(x, y, 1 + (x + y) % 15) end end
  for _, t in ipairs(mixed or {}) do img:putPixel(t[1] * 8 + 3, t[2] * 8 + 3, 17) end
  local path = app.fs.joinPath(root, "art/" .. name .. ".png")
  spr:saveAs(path)
  spr:close()
  return path
end
local bg = make_png("bg", { { 2, 1 } })
local hero = make_png("hero", {})
local ghost = make_png("ghost", {})
local weird = make_png("weird", {})
local rgbpng = make_png("rgb", {}, ColorMode.RGB)
local stray = make_png("stray", {})
local m7png = make_png("m7", {})
-- rgb.png is listed too, so that the full-colour refusal is exercised
write_manifest {
  { name = "bg_pal", kind = "palette", file = "palettes/bg_pal.gpl", options = { bit_depth = 4 } },
  { name = "bg", kind = "bg", file = "art/bg.png", options = { bit_depth = 4, tile_size = 8, palette = "bg_pal" } },
  { name = "hero", kind = "obj", file = "art/hero.png", options = { bit_depth = 4, cell_size = 16, palette = "bg_pal" } },
  { name = "ghost", kind = "bg", file = "art/ghost.png", options = { palette = "nope" } },
  { name = "weird", kind = "frobnicate", file = "art/weird.png" },
  { name = "rgb", kind = "bg", file = "art/rgb.png" }, -- naming bg_pal would make every palette sync refuse it
  { name = "m7_pal", kind = "palette", file = "palettes/m7_pal.gpl", options = { bit_depth = 8 } },
  { name = "m7", kind = "m7", file = "art/m7.png", options = { bit_depth = 8, extbg = true, palette = "m7_pal" } },
}

-- ---- opening a project PNG ----
local spr = Sprite { fromFile = bg }
local st, msg = SnesArt.open(spr)
check(st, "bg.png opens as SNES art" .. tostring(msg))
check(spr.colorMode == ColorMode.INDEXED, "colour mode is INDEXED")
local pal = spr.palettes[1]
check(#pal == 32, "palette has the .gpl's 32 entries")
local rgb_ok = true
for i = 0, 31 do
  local r, g, b = rgb(i)
  local c = pal:getColor(i)
  if c.red ~= r or c.green ~= g or c.blue ~= b then rgb_ok = false end
end
check(rgb_ok, "palette RGB equals the .gpl for every entry")
check(pal:getColor(0).alpha == 0 and pal:getColor(16).alpha == 0, "alpha 0 at entries 0 and 16")
check(pal:getColor(1).alpha == 255 and pal:getColor(17).alpha == 255, "other entries stay opaque")
check(spr.gridBounds.width == 8 and spr.gridBounds.height == 8, "grid is the 8x8 tile size")
-- the setup replaced the opaque PLTE, so the open starts the live check at once instead of showing the file's verdict
check(st.running and st.result == nil, "open starts a live check (the file verdict would describe the old palette)")
local function wait(s) for _ = 1, 200 do if not s.running then return true end SnesArt.poll(s) os.execute("sleep 0.05") end end
check(wait(st), "the live check lands")
local first_mixed, stale
for _, f in ipairs(st.result and st.result.findings or {}) do
  if f.rule == "mixed-rows" and f.x == 2 and f.y == 1 then first_mixed = true end
  if f.rule == "not-snes-art" then stale = true end
end
check(first_mixed and not stale and st.status == "1 finding", "a plain-editor PNG opens with no stale not-snes-art finding: " .. tostring(st.status))
check(spr.undoHistory.undoSteps == 1, "opening is one undo step, named Open as SNES art (" .. spr.undoHistory.undoSteps .. ")")

-- ---- a colour edit only arms the live check: no revert, no write ----
app.transaction("test edit", function() spr.palettes[1]:setColor(16, Color { r = 1, g = 2, b = 3, a = 255 }) end)
check(spr.palettes[1]:getColor(16).rgbaPixel == Color { r = 1, g = 2, b = 3, a = 255 }.rgbaPixel, "an edit to a transparent entry stays: nothing puts it back")
check(st.debounce.isRunning, "a change arms the debounce timer (the only stroke-path work)")
app.undo()
check(spr.palettes[1]:getColor(16).alpha == 0, "undo puts the entry back (Aseprite's own undo)")

-- ---- the real check through the real CLI, stdin path ----
local res = SnesArt.check_sync(st)
check(not res.error, "check ran: " .. tostring(res.error))
check(not res.ok and #res.findings == 1, "one finding for the planted tile")
local fd = res.findings[1]
check(fd.rule == "mixed-rows" and fd.x == 2 and fd.y == 1, "mixed-rows at unit 2,1 (got " .. fd.rule .. " " .. tostring(fd.x) .. "," .. tostring(fd.y) .. ")")
check(res.unit and res.unit.width == 8, "unit is 8x8")
check(res.budgets and res.budgets.tiles.max and res.budgets.palettes.max, "budgets come through")

-- ---- the open-time check must not disturb a running live check ----
local stray_spr = Sprite { fromFile = stray }
SnesArt.start_check(st)
check(st.running, "a live check is running")
local sst, smsg = SnesArt.open(stray_spr)
check(sst == nil and smsg == nil, "meanwhile another PNG (in no project) is opened and ignored")
check(wait(st) and st.result and not st.result.error, "the live check's result still lands: " .. tostring(st.result and st.result.error))
stray_spr:close()
check(st.result.transparent[1] == 0 and st.result.transparent[2] == 16, "the check lists the transparent entries")

-- ---- undo: a stroke keeps the live check armed, and so does an undo, all the way back ----
app.sprite = spr -- transactions and undo act on the active sprite; closing stray left none
st.debounce:stop()
app.transaction("stroke", function() spr.cels[1].image:putPixel(0, 0, 3) end)
check(st.debounce.isRunning, "a stroke arms the debounce")
st.debounce:stop()
app.undo()
check(not st.dead and st.debounce.isRunning, "undoing a stroke keeps SNES art on and re-arms the debounce")
while spr.undoHistory.undoSteps > 0 do app.undo() end
check(not st.dead, "undoing Open as SNES art keeps the panel's state: no guard, no detach")
app.transaction("test edit", function() spr.palettes[1]:setColor(0, Color { r = 1, g = 2, b = 3, a = 255 }) end)
check(spr.palettes[1]:getColor(0).alpha == 255, "and an edit to entry 0 stays")
app.undo() -- back to the PNG as loaded; the edit cleared the redo of the setup

-- ---- opening an already set-up sprite again adds no undo step ----
wait(assert(SnesArt.open(spr), "opening again after the undo"))
check(spr.undoHistory.undoSteps == 1, "opening again sets it up in one step")
local steps = spr.undoHistory.undoSteps
local st2 = assert(SnesArt.open(spr))
check(spr.undoHistory.undoSteps == steps, "reopening a set-up sprite adds no undo step (" .. steps .. " -> " .. spr.undoHistory.undoSteps .. ")")
wait(st2)
spr:saveAs(bg)
spr:close()
spr = Sprite { fromFile = bg }
local st3 = assert(SnesArt.open(spr))
-- the grid is not stored in a PNG, so a freshly loaded file still takes one step for it
check(spr.undoHistory.undoSteps == 1, "a saved SNES-art PNG opens in one step (" .. spr.undoHistory.undoSteps .. ")")
check(st3.result and not st3.running, "and shows the saved file's verdict without a live check")

-- ---- sitechange asks ppu once about a PNG in no project ----
local stray2 = Sprite { fromFile = stray }
local runs, exec = 0, os.execute
os.execute = function(cmd) if cmd:find("art job", 1, true) and cmd:find("now", 1, true) then runs = runs + 1 end return exec(cmd) end
app.sprite = stray2
SnesArt.open_and_show(false); SnesArt.open_and_show(false)
os.execute = exec
check(runs == 1, "a PNG in no project is checked once, not at every site change (" .. runs .. ")")
stray2:close()
spr:close()

-- ---- obj grid at the cell size; a clean PNG has no findings ----
local hs = Sprite { fromFile = hero }
local hst = assert(SnesArt.open(hs))
check(hs.gridBounds.width == 16 and hs.gridBounds.height == 16, "obj grid is the 16x16 cell")
wait(hst) -- open started a live check; let it land before the synchronous one reuses the files
local hres = SnesArt.check_sync(hst)
check(hres.ok and #hres.findings == 0, "clean PNG: no findings")
hs:saveAs(hero) -- set up as saved: the next open shows the file verdict and starts no check
hs:close()

-- ---- an old sprite's live check never touches a newer one's files ----
-- A slowed ppu makes the old job finish while the new one is still running, every time.
local real_getenv = os.getenv
local function slowed(delay)
  local w = app.fs.joinPath(root, "slow" .. delay .. ".sh")
  local sf = io.open(w, "w")
  sf:write("#!/bin/sh\nsleep " .. delay .. "\nexec \"" .. (real_getenv("PPU_BIN") or "ppu") .. "\" \"$@\"\n"); sf:close()
  os.execute('chmod +x "' .. w .. '"')
  return w
end
local wrapper
os.getenv = function(k) if k == "PPU_BIN" and wrapper then return wrapper end return real_getenv(k) end
local old_spr = Sprite { fromFile = bg }
local old = assert(SnesArt.open(old_spr))
wait(old)
wrapper = slowed("1.0")
SnesArt.start_check(old)
os.execute("sleep 0.2") -- the old job has claimed its files and opened its output
wrapper = nil
local new_spr = Sprite { fromFile = hero }
local new = assert(SnesArt.open(new_spr)) -- replaces the old state while its job runs
check(old.dead and not new.running, "the new sprite opens as saved while the old one's check still runs")
wrapper = slowed("2.0")
SnesArt.start_check(new)
wrapper = nil
os.getenv = real_getenv
check(wait(new) and new.result and not new.result.error and new.result.ok,
  "the new sprite's check lands its own result, untouched by the old job: " .. tostring(new.result and new.result.error))
new_spr:close()
old_spr:close()

-- ---- closing the panel mid-check, then reopening it, restarts the poll ----
local p4 = Sprite { fromFile = bg }
local s4 = assert(SnesArt.open(p4))
-- batch mode has no Dialog: a stand-in panel, closed and shown through the panel's own handlers
local fake = { modify = function() end, repaint = function() end, close = function() end }
local function show() s4.dlg = fake; SnesArt.panel_shown(s4) end
local function close() SnesArt.panel_closed(s4) end
show()
SnesArt.start_check(s4)
close()
check(s4.dlg == nil and s4.running and not s4.poll.isRunning, "closing the panel mid-check stops the poll")
show()
check(s4.poll.isRunning, "reopening the panel mid-check restarts the poll")
check(wait(s4) and s4.result and not s4.result.error and s4.status == "1 finding", "and the check's verdict lands: " .. tostring(s4.status))
app.sprite = p4
app.transaction("stroke", function() p4.cels[1].image:putPixel(1, 1, 4) end)
check(s4.debounce.isRunning, "a stroke with the panel open arms the debounce")
close()
check(not s4.running, "the panel closed before the debounce fired: no check yet")
show()
check(s4.running and s4.poll.isRunning, "reopening checks the change made before the close")
wait(s4)
-- a change made while the panel is closed AND a check runs is checked on reopen
local before = #s4.result.findings
show()
SnesArt.start_check(s4)
close()
app.transaction("stroke", function() p4.cels[1].image:putPixel(1, 1, 17) end)
SnesArt.debounce_tick(s4) -- no panel: the tick marks the state stale, as the real timer would
check(s4.running and s4.stale, "a change while the panel is closed and a check runs marks the state stale")
show()
check(wait(s4) and s4.result and not s4.result.error, "the reopened panel's check lands")
local truth = SnesArt.check_sync(s4)
check(#s4.result.findings == #truth.findings and #truth.findings ~= before,
  "the reopened panel shows the sprite as it is: " .. #s4.result.findings .. " vs " .. #truth.findings .. " (was " .. before .. ")")
close()
p4:close()

local function slurp(path) local fh = io.open(path, "rb"); local t = fh:read("a"); fh:close(); return t end
local function gpl_path(name) return app.fs.joinPath(root, "palettes/" .. name .. ".gpl") end
local function px(sp, x, y) return sp.cels[1].image:getPixel(x, y) end

-- ---- Select bad tiles: the selection is the union of the flagged units ----
do
  local bs = Sprite { fromFile = bg }
  app.sprite = bs
  local bst = assert(SnesArt.open(bs))
  check(wait(bst), "the live check lands")
  check(SnesArt.select_bad_tiles(bst), "select bad tiles on a sprite with a flagged tile")
  local sel = bs.selection
  check(sel.bounds == Rectangle(16, 8, 8, 8) and sel:contains(16, 8) and sel:contains(23, 15), "the selection is the flagged 8x8 tile (2,1)")
  bs.cels[1].image:putPixel(3, 3, 17) -- a second tile of row 1 inside row 0
  SnesArt.start_check(bst)
  check(wait(bst), "the check lands again")
  check(SnesArt.select_bad_tiles(bst), "select bad tiles with two flagged tiles")
  sel = bs.selection
  check(sel:contains(16, 8) and sel:contains(0, 0) and not sel:contains(8, 8) and not sel:contains(24, 24)
    and sel.bounds == Rectangle(0, 0, 24, 16), "the selection is exactly the two flagged tiles")
  bst.result.findings[#bst.result.findings + 1] = { rule = "budget", message = "no position" }
  check(SnesArt.select_bad_tiles(bst) and bs.selection.bounds == Rectangle(0, 0, 24, 16), "a finding without a position is skipped")
  bs:close()
  local cs = Sprite { fromFile = hero }
  app.sprite = cs
  local cst = assert(SnesArt.open(cs))
  wait(cst)
  local oks, whys = SnesArt.select_bad_tiles(cst)
  check(oks == false and whys:find("no tile", 1, true) and cs.selection.isEmpty, "a clean sprite refuses with a message and selects nothing: " .. tostring(whys))
  cst.result = { error = "boom" }
  oks, whys = SnesArt.select_bad_tiles(cst)
  check(oks == false and whys:find("boom", 1, true), "a failed check refuses and says why: " .. tostring(whys))
  cst.result = nil
  oks, whys = SnesArt.select_bad_tiles(cst)
  check(oks == false and whys, "no result yet refuses with a message: " .. tostring(whys))
  cs:close()
end

-- ---- palette: a colour edit changes no file until the sprite is saved ----
local art_runs, real_exec = 0, os.execute
-- counts the palette writes: the job staged for the runner names its command
os.execute = function(cmd)
  local fh = io.open(app.fs.joinPath(app.fs.tempPath, "ppu-art/now/job.args"), "rb")
  local staged = fh and fh:read("a") or ""
  if fh then fh:close() end
  if cmd:find("art job", 1, true) and staged:find("palette\nwrite", 1, true) then art_runs = art_runs + 1 end
  return real_exec(cmd)
end
local function files() -- every file the plugin could touch, as bytes
  local t = {}
  for _, f in ipairs { "palettes/bg_pal.gpl", "art/bg.png", "art/hero.png", "ppu.json" } do t[f] = slurp(app.fs.joinPath(root, f)) end
  return t
end
local function same_files(a, b) for k, v in pairs(a) do if b[k] ~= v then return false end end return true end
local function save(sp) app.sprite = sp; app.command.SaveFile() end -- the real save command: the plugin's aftercommand hook
local ps = Sprite { fromFile = bg }
local pst = assert(SnesArt.open(ps))
wait(pst)
app.sprite = ps
local steps0 = ps.undoHistory.undoSteps
local disk = files()
app.transaction("edit colour", function() ps.palettes[1]:setColor(5, Color { r = 255, g = 8, b = 7 }) end)
SnesArt.debounce_tick(pst)
check(same_files(disk, files()) and art_runs == 0, "a colour edit and the debounce tick change no file")
app.undo(); SnesArt.debounce_tick(pst)
check(same_files(disk, files()) and art_runs == 0, "undoing the edit changes no file")
app.redo(); SnesArt.debounce_tick(pst)
check(same_files(disk, files()) and art_runs == 0 and ps.palettes[1]:getColor(5).green == 8, "redoing the edit changes no file")

-- the save: the .gpl is rewritten once (snapped, header and names kept), every PNG on it follows
save(ps)
check(art_runs == 1, "the save writes the palette once (" .. art_runs .. ")")
local text = slurp(gpl_path("bg_pal"))
check(text:find("GIMP Palette\nName: bg_pal\nColumns: 16\n#\n", 1, true), "the .gpl keeps its header")
check(text:find("255   8   0\tc5\n", 1, true) and text:find("\tc31\n", 1, true), "the edited colour is snapped and keeps its name")
local c5 = ps.palettes[1]:getColor(5)
check(c5.red == 255 and c5.green == 8 and c5.blue == 0, "snapping changed the colour, so the sprite takes the snapped one")
check(ps.undoHistory.undoSteps == steps0 + 2, "the edit and its snap are two steps, the snap named for what it is")
check(pst.status:find("bg_pal.gpl saved, 2 PNGs synced", 1, true), "status names the .gpl and the PNG count: " .. tostring(pst.status))
local sib = Sprite { fromFile = hero }
local sc = sib.palettes[1]:getColor(5)
check(sc.red == 255 and sc.green == 8 and sc.blue == 0, "a sibling PNG shows the new colour on reopen")
sib:close()
app.sprite = ps
print("note: after the snap the sprite is " .. (ps.isModified and "marked modified" or "clean"))
save(ps)
check(art_runs == 1, "a save with no colour change writes nothing")
local after_save = files()
app.undo(); app.undo(); SnesArt.debounce_tick(pst)
check(same_files(after_save, files()), "undoing the snap and the edit after the save changes no file")
app.redo(); app.redo()

-- the unmatched-colour alert is a table of lines, capped
check(#SnesArt.alert_lines("a\nb\nc") == 3, "a short list is one alert line each")
local long = {}
for i = 1, 15 do long[i] = "#" .. i end
local al = SnesArt.alert_lines(table.concat(long, "\n"))
check(#al == 11 and al[1] == "#1" and al[10] == "#10" and al[11] == "…", "a long list is capped at 10 lines plus an ellipsis")

-- tile rows: one layer and one frame, a selection, one undo step each
ps:newLayer()
local okr, why = SnesArt.set_tile_row(pst)
check(not okr and why:find("one layer", 1, true), "a sprite with two layers is refused: " .. tostring(why))
ps:deleteLayer(ps.layers[2])
local okf, whyf = SnesArt.fix_tile_rows(pst)
check(okf, "one layer again")
app.sprite = ps
ps.selection:deselect()
okr, why = SnesArt.set_tile_row(pst)
check(not okr and why:find("Select", 1, true), "no selection: " .. tostring(why))
ps.selection = Selection(Rectangle(8, 8, 8, 8))
app.fgColor = Color { index = 17 }
steps0 = ps.undoHistory.undoSteps
local p00 = px(ps, 0, 0)
check(px(ps, 8, 8) == 2 and p00 < 16, "tile 1,1 starts on row 0")
okr, why = SnesArt.set_tile_row(pst)
check(okr, "set tile row: " .. tostring(why))
check(px(ps, 8, 8) == 18 and px(ps, 15, 15) == 16 + (1 + 30 % 15) and px(ps, 0, 0) == p00, "the selected tile moves to the foreground colour's row, no other")
check(ps.undoHistory.undoSteps == steps0 + 1, "set tile row is one undo step")
app.undo()
check(px(ps, 8, 8) == 2, "one undo puts the tile back")
-- fix: the stray pixel (index 17) joins row 0 once row 0 has its colour (saved, so ppu reads it from the .gpl)
app.transaction("edit colour", function() ps.palettes[1]:setColor(17, ps.palettes[1]:getColor(4)) end)
save(ps)
steps0 = ps.undoHistory.undoSteps
okf, whyf = SnesArt.fix_tile_rows(pst)
check(okf and px(ps, 19, 11) == 4, "fix tile rows moves a stray pixel to the same colour in its tile's row: " .. tostring(whyf))
check(ps.undoHistory.undoSteps == steps0 + 1, "fix tile rows is one undo step")
check(pst.status:find("moved 1 pixels", 1, true), "status says what moved: " .. tostring(pst.status))
steps0 = ps.undoHistory.undoSteps
okf = SnesArt.fix_tile_rows(pst)
check(okf and ps.undoHistory.undoSteps == steps0, "fixing a fixed sprite adds no undo step")

-- Save As to another path writes no palette: the .gpl belongs to the PNG that was opened
app.transaction("edit colour", function() ps.palettes[1]:setColor(9, Color { r = 255, g = 0, b = 0 }) end)
local before_as, runs_as = slurp(gpl_path("bg_pal")), art_runs
app.command.SaveFileAs { filename = app.fs.joinPath(root, "art/copy.png") }
check(slurp(gpl_path("bg_pal")) == before_as and art_runs == runs_as, "Save As to another path writes no .gpl")
check(pst.warning:find("not written", 1, true), "and says so: " .. tostring(pst.warning))
os.execute = real_exec
ps:close()
-- ---- Mode 7 EXTBG with a .gpl of 128 colours: opens with the PLTE ppu reports (mirrored to 256) ----
local m7s = Sprite { fromFile = m7png }
local m7st, m7msg = SnesArt.open(m7s)
check(m7st, "EXTBG art with a 128-colour .gpl opens: " .. tostring(m7msg))
local mp = m7s.palettes[1]
check(#mp == 256, "its palette is the 256-entry PLTE, not the .gpl's 128 (" .. #mp .. ")")
check(mp:getColor(0).alpha == 0 and mp:getColor(128).alpha == 0 and mp:getColor(1).alpha == 255
  and mp:getColor(129).alpha == 255, "entries 0 and 128 are transparent, the rest opaque")
local r9, g9, b9 = rgb(9)
check(mp:getColor(9).red == r9 and mp:getColor(137).red == r9 and mp:getColor(137).blue == b9, "entry 128 + k shows entry k")
local m7before = slurp(gpl_path("m7_pal"))
app.sprite = m7s
-- an edit to one of the .gpl's own entries is written to its 128 lines on save, and the mirror follows
app.transaction("edit colour", function() m7s.palettes[1]:setColor(9, Color { r = 255, g = 8, b = 7 }) end)
check(slurp(gpl_path("m7_pal")) == m7before, "the edit alone writes nothing")
app.command.SaveFile()
local m7text = slurp(gpl_path("m7_pal"))
local lines = 0
for _ in m7text:gmatch("\tc%d+\n") do lines = lines + 1 end
check(m7text:find("255   8   0\tc9\n", 1, true) and lines == 128, "the save writes the edit, the .gpl keeps its 128 colours (" .. lines .. ")")
local c137 = m7s.palettes[1]:getColor(137)
check(c137.red == 255 and c137.green == 8 and c137.blue == 0 and c137.alpha == 255, "entry 137 takes entry 9's snapped colour")
-- an edit past the .gpl's own entries has nowhere to go: the .gpl is unchanged and the panel says so
local m7saved = slurp(gpl_path("m7_pal"))
app.transaction("edit mirror", function() m7s.palettes[1]:setColor(140, Color { r = 0, g = 255, b = 0 }) end)
app.command.SaveFile()
check(slurp(gpl_path("m7_pal")) == m7saved and m7before ~= m7saved, "an edit to entry 140 writes nothing")
check(m7st.warning:find("Entry 140 is past m7_pal.gpl's own 128 colours", 1, true), "and warns: " .. tostring(m7st.warning))
m7s:close()

-- ---- art that names no palette: the row commands refuse while its colours are unsaved ----
do
  local np = Sprite(32, 32, ColorMode.INDEXED)
  local npal = Palette(32)
  for i = 0, 31 do local r, g, b = rgb(i); npal:setColor(i, Color { r = r, g = g, b = b, a = (i % 16 == 0) and 0 or 255 }) end
  np:setPalette(npal)
  for y = 0, 31 do for x = 0, 31 do np.cels[1].image:putPixel(x, y, 1 + (x + y) % 15) end end
  np.cels[1].image:putPixel(3, 3, 17) -- a stray of row 1 in tile 0,0
  local nppath = app.fs.joinPath(root, "art/plain.png")
  np:saveAs(nppath)
  np:close()
  -- spliced into the text: a json round trip would turn the manifest's empty lists into null
  local mpath = app.fs.joinPath(root, "ppu.json")
  local plain = json.encode { name = "plain", kind = "bg", file = "art/plain.png", options = { bit_depth = 4, tile_size = 8 } }
  local mtext, nsub = slurp(mpath):gsub('"sources"%s*:%s*%[', function(m) return m .. plain .. "," end, 1)
  assert(nsub == 1, "ppu.json has a sources list")
  local mf = io.open(mpath, "w"); mf:write(mtext); mf:close()
  local ns = Sprite { fromFile = nppath }
  local nst = assert(SnesArt.open(ns))
  wait(nst)
  app.sprite = ns
  check(nst.gpl == nil, "plain.png names no palette")
  check(SnesArt.fix_tile_rows(nst), "with its colours saved, fix tile rows runs")
  app.undo()
  app.transaction("edit colour", function() ns.palettes[1]:setColor(17, Color { r = 255, g = 255, b = 0 }) end)
  local bytes = ns.cels[1].image.bytes
  local okn, whyn = SnesArt.fix_tile_rows(nst)
  check(not okn and whyn == "The art's colours are not saved yet: save the sprite first.", "an unsaved colour edit refuses fix tile rows: " .. tostring(whyn))
  ns.selection = Selection(Rectangle(0, 0, 8, 8))
  okn, whyn = SnesArt.set_tile_row(nst)
  check(not okn and whyn:find("not saved yet", 1, true), "and set tile row: " .. tostring(whyn))
  check(ns.cels[1].image.bytes == bytes and ns.palettes[1]:getColor(17).blue == 0, "and nothing changed")
  ns:saveAs(nppath)
  check(SnesArt.fix_tile_rows(nst), "once saved, fix tile rows runs again")
  ns:close()
end

-- ---- unrecognised input is refused with a message, never dropped ----
local u = Sprite { fromFile = stray }
local ust, umsg = SnesArt.open(u)
check(ust == nil and umsg == nil, "a PNG in no ppu.json: nothing happens")
u:close()
local g = Sprite { fromFile = ghost }
local gst, gmsg = SnesArt.open(g)
check(gst == nil and gmsg and gmsg:find("nope"), "unknown palette name refused: " .. tostring(gmsg))
g:close()
local w = Sprite { fromFile = weird }
local wst, wmsg = SnesArt.open(w)
check(wst == nil and wmsg and wmsg:find("frobnicate"), "unknown source kind refused: " .. tostring(wmsg))
w:close()
local r = Sprite { fromFile = rgbpng }
local rst, rmsg = SnesArt.open(r)
check(rst == nil and rmsg and rmsg:find("full colour"), "full-colour PNG refused: " .. tostring(rmsg))
r:close()
os.rename(gpl_path("bg_pal"), gpl_path("bg_pal") .. ".away")
local m = Sprite { fromFile = bg }
local mst, mmsg = SnesArt.open(m)
check(mst == nil and mmsg and mmsg:find("cannot read"), "missing .gpl refused: " .. tostring(mmsg))
m:close()
os.rename(gpl_path("bg_pal") .. ".away", gpl_path("bg_pal"))

-- ---- New SNES art: ppu writes the files, the plugin opens the result ----
local function exists(p) local fh = io.open(p, "rb"); if fh then fh:close() end return fh ~= nil end
local function manifest_names()
  local mf = io.open(app.fs.joinPath(root, "ppu.json")); local d = json.decode(mf:read("a")); mf:close()
  local n = {}; for _, s in ipairs(d.sources) do n[s.name] = true end
  return n
end
local newpng = app.fs.joinPath(root, "art/fresh.png")
local nok, nres = SnesArt.new_art { png = newpng, kind = "bg", size = "64x32", new_palette = "fresh_pal" }
check(nok and nres == newpng, "new_art with a new palette succeeds: " .. tostring(nres))
check(exists(newpng) and exists(app.fs.joinPath(root, "palettes/fresh_pal.gpl")), "the PNG and the new .gpl exist")
check(manifest_names().fresh and manifest_names().fresh_pal, "ppu.json lists the art and its palette")
local ns = app.sprite
check(ns and ns.filename == newpng and ns.width == 64 and ns.height == 32, "the new art is the open sprite")
local nst, nmsg = SnesArt.open(ns) -- sitechange does the same in the GUI; batch mode does not fire it
check(nst, "the opened sprite is set up as SNES art" .. tostring(nmsg))
local nchk = SnesArt.check_sync(nst)
check(nchk.ok and #nchk.findings == 0, "the check passes on the empty image")
ns:close()

local gok = SnesArt.new_art { png = app.fs.joinPath(root, "art/sheet2.png"), kind = "obj", size = "32x32", cell = 16,
  palette = app.fs.joinPath(root, "palettes/bg_pal.gpl"), bpp = 4 }
check(gok and manifest_names().sheet2, "new_art with an existing .gpl and a cell succeeds")
if app.sprite then app.sprite:close() end

local before = io.open(app.fs.joinPath(root, "ppu.json")):read("a")
local rok, rwhy = SnesArt.new_art { png = newpng, kind = "bg", size = "64x32", new_palette = "other_pal" }
check(rok == false and rwhy:find("exist", 1, true), "an existing PNG is refused with ppu's message: " .. tostring(rwhy))
local bok, bwhy = SnesArt.new_art { png = app.fs.joinPath(root, "art/bad.png"), kind = "bg", size = "60x32", new_palette = "bad_pal" }
check(bok == false and bwhy ~= "" and not exists(app.fs.joinPath(root, "art/bad.png")) and
  not exists(app.fs.joinPath(root, "palettes/bad_pal.gpl")), "a size off the tile grid is refused and writes nothing: " .. tostring(bwhy))
check(io.open(app.fs.joinPath(root, "ppu.json")):read("a") == before, "a refusal leaves ppu.json untouched")
local nrok, nrwhy = SnesArt.new_art { png = app.fs.joinPath(root, "art/nopal.png"), kind = "bg", size = "64x32" }
check(nrok == false and nrwhy ~= "", "no palette named is refused: " .. tostring(nrwhy))

-- ---- parser: unknown rule kept, bad JSON refused ----
local p = SnesArt.parse('{"ok":false,"protocol":1,"findings":[{"rule":"future-rule","message":"something new","x":1,"y":2}],"unit":{"width":8,"height":8}}')
check(p.findings[1].rule == "future-rule" and p.findings[1].x == 1 and p.findings[1].message == "something new", "unknown rule kept as a finding")
check(SnesArt.parse("not json").error, "malformed JSON is an error")
check(SnesArt.parse("").error, "empty output is an error")
do
local boom = SnesArt.parse('{"ok":false,"protocol":1,"reason":"not_listed","error":"boom"}')
check(boom.error == "boom" and boom.reason == "not_listed", "the CLI's error and its reason code are passed on")
check(SnesArt.parse('{"ok":false,"protocol":1,"findings":[]}').findings[1].rule == "unknown", "ok:false with no findings still reports one")
local old = SnesArt.parse('{"ok":true,"findings":[]}')
check(old.reason == "too_old" and old.error:find("too old", 1, true), "an answer without a protocol number is a ppu too old: " .. tostring(old.error))
check(SnesArt.parse('{"ok":true,"protocol":2,"findings":[]}').reason == "plugin_too_old", "a protocol this plugin does not know is refused")
end

-- ---- an unsaved colour edit survives a switch to another sprite; two sprites on one .gpl lose nothing ----
local function set_rgb(sp, i, r, g, b) app.transaction("edit colour", function() sp.palettes[1]:setColor(i, Color { r = r, g = g, b = b }) end) end
local a2 = Sprite { fromFile = bg }
assert(SnesArt.open(a2))
set_rgb(a2, 7, 8, 16, 24)
local h2 = Sprite { fromFile = hero }
assert(SnesArt.open(h2))
assert(SnesArt.open(a2))
local k7 = a2.palettes[1]:getColor(7)
check(k7.red == 8 and k7.green == 16 and k7.blue == 24, "coming back to an edited sprite keeps its unsaved colour edit")
save(a2)
check(slurp(gpl_path("bg_pal")):find("  8  16  24\tc7\n", 1, true), "and its save writes that edit to the .gpl")
-- both sprites hold the .gpl as it is now; each edits a different entry, each saves in turn
assert(SnesArt.open(h2)); set_rgb(h2, 9, 16, 24, 8)
assert(SnesArt.open(a2)); set_rgb(a2, 11, 24, 8, 16)
assert(SnesArt.open(h2))
save(h2)
assert(SnesArt.open(a2))
local k11 = a2.palettes[1]:getColor(11)
check(k11.red == 24 and k11.green == 8 and k11.blue == 16, "the sprite that did not save keeps its own edit after another sprite's save")
local k9 = a2.palettes[1]:getColor(9)
check(k9.red == 16 and k9.green == 24 and k9.blue == 8, "and takes the other sprite's saved colour for an entry it did not edit")
save(a2)
local both = slurp(gpl_path("bg_pal"))
check(both:find("  8  16  24\tc7\n", 1, true) and both:find(" 16  24   8\tc9\n", 1, true) and both:find(" 24   8  16\tc11\n", 1, true),
  "two sprites on one .gpl: the .gpl holds both edits and the first one")

-- a save that did not complete (the sprite is still modified) writes nothing
set_rgb(a2, 13, 8, 8, 8)
local runs_before, text_before = art_runs, slurp(gpl_path("bg_pal"))
check(a2.isModified, "a pending edit leaves the sprite modified")
SnesArt.after_command { name = "SaveFile" }
check(art_runs == runs_before and slurp(gpl_path("bg_pal")) == text_before, "a cancelled or failed save (sprite still modified) writes nothing")
-- an off-grid .gpl (one a plain editor wrote): another sprite's save is still not undone by a pixel-only save
do
  local og = { "GIMP Palette", "Name: bg_pal", "Columns: 16", "#" }
  for i = 0, 31 do og[#og + 1] = string.format("%3d 100 100\tc%d", 100 + i, i) end
  local fh = io.open(gpl_path("bg_pal"), "w"); fh:write(table.concat(og, "\n"), "\n"); fh:close()
  local A, B = Sprite { fromFile = bg }, Sprite { fromFile = hero }
  assert(SnesArt.open(A)); assert(SnesArt.open(B))
  set_rgb(B, 3, 248, 0, 0)
  save(B)
  assert(SnesArt.open(A))
  local c3 = A.palettes[1]:getColor(3)
  check(c3.red == 255 and c3.green == 0 and c3.blue == 0, "off-grid .gpl: A takes the colour B saved")
  app.sprite = A
  app.transaction("pixel", function() A.cels[1].image:putPixel(0, 0, 2) end)
  save(A)
  check(slurp(gpl_path("bg_pal")):find("255   0   0\tc3\n", 1, true), "and A's pixel-only save keeps B's colour in the .gpl")
end
-- ---- Convert to SNES art: ppu converts, the plugin saves first and reopens the result ----
do
  local function list_source(name, kind, file, options)
    local text = slurp(app.fs.joinPath(root, "ppu.json"))
    local entry = json.encode { name = name, kind = kind, file = file, options = options }
    local m = io.open(app.fs.joinPath(root, "ppu.json"), "w")
    -- ppu keeps the manifest pretty-printed with "sources" before "title": the last source gets a sibling
    local out, n = text:gsub('%s*%],(%s*)"title"', function(ws) return "," .. entry .. "]," .. ws .. '"title"' end)
    assert(n == 1, "manifest layout changed")
    m:write(out); m:close()
  end
  local function rgb_sprite(name) -- a 32x32 full-colour drawing, saved
    local sp = Sprite(32, 32, ColorMode.RGB)
    local img = sp.cels[1].image
    for y = 0, 31 do for x = 0, 31 do
      img:putPixel(x, y, app.pixelColor.rgba(40 * ((x // 4 + y // 8) % 5) + 20, 200 - 30 * ((x // 8) % 4), 90 + 20 * (y // 8), 255))
    end end
    local path = app.fs.joinPath(root, "art/" .. name .. ".png")
    sp:saveAs(path)
    return sp, path
  end
  local function options_of(name) -- the source's options as ppu.json stores them
    local d = json.decode(slurp(app.fs.joinPath(root, "ppu.json")))
    for _, s in ipairs(d.sources) do if s.name == name then return s.options end end
  end
  local function tree() -- every file a refusal must leave alone
    return slurp(app.fs.joinPath(root, "ppu.json")) .. slurp(gpl_path("bg_pal")) .. table.concat(app.fs.listFiles(app.fs.joinPath(root, "palettes")), ",")
  end

  -- 1: a listed full-colour PNG refuses to open and names the command
  local rg = Sprite { fromFile = rgbpng }
  local _, why = SnesArt.open(rg)
  check(why and why:find("Convert to SNES art", 1, true), "a full-colour project PNG's refusal offers Convert to SNES art: " .. tostring(why))
  -- 2: the dialog's fields come from ppu's check
  local f = SnesArt.convert_fields(rg)
  check(f and f.listed and f.kind == "bg" and f.bpp == 4 and f.cell == 8 and f.dither == "none" and f.dither_strength == 50
    and f.alpha_threshold == 128 and f.max_palettes == 8 and f.new_palette == "rgb_pal", "a listed PNG's fields: its kind, converter defaults, <stem>_pal")
  rg:close()
  local _, op = rgb_sprite("objpic")
  list_source("objpic", "obj", "art/objpic.png", { cell_size = 16, dither = "bayer", max_palettes = 3 })
  app.sprite:close()
  local os_ = Sprite { fromFile = op }
  local of = SnesArt.convert_fields(os_)
  check(of and of.listed and of.kind == "obj" and of.cell == 16 and of.dither == "bayer" and of.max_palettes == 3 and of.alpha_threshold == 128,
    "an obj entry's stored cell, dither and cap are prefilled")
  local bgs = Sprite { fromFile = bg }
  local uf = SnesArt.convert_fields(bgs)
  bgs:close()
  check(uf and uf.listed and uf.kind == "bg" and uf.bpp == 4, "an indexed listed PNG is prefilled too")
  check(tostring(uf.bpp) == "4" and tostring(of.cell) == "16" and tostring(of.max_palettes) == "3",
    "prefilled numbers print as the dialog's options (\"4\", not \"4.0\")")
  -- 6: obj with dither and alpha set: no flag the kind ignores is passed (ppu would exit 2)
  local ok, lines, new = SnesArt.convert(os_, { kind = "obj", cell = 16, cell_height = nil, new_palette = "objpic_pal",
    dither = "bayer", dither_strength = 30, alpha_threshold = 100, max_palettes = 3 })
  check(ok, "an obj converts with dither and alpha set: " .. tostring(ok and "" or lines))
  check(new and new.colorMode == ColorMode.INDEXED and app.sprite == new, "the reopened sprite is the converted, indexed PNG")
  local ns = SnesArt.open(new)
  check(ns and SnesArt.check_sync(ns).ok, "and it opens as SNES art and passes ppu art check")
  new:close()

  -- 3: a modified full-colour sprite is saved first, then converted and reopened
  local sp, path = rgb_sprite("edited")
  list_source("edited", "bg", "art/edited.png", nil)
  app.transaction("draw", function() sp.cels[1].image:putPixel(0, 0, app.pixelColor.rgba(255, 0, 255, 255)) end)
  check(sp.isModified, "the drawing is unsaved")
  local f3 = assert(SnesArt.convert_fields(sp))
  local ok3, lines3, new3 = SnesArt.convert(sp, f3)
  check(ok3, "convert on a modified sprite: " .. tostring(ok3 and "" or lines3))
  check(new3.filename == path and new3.colorMode == ColorMode.INDEXED, "reopened as an indexed PNG at the same path")
  local st3, msg3 = SnesArt.open(new3)
  check(st3 and SnesArt.check_sync(st3).ok and #SnesArt.check_sync(st3).findings == 0, "the file passes the check with no findings " .. tostring(msg3 or ""))
  local o3 = options_of("edited")
  check(o3 and o3.dither == nil and o3.dither_strength == nil and o3.palette == "edited_pal",
    "fields left as prefilled are not passed, so ppu.json gains no option but the palette")
  local summary = table.concat(lines3, "\n")
  local backup = summary:match("Original kept in ([^\n]+)")
  check(backup and exists(backup) and backup:find(".ppu-backup", 1, true), "the summary says where the original is kept, and it is there: " .. summary)
  check(summary:find("colours", 1, true) and summary:find("tiles", 1, true) and summary:find("palettes", 1, true), "and what the import made")
  -- the backup holds the saved drawing (the unsaved pixel too: it was saved first)
  local bk = Sprite { fromFile = backup }
  check(bk.cels[1].image:getPixel(0, 0) == app.pixelColor.rgba(255, 0, 255, 255), "the original kept is the drawing as saved by the plugin")
  bk:close(); new3:close()

  -- 4: an unlisted PNG inside the project converts with the kind the author picks
  local up, upath = rgb_sprite("unlisted")
  local f4 = SnesArt.convert_fields(up)
  check(f4 and not f4.listed and f4.kind == "bg" and f4.new_palette == "unlisted_pal", "an unlisted PNG gets the defaults")
  f4.kind, f4.dither = "sheet", "diffusion"
  local ok4, lines4, new4 = SnesArt.convert(up, f4)
  check(ok4 and manifest_names().unlisted, "an unlisted PNG converts and is listed: " .. tostring(ok4 and "" or lines4))
  local o4 = options_of("unlisted")
  check(o4 and o4.dither == "diffusion", "a dither changed from the prefill is passed: " .. tostring(o4 and o4.dither))
  check(o4 and o4.max_palettes == nil and o4.alpha_threshold == nil,
    "the converter's defaults are not pinned into ppu.json")
  local chk = SnesArt.open(new4)
  check(chk and SnesArt.check_sync(chk).ok, "and passes the check")
  new4:close()

  -- a listed obj with a stored rectangular cell: a new cell from the dialog is square
  do
  local tp = rgb_sprite("tall")
  list_source("tall", "obj", "art/tall.png", { cell_size = 16, cell_height = 32 })
  local ft = assert(SnesArt.convert_fields(tp))
  check(ft.cell == 16, "the stored cell is prefilled")
  ft.cell = 8
  local okt, linest, newt = SnesArt.convert(tp, ft)
  local ot = options_of("tall")
  check(okt and ot.cell_size == 8 and ot.cell_height == 8, "a new cell does not keep the stored height: " .. tostring(okt and "" or linest))
  if newt then newt:close() end
  end

  -- a PNG outside any project is refused before the dialog, with ppu's message
  do
  local outside = Sprite(8, 8, ColorMode.RGB)
  outside:saveAs(app.fs.joinPath(app.fs.tempPath, "ppu-art-outside.png"))
  local of2, owhy = SnesArt.convert_fields(outside)
  check(of2 == nil and owhy and owhy ~= "", "a PNG in no project has no fields, and ppu's message says why: " .. tostring(owhy))
  outside:close()
  os.remove(app.fs.joinPath(app.fs.tempPath, "ppu-art-outside.png"))
  end

  -- 7: not a .png
  local fresh = Sprite(8, 8, ColorMode.RGB)
  local okf, whyf = SnesArt.convert(fresh, { kind = "bg" })
  check(okf == false and whyf:find("PNG", 1, true) and fresh.isValid, "an unsaved sprite is refused, and stays open: " .. tostring(whyf))
  local nf, nwhy = SnesArt.convert_fields(fresh)
  check(nf == nil and nwhy:find("PNG", 1, true), "and has no fields: " .. tostring(nwhy))
  fresh:close()

  -- 5: a refusal names ppu's message, changes no file, keeps the sprite
  local rs, rpath = rgb_sprite("refused")
  local before, pngbefore = tree(), slurp(rpath)
  local f5 = { kind = "bg", bpp = 4, new_palette = "bg_pal", dither = "none", dither_strength = 50, alpha_threshold = 128, max_palettes = 8 }
  local ok5, why5 = SnesArt.convert(rs, f5)
  check(ok5 == false and why5:find("bg_pal", 1, true), "a taken palette name is refused with ppu's message: " .. tostring(why5))
  check(tree() == before and slurp(rpath) == pngbefore and rs.isValid, "a refusal changes no file and keeps the sprite open")
  local ok5b, why5b = SnesArt.convert(rs, { kind = "frobnicate" })
  check(ok5b == false and why5b:find("frobnicate", 1, true), "an unknown kind is refused by name")
  f5.dither = "ordered"
  local ok5c, why5c = SnesArt.convert(rs, f5)
  check(ok5c == false and why5c:find("ordered", 1, true), "an unknown dither is refused by name")
  f5.dither, f5.max_palettes = "none", "many"
  local ok5d, why5d = SnesArt.convert(rs, f5)
  check(ok5d == false and why5d:find("many", 1, true) and tree() == before, "a non-number is refused by name")
  rs:close()

  -- m7 passes no dither, alpha or palette-cap flag
  local m7s = Sprite(64, 64, ColorMode.RGB)
  local mi = m7s.cels[1].image
  for y = 0, 63 do for x = 0, 63 do mi:putPixel(x, y, app.pixelColor.rgba(60 * ((x // 8 + y // 8) % 4) + 15, 120, 200 - 40 * (y // 16), 255)) end end
  m7s:saveAs(app.fs.joinPath(root, "art/m7new.png"))
  local ok6, lines6, new6 = SnesArt.convert(m7s, { kind = "m7", new_palette = "m7new_pal", dither = "bayer", max_palettes = 8 })
  check(ok6, "an m7 converts without any flag it ignores: " .. tostring(ok6 and "" or lines6))
  if new6 then new6:close() end
end

print("ALL PASSED")
