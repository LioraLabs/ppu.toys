-- SNES art for Aseprite: opens a PNG listed in the nearest ppu.json as SNES art, flags bad tiles
-- live, and writes the palette when the sprite is saved. Every verdict, and what the manifest says
-- about the PNG, comes from `ppu art check` (never reimplemented here). ALL file and
-- process access (io.*, os.execute, Palette{fromFile=}) lives in this one file, so Aseprite's
-- security prompt names one script. Needs Aseprite 1.3 or newer (API 25: json, Timer, canvas, events).

local MIN_API = 25
local PANEL = "SNES art"
local DEBOUNCE = 0.12 -- seconds from the last change to the check
local POLL = 0.02 -- seconds between looks for the finished check
local GIVE_UP = 60 -- seconds before a check that never finished is called failed
local MAX_LISTED = 8 -- findings shown in the panel list

SnesArt = {}
local M = SnesArt
local taken = {} -- png -> { sprite, lines }: the .gpl lines that sprite last took (at open, after its own save)
local S = nil -- the one SNES-art sprite being watched (shortcut: one at a time, when two are needed key this by sprite)
local tried = {} -- project PNGs that already failed to open, so sitechange does not reopen the panel at each switch
-- ---------------------------------------------------------------------------------------------
-- The job protocol. Every `ppu` call is a job in a folder of the temp path, run by `ppu art job`
-- (ppu's own job runner: no shell, no `timeout` program, the same command line on every OS), so
-- Aseprite's security prompt is answered once and no script text lives here. A job is
-- `<name>.in` (stdin, optional; written first) and `<name>.args` (one ppu argument per line;
-- written last: the runner takes a job when its args file appears). The runner leaves
-- `<name>.out`, `<name>.err` and last `<name>.code`, the exit code: `.code` means done, and the
-- three files are never half-written. Two folders, so the two modes never share a file:
--   live/job.<n>.*  the live check, in the background (`--detach`); each job n has its own files, so
--                   an old sprite's job can never touch a newer one's, and the poller watches .code
--   now/job.*       everything else (the open check, palette write, rows, new, convert): the
--                   runner is waited for; ppu's own 20 s timeout stops a hung ppu freezing Aseprite
-- shortcut: an argument holding a line break cannot travel (such paths are refused), when it matters pass them NUL-split.
-- ---------------------------------------------------------------------------------------------
local PROTOCOL = 1 -- the machine contract of `ppu art ... --json` this plugin reads (docs: ppu docs cli)

-- json.decode hands back userdata for objects and arrays (indexable, ipairs works), not tables
local function istab(v) return type(v) == "table" or type(v) == "userdata" end

local function jobdir(kind) return app.fs.joinPath(app.fs.joinPath(app.fs.tempPath, "ppu-art"), kind) end
local function jobfile(kind, name, ext) return app.fs.joinPath(jobdir(kind), name .. "." .. ext) end
local seq = os.time() * 1000 -- live job numbers; seeded by the clock so a restart never reuses a leftover's name
local function livename(n) return "job." .. n end
local function livefile(n, ext) return jobfile("live", livename(n), ext) end

-- Remove every file of a folder whose name starts with `prefix` (a job's files, results, .part names too).
local function sweep(kind, prefix)
  for _, f in ipairs(app.fs.listFiles(jobdir(kind))) do
    if f:sub(1, #prefix) == prefix then os.remove(app.fs.joinPath(jobdir(kind), f)) end
  end
end
local function remove_job(n) sweep("live", livename(n) .. ".") end

local function writefile(path, data, mode)
  local f, e = io.open(path, mode or "w")
  if not f then return nil, e end
  f:write(data)
  f:close()
  return true
end

local function readfile(path)
  local f = io.open(path, "rb")
  if not f then return nil end
  local s = f:read("a")
  f:close()
  return s
end

local function ppu_bin() return os.getenv("PPU_BIN") or "ppu" end

-- The one constant command line per mode. cmd /c strips the outer pair of quotes, so on Windows the
-- whole line gets one more.
local function runner(kind, detach)
  local line = '"' .. ppu_bin() .. '" art job "' .. jobdir(kind) .. '"' .. (detach and " --detach" or "")
  if app.os.windows then line = '"' .. line .. '"' end
  return line
end

-- Stage one job: `input` (stdin; nil for none) first, the args last. true, or nil + why.
local function stage(kind, name, args, input)
  app.fs.makeAllDirectories(jobdir(kind))
  local ok, why = true, nil
  if input then ok, why = writefile(jobfile(kind, name, "in"), input, "wb") end
  if ok then ok, why = writefile(jobfile(kind, name, "args"), table.concat(args, "\n") .. "\n") end
  if not ok then return nil, "cannot write the check job files in " .. jobdir(kind) .. ": " .. tostring(why) end
  return true
end

local function hex_bytes(hex) return (hex:gsub("%x%x", function(h) return string.char(tonumber(h, 16)) end)) end

-- Run `ppu <args> --json` with `input` on stdin, waiting for it. {code, out, err, answer, pixels, notes};
-- code is nil when ppu left no result. `err` is the refusal's message (ppu's own words, shown to the
-- author as they are), `answer` the decoded JSON, `pixels` the answer's index bytes, `notes` its lines.
-- The job files are gone when this returns, however it ends.
local function run_art(args, input)
  for _, a in ipairs(args) do
    if a:find("[\r\n]") then return { err = "an argument with a line break cannot be passed to ppu: " .. a } end
  end
  app.fs.makeAllDirectories(jobdir("now"))
  sweep("now", "job.")
  local all = { table.unpack(args) }
  all[#all + 1] = "--json"
  local ok, why = stage("now", "job", all, input)
  if not ok then sweep("now", "job."); return { err = why } end
  os.execute(runner("now"))
  local res = { code = tonumber(readfile(jobfile("now", "job", "code"))), out = readfile(jobfile("now", "job", "out")) or "",
    err = (readfile(jobfile("now", "job", "err")) or ""):gsub("%s+$", "") }
  sweep("now", "job.")
  if not res.code then res.err = "ppu left no result: is it installed (PATH or PPU_BIN) and up to date?"; return res end
  local okj, d = pcall(json.decode, res.out)
  if okj and istab(d) then
    res.answer = d
    if d.ok == false and d.error then res.err = tostring(d.error) end
    if d.reason == "unmatched" and istab(d.unmatched) then
      local lines = {}
      for _, l in ipairs(d.unmatched) do lines[#lines + 1] = tostring(l) end
      res.err = table.concat(lines, "\n")
    end
    if type(d.pixels) == "string" then res.pixels = hex_bytes(d.pixels) end
    res.notes = {}
    if istab(d.notes) then for _, n in ipairs(d.notes) do res.notes[#res.notes + 1] = tostring(n) end end
  end
  return res
end

-- Normalise the check's JSON into a result table; never drops what it does not know.
-- {ok=bool (no findings), error=text or nil, findings={{rule,message,x,y}}, budgets, unit}
function M.parse(text)
  if text == nil or text:match("^%s*$") then return { error = "the check printed nothing" } end
  local okj, d = pcall(json.decode, text)
  if not okj or not istab(d) then
    return { error = "the check printed something that is not JSON: " .. text:sub(1, 120) }
  end
  local proto = tonumber(d.protocol)
  if not proto or proto < PROTOCOL then
    return { error = "this ppu is too old: update it (" .. ppu_bin() .. " does not answer with protocol " .. PROTOCOL .. ")", reason = "too_old" }
  end
  if proto > PROTOCOL then
    return { error = "this plugin is too old for this ppu: run ppu art install aseprite again", reason = "plugin_too_old" }
  end
  if d.ok == false and d.error then return { error = tostring(d.error), reason = d.reason and tostring(d.reason) } end
  local res = { ok = d.ok == true, findings = {}, budgets = d.budgets, unit = d.unit, source = d.source, path = d.path,
    kind = d.kind, row = tonumber(d.row), transparent = istab(d.transparent) and {} or nil, palette = d.palette, palette_file = d.palette_file,
    plte = istab(d.plte) and d.plte or nil, plte_own = tonumber(d.plte_own), options = istab(d.options) and d.options or nil }
  if res.transparent then for _, i in ipairs(d.transparent) do res.transparent[#res.transparent + 1] = tonumber(i) end end
  if not istab(d.findings) then return { error = "the check's JSON has no findings list: " .. text:sub(1, 120) } end
  for _, f in ipairs(d.findings) do
    local x, y = tonumber(f.x), tonumber(f.y)
    res.findings[#res.findings + 1] = {
      rule = tostring(f.rule), -- an unknown rule is still a finding
      message = tostring(f.message or f.rule or "finding"),
      x = x and math.floor(x), y = y and math.floor(y),
    }
  end
  if not res.ok and #res.findings == 0 then
    res.findings[1] = { rule = "unknown", message = "the check reported a problem without a finding: " .. text:sub(1, 120) }
  end
  return res
end

-- The check's answer from its stdout `out`; when it printed nothing, the stderr `err` says why.
local function result_from(out, err)
  local res = M.parse(out)
  if res.error and out:match("^%s*$") and err and not err:match("^%s*$") then res.error = (err:gsub("%s+$", "")) end
  return res
end

local function result_from_files(out_path, err_path)
  local out = readfile(out_path)
  if out == nil then return nil end
  return result_from(out, readfile(err_path))
end

-- A live job's files: its stdin bytes first, its args last. The job's number, or nil + why. Every other
-- live job is superseded (one state at a time, one job per state): what they left is swept.
local function stage_live(png, snap)
  sweep("live", "job.")
  seq = seq + 1
  local ok, why = stage("live", livename(seq),
    { "art", "check", png, "--stdin", "--width", tostring(snap.width), "--height", tostring(snap.height), "--json" }, snap.bytes)
  if not ok then return nil, why end
  return seq
end

-- The live job's result, once ppu has written its .code (the last file: out and err are complete by then).
local function live_result(n)
  if not readfile(livefile(n, "code")) then return nil end
  return result_from_files(livefile(n, "out"), livefile(n, "err"))
end

-- ---------------------------------------------------------------------------------------------
-- Opening: indexed, the .gpl, the grid, transparent entries.
-- ---------------------------------------------------------------------------------------------
local function colour_key(pal, i) return pal:getColor(i).rgbaPixel end

local function valid(spr)
  if spr == nil then return false end
  local ok, v = pcall(function() return spr.isValid end)
  if not ok then return false end
  return v ~= false -- isValid arrived in 1.3.16; older builds raise or return nil
end

local function set_status(st, text)
  st.status = text
  if st.dlg then st.dlg:modify { id = "status", text = text } end
end

local function snapshot(st)
  local img = Image(st.sprite.spec)
  img:drawSprite(st.sprite, 1)
  return img
end

local function repaint(st)
  if not st.dlg then return end
  if st.refresh then st.refresh() end
  st.dlg:repaint()
end

local function stop_timers(st)
  if st.debounce then st.debounce:stop() end
  if st.poll then st.poll:stop() end
end

local function teardown(st, why)
  stop_timers(st)
  st.dead = true
  if st.listener and valid(st.sprite) then pcall(function() st.sprite.events:off(st.listener) end) end
  if st.job then remove_job(st.job) end
  if st.dlg then local d = st.dlg; st.dlg = nil; pcall(function() d:close() end) end
  if S == st then S = nil end
  if why then tried[st.png] = nil end
end

local function verdict_text(res)
  if res.error then return "Check failed: " .. res.error end
  if res.ok then return "No findings: the art would import." end
  return #res.findings .. " finding" .. (#res.findings == 1 and "" or "s")
end

local function finish(st, res)
  st.running = false
  st.poll:stop()
  st.result = res
  set_status(st, verdict_text(res) .. (st.saved and ("  " .. st.saved) or ""))
  st.saved = nil
  repaint(st)
  if st.again then st.again = false; M.start_check(st) end
end

-- Start a check in the background. Never blocks: `ppu art job --detach` returns at once.
function M.start_check(st)
  if st.dead or not valid(st.sprite) then return end
  if st.running then st.again = true; return end -- one at a time; run once more when it ends
  local snap = snapshot(st)
  local n, why = stage_live(st.png, snap)
  if not n then st.result = { error = why }; set_status(st, "Check failed: " .. why); repaint(st); return end
  st.snap, st.job, st.running, st.started, st.stale = snap, n, true, os.time(), false
  set_status(st, "Checking…")
  os.execute(runner("live", true))
  st.poll:start()
end

-- Test-only synchronous path: the same files, the same job script, the same parser.
function M.check_sync(st)
  local snap = snapshot(st)
  local n, why = stage_live(st.png, snap)
  if not n then return { error = why } end
  os.execute(runner("live"))
  local res = result_from_files(livefile(n, "out"), livefile(n, "err"))
  remove_job(n)
  return res or { error = "the check left no result" }
end

local function on_poll(st)
  if st.dead then return end
  if not valid(st.sprite) then teardown(st, true); return end
  local res = live_result(st.job)
  if res then remove_job(st.job); finish(st, res)
  elseif os.time() - st.started > GIVE_UP then remove_job(st.job); finish(st, { error = "the check did not finish in " .. GIVE_UP .. " s" }) end
end

M.poll = on_poll -- test-only: timers do not fire in batch mode, so the test steps the poller itself

-- Any change, an undo included, only arms the check: no file is written outside a save or a command.
local function on_change(st)
  if st.dead then return end
  st.debounce:stop(); st.debounce:start() -- the only work on the stroke path (Timer has no restart)
end

-- ppu's `reason` for a PNG that belongs to no project (not_project) or is no source's file
-- (not_listed): such a PNG is none of the plugin's business and is left alone without a word.
local function unlisted(res) return res.reason == "not_project" or res.reason == "not_listed" end

-- The one synchronous check at open (not the stroke path): what the manifest says about this PNG
-- and the verdict on the file as saved.
local function open_check(png)
  local r = run_art({ "art", "check", png })
  if not r.code then return { error = r.err } end
  return result_from(r.out, r.err)
end

function M.debounce_tick(st) -- public so the batch test can fire it (timers do not tick in batch mode)
  st.debounce:stop()
  if not valid(st.sprite) then teardown(st, true); return end
  if st.dlg then M.start_check(st) else st.stale = true end -- no panel to show it: check when one opens
end

-- The sprite's colours as "r g b" lines, the text `ppu art palette write` reads.
local function rgbs(pal)
  local t = {}
  for i = 0, #pal - 1 do local c = pal:getColor(i); t[#t + 1] = string.format("%d %d %d", c.red, c.green, c.blue) end
  return t
end

-- The named .gpl as it is on disk (Aseprite's own reader; ppu wrote the file).
local function load_gpl(path)
  -- a closure, not pcall(Palette, ...): Aseprite names the calling script by the Lua frame, and a C frame reads "=[C]"
  local okp, gp = pcall(function() return Palette { fromFile = path } end)
  if not okp or gp == nil or #gp == 0 then return nil, "cannot read the palette " .. path end
  return gp
end

-- The check's `plte` (the PLTE the art means: Mode 7 EXTBG's is the .gpl mirrored to 256) as a
-- Palette, opaque. nil + why for an entry that is not [r, g, b].
local function plte_palette(list)
  if #list == 0 then return nil, "the check's PLTE is empty" end
  local p = Palette(#list)
  for i, c in ipairs(list) do
    local r, g, b
    if istab(c) then r, g, b = tonumber(c[1]), tonumber(c[2]), tonumber(c[3]) end
    if not (r and g and b) then return nil, "the check's PLTE entry " .. (i - 1) .. " is not [r, g, b]" end
    p:setColor(i - 1, Color { r = r, g = g, b = b, a = 255 })
  end
  return p
end

-- Sets the sprite up as the check `res` describes: the art's PLTE (from the .gpl) as palette, the
-- transparent entries alpha 0, the grid. One transaction, one undo step. Fills st.fresh / st.alpha_set /
-- st.palette_replaced (what open needs to know). nil, or why it cannot.
local function setup(st, res)
  local spr = st.sprite
  local pal = spr.palettes[1]
  local raw
  if res.palette_file then
    if not res.plte or not res.plte_own then
      return "this ppu is too old: update it (" .. ppu_bin() .. " reports no PLTE for " .. res.source .. ")"
    end
    local why
    raw, why = load_gpl(res.palette_file)
    if not raw then return why end
    local gp
    gp, why = plte_palette(res.plte)
    if not gp then return why end
    local differs
    -- an own entry the sprite changed since it took `lines` is an unsaved edit: it stays, the rest follows the .gpl
    local mem = taken[st.png]
    local lines, prev = rgbs(gp), mem and mem.sprite == spr and mem.lines
    for i = 0, math.min(res.plte_own, #pal, #lines) - 1 do
      local c = pal:getColor(i)
      if prev and string.format("%d %d %d", c.red, c.green, c.blue) ~= prev[i + 1] then
        gp:setColor(i, Color { r = c.red, g = c.green, b = c.blue, a = 255 })
        lines[i + 1] = prev[i + 1]
      end
    end
    taken[st.png] = { sprite = spr, lines = lines }
    differs = #gp ~= #pal
    for i = 0, math.min(#gp, #pal) - 1 do
      local a, b = gp:getColor(i), pal:getColor(i)
      if a.red ~= b.red or a.green ~= b.green or a.blue ~= b.blue then differs = true; break end
    end
    st.palette_replaced = differs
    pal = gp
  else
    pal = Palette(pal)
  end
  st.alpha_set = false
  local orig = spr.palettes[1]
  for _, i in ipairs(res.transparent) do
    if i >= #pal then -- the check's PLTE and its transparent list disagree: a ppu bug, never skipped
      return string.format("%s: transparent entry %d is past the end of the art's palette (%d colours)", res.source, i, #pal)
    end
    if i >= #orig or orig:getColor(i).alpha ~= 0 then st.alpha_set = true end
    local c = pal:getColor(i)
    pal:setColor(i, Color { r = c.red, g = c.green, b = c.blue, a = 0 })
  end
  st.gpl = res.palette_file
  st.written = raw and rgbs(raw) or nil -- what the .gpl holds: a later colour edit differs from it
  st.own = res.plte_own -- the leading entries that are the .gpl's; past them the PLTE follows them
  st.plte = raw and rgbs(pal) or nil
  st.fresh = not spr.isModified -- before our own setup edits it
  local grid = Rectangle(0, 0, st.cw, st.ch)
  local same = #orig == #pal and spr.gridBounds == grid
  for i = 0, #pal - 1 do if not same then break end; same = colour_key(orig, i) == colour_key(pal, i) end
  if not same then -- already set up (a sprite opened again): no new undo step, not marked modified
    -- one transaction: the setup is a single undo step named for what it is (Aseprite has no way to apply it without one)
    local okt, err = pcall(app.transaction, "Open as SNES art", function()
      spr:setPalette(pal)
      spr.gridBounds = grid
    end)
    if not okt then return "cannot set up " .. res.source .. ": " .. tostring(err) end
  end
  return nil
end

function M.open(spr)
  local png = spr.filename
  local res = open_check(png)
  if res.error then
    if unlisted(res) then return nil end
    return nil, res.error
  end
  if res.path == "keep" and not res.transparent then
    return nil, "this ppu is too old: update it (" .. ppu_bin() .. " reports no transparent entries)"
  end
  if res.path ~= "keep" or not res.row or not res.unit then
    return nil, res.source .. " is not indexed SNES art (it is full colour, or an indexed PNG without its transparent " ..
      "entries): Sprite > Convert to SNES art… makes it SNES art (the original is kept)"
  end
  if spr.colorMode ~= ColorMode.INDEXED then
    return nil, res.source .. " is not an indexed PNG (it is full colour): Sprite > Convert to SNES art… " ..
      "makes it SNES art (the original is kept)"
  end
  local st = { sprite = spr, png = png, entry = { name = res.source, kind = res.kind }, row = res.row,
    cw = res.unit.width, ch = res.unit.height, warning = "", result = res }
  local why = setup(st, res)
  if why then return nil, why end
  pcall(function() app.preferences.document(spr).show.grid = true end)
  -- the verdict on the saved file is the sprite's verdict only if setup changed nothing (a replaced
  -- palette or new alpha would make it describe a file we no longer hold); else check at once
  local as_saved = st.fresh and not st.palette_replaced and not st.alpha_set
  st.snap = as_saved and snapshot(st) or nil
  st.result = as_saved and res or nil
  if as_saved then set_status(st, verdict_text(res)) end
  if st.palette_replaced then st.warning = "The PNG's own palette differed from " .. res.palette .. ".gpl; the .gpl was loaded." end
  st.poll = Timer { interval = POLL, ontick = function() on_poll(st) end }
  st.debounce = Timer { interval = DEBOUNCE, ontick = function() M.debounce_tick(st) end }
  st.listener = spr.events:on("change", function() on_change(st) end)
  if S and S ~= st then teardown(S) end
  S = st
  if not as_saved then M.start_check(st) end
  return st
end

-- ---------------------------------------------------------------------------------------------
-- Editing palettes and rows. ppu writes the .gpl and ppu.json and decides every remap; the plugin
-- passes the sprite's bytes in and applies the bytes that come out, each gesture in one transaction.
-- ---------------------------------------------------------------------------------------------
local function same_lines(a, b)
  if #a ~= #b then return false end
  for i = 1, #a do if a[i] ~= b[i] then return false end end
  return true
end

-- What the .gpl would hold now: the sprite's colours for its own entries, the .gpl's lines past them.
local function gpl_lines(st)
  local p, t = st.sprite.palettes[1], {}
  for i = 1, #st.written do
    local c = i <= st.own and i <= #p and p:getColor(i - 1)
    t[i] = c and string.format("%d %d %d", c.red, c.green, c.blue) or st.written[i]
  end
  return t
end

-- Called when the sprite is saved: writes the sprite's palette colours to the .gpl (and syncs every PNG
-- that uses it) when they differ from what the .gpl holds, then takes the snapped colours (RGB555, EXTBG
-- mirror) into the sprite in one transaction. true when the .gpl is current, nil + why when it is not.
local function sync_palette(st)
  if not st.gpl or st.dead or not valid(st.sprite) then return true end
  for i = st.own, math.min(#st.sprite.palettes[1], #st.plte) - 1 do
    local c = st.sprite.palettes[1]:getColor(i)
    if string.format("%d %d %d", c.red, c.green, c.blue) ~= st.plte[i + 1] then
      st.warning = string.format("Entry %d is past %s's own %d colours: it follows them, so its edit is not saved.",
        i, app.fs.fileName(st.gpl), st.own)
      if st.dlg then st.dlg:modify { id = "warning", text = st.warning } end
      break
    end
  end
  local now = gpl_lines(st)
  if same_lines(now, st.written) then return true end
  local r = run_art({ "art", "palette", "write", st.gpl }, table.concat(now, "\n") .. "\n")
  if r.code ~= 0 then return nil, r.err ~= "" and r.err or "ppu failed" end
  local gp, why = load_gpl(st.gpl)
  if not gp then return nil, "the palette was written but not reloaded: " .. why end
  local synced = 0
  if r.answer and istab(r.answer.synced) then for _ in ipairs(r.answer.synced) do synced = synced + 1 end end
  st.written = rgbs(gp)
  taken[st.png].lines = st.written -- the sprite has now taken (and will snap to) what the .gpl holds
  -- the PLTE the saved .gpl makes (snapped to RGB555, and mirrored for EXTBG): ppu's, never derived here
  local res = open_check(st.png)
  local plte
  if res.plte then plte, why = plte_palette(res.plte) end
  if not plte then return nil, "the palette was written but not reloaded: " .. tostring(why or res.error or "the check reports no PLTE") end
  st.plte = rgbs(plte)
  st.saved = string.format("%s saved, %d PNG%s synced.", app.fs.fileName(st.gpl), synced, synced == 1 and "" or "s")
  set_status(st, st.saved)
  if not same_lines(rgbs(st.sprite.palettes[1]), st.plte) then
    local ok, err = pcall(app.transaction, "Snap to SNES colours", function()
      local p = st.sprite.palettes[1]
      for i = 0, math.min(#plte, #p) - 1 do
        local c, a = plte:getColor(i), p:getColor(i).alpha
        p:setColor(i, Color { r = c.red, g = c.green, b = c.blue, a = a })
      end
    end)
    if not ok then return nil, app.fs.fileName(st.gpl) .. " is saved, but the sprite could not take its snapped colours: " .. tostring(err) end
  end
  return true
end

-- app.events aftercommand: the artist saved the sprite. Never raises into Aseprite. A Save As to another
-- path writes nothing: the .gpl belongs to the PNG that was opened, and the new file is not in the project
-- until it is listed there (then opening it as SNES art starts afresh).
function M.after_command(ev)
  if ev.name ~= "SaveFile" and ev.name ~= "SaveFileAs" then return end
  local st = S
  if not st or st.dead or not valid(st.sprite) or app.sprite ~= st.sprite then return end
  if st.sprite.isModified then return end -- a cancelled or failed save leaves it modified: nothing was saved, nothing is written
  local ok, res, why
  if st.sprite.filename ~= st.png then
    ok, res, why = true, nil, "the sprite is now " .. app.fs.fileName(st.sprite.filename) .. ", not " ..
      app.fs.fileName(st.png) .. ", so the palette was not written"
  else
    ok, res, why = pcall(sync_palette, st)
    if ok and res then return end
    if not ok then why = res end
  end
  st.warning = "Palette not saved: " .. tostring(why)
  if st.dlg then st.dlg:modify { id = "warning", text = st.warning } end
end

function M.watch_saves() app.events:on("aftercommand", function(ev) M.after_command(ev) end) end

-- Index bytes of the whole sprite, or why the row commands cannot work on it.
local function row_input(st)
  local spr = st.sprite
  if #spr.layers ~= 1 or #spr.frames ~= 1 then
    return nil, "Tile rows work on a sprite of one layer and one frame (this has " .. #spr.layers .. " layers, " ..
      #spr.frames .. " frames): flatten it first."
  end
  -- art that names no palette keeps its colours in the PNG alone: ppu reads them from disk, so it would remap
  -- against colours the user has already changed
  if not st.gpl then
    local okp, disk = pcall(function() return Palette { fromFile = st.png } end) -- a closure: see load_gpl
    if not okp or disk == nil then return nil, "cannot read the colours of " .. st.png end
    if not same_lines(rgbs(disk), rgbs(spr.palettes[1])) then
      return nil, "The art's colours are not saved yet: save the sprite first."
    end
  end
  return snapshot(st).bytes
end

local function size_args(st) return { "--width", tostring(st.sprite.width), "--height", tostring(st.sprite.height) } end

-- Lays ppu's index bytes over the sprite in one transaction `name` (not when nothing changed).
local function apply_pixels(st, name, bytes, was)
  if bytes == was then return end
  local ok, err = pcall(app.transaction, name, function()
    local img = Image(st.sprite.spec)
    img.bytes = bytes
    st.sprite.cels[1].image = img
    st.sprite.cels[1].position = Point(0, 0)
  end)
  if not ok then error(err, 0) end
end

local function result_lines(err)
  local t = {}
  for l in (err .. "\n"):gmatch("(.-)\n") do if l ~= "" then t[#t + 1] = l end end
  return t
end

-- Moves the selected tiles or cells to the foreground colour's palette row. true, or false + why.
function M.set_tile_row(st)
  local spr = st.sprite
  local input, why = row_input(st)
  if not input then return false, why end
  local sel = spr.selection
  if sel.isEmpty then return false, "Select the tiles or cells to move first (the rectangle select tool)." end
  local b = sel.bounds:intersect(Rectangle(0, 0, spr.width, spr.height))
  if b.isEmpty then return false, "The selection is outside the sprite." end
  local row = math.floor(app.fgColor.index / st.row)
  local args = { "art", "rows", st.png, "--set", string.format("%d", row), "--rect", string.format("%d,%d,%d,%d", b.x, b.y, b.width, b.height) }
  for _, a in ipairs(size_args(st)) do args[#args + 1] = a end
  local r = run_art(args, input)
  if r.code ~= 0 then return false, r.err ~= "" and r.err or "ppu failed" end
  apply_pixels(st, "Set tile row", r.pixels, input)
  set_status(st, string.format("Set tile row %d.", row))
  return true
end

-- Remaps stray pixels into their tile's row. true, or false + why; what stayed goes to the panel.
function M.fix_tile_rows(st)
  local input, why = row_input(st)
  if not input then return false, why end
  local args = { "art", "rows", st.png, "--fix" }
  for _, a in ipairs(size_args(st)) do args[#args + 1] = a end
  local r = run_art(args, input)
  if r.code ~= 0 then return false, r.err ~= "" and r.err or "ppu failed" end
  apply_pixels(st, "Fix tile rows", r.pixels, input)
  local lines = r.notes
  set_status(st, lines[1] or "Fixed tile rows.")
  st.warning = table.concat(lines, " ", 2, math.min(#lines, 3)) .. (#lines > 3 and " …" or "")
  if st.dlg then st.dlg:modify { id = "warning", text = st.warning } end
  return true
end

-- Selects the tiles or cells the last check flagged, so the offenders are marked on the canvas.
-- true, or false + why. Findings without a position (budgets) have nothing to select.
function M.select_bad_tiles(st)
  local r = st.result
  if not r then return false, "No check result yet: wait for the check." end
  if r.error then return false, "The check failed, so there is nothing to select: " .. r.error end
  local uw, uh = (r.unit and r.unit.width) or st.cw, (r.unit and r.unit.height) or st.ch
  local sel, n = Selection(), 0
  for _, f in ipairs(r.findings) do
    if f.x and f.y then sel:add(Selection(Rectangle(f.x * uw, f.y * uh, uw, uh))); n = n + 1 end
  end
  if n == 0 then return false, "The check flagged no tile or cell to select." end
  st.sprite.selection = sel
  set_status(st, string.format("Selected %d flagged %s.", n, n == 1 and "tile" or "tiles"))
  return true
end

-- ---------------------------------------------------------------------------------------------
-- The panel.
-- ---------------------------------------------------------------------------------------------
local function paint_palette(st, gc)
  local pal = st.sprite.palettes[1]
  local per = math.min(st.row, 16) -- an 8 bpp block is shown 16 x 16
  local sz = 10
  gc.strokeWidth = 1
  for i = 0, #pal - 1 do
    local x, y = (i % per) * sz, (i // per) * sz
    local c = pal:getColor(i)
    if c.alpha == 0 then
      gc.color = Color { r = 200, g = 200, b = 200 }; gc:fillRect(Rectangle(x, y, sz - 1, sz - 1))
      gc.color = Color { r = 140, g = 140, b = 140 }; gc:fillRect(Rectangle(x, y, sz // 2, sz // 2))
      gc:fillRect(Rectangle(x + sz // 2, y + sz // 2, sz // 2 - 1, sz // 2 - 1))
    else
      gc.color = Color { r = c.red, g = c.green, b = c.blue }; gc:fillRect(Rectangle(x, y, sz - 1, sz - 1))
    end
  end
end

local function paint_map(st, gc, scale)
  local img = st.snap or snapshot(st)
  gc:drawImage(img, Rectangle(0, 0, img.width, img.height), Rectangle(0, 0, math.floor(img.width * scale), math.floor(img.height * scale)))
  local r = st.result
  if not r or not r.findings then return end
  local uw, uh = (r.unit and r.unit.width) or st.cw, (r.unit and r.unit.height) or st.ch
  gc.color = Color { r = 255, g = 40, b = 40 }
  gc.strokeWidth = 2
  for _, f in ipairs(r.findings) do
    if f.x and f.y then
      gc:strokeRect(Rectangle(math.floor(f.x * uw * scale) - 1, math.floor(f.y * uh * scale) - 1,
        math.max(4, math.floor(uw * scale) + 2), math.max(4, math.floor(uh * scale) + 2)))
    end
  end
end

local function budget_text(st)
  local b = st.result and st.result.budgets
  if not b then return "Tiles: -    Palettes: -" end
  local function one(v) return v and (math.floor(v.used) .. " / " .. math.floor(v.max)) or "-" end
  return "Tiles: " .. one(b.tiles) .. "    Palettes: " .. one(b.palettes)
end

local function list_text(st, k)
  local r = st.result
  if not r then return "" end
  if r.error then return k == 1 and r.error or "" end
  if k > MAX_LISTED then return "" end
  if k == MAX_LISTED and #r.findings > MAX_LISTED then return "… and " .. (#r.findings - MAX_LISTED + 1) .. " more" end
  local f = r.findings[k]
  if not f then return "" end
  local where = (f.x and f.y) and (" [" .. f.x .. "," .. f.y .. "]") or ""
  return f.rule .. where .. ": " .. f.message:sub(1, 90)
end

-- Closing the panel stops the timers; showing it again picks up a check still running, or one a
-- change asked for while the panel was closed.
local function panel_closed(st)
  if not st.dlg then return end
  st.dlg = nil
  if st.debounce.isRunning then st.stale = true end -- its check waits for the panel
  stop_timers(st) -- no panel, no verdict to show
end

local function panel_shown(st)
  if st.dead then return end
  if st.running then
    if st.stale then st.stale, st.again = false, true end -- a change the running job never saw: check again when it ends
    st.poll:start()
  elseif st.stale then M.start_check(st) end
end

M.panel_closed, M.panel_shown = panel_closed, panel_shown -- test-only: a batch run has no Dialog

function M.show_panel(st)
  st = st or S
  if not st or st.dlg then return end
  local img = st.sprite
  local scale = math.min(1, 256 / img.width, 256 / img.height)
  local per = math.min(st.row, 16)
  local d = Dialog { title = PANEL, onclose = function() panel_closed(st) end }
  d:label { text = string.format("%s: %s, %dx%d, grid %dx%d, rows of %d", st.entry.name, st.entry.kind, img.width, img.height, st.cw, st.ch, st.row) }
  d:newrow()
  d:label { id = "status", text = st.status or "Checking…" }
  d:newrow()
  d:label { id = "budgets", text = budget_text(st) }
  d:newrow()
  d:label { id = "warning", text = st.warning }
  d:newrow()
  -- one gesture each; the work is M.set_tile_row / fix_tile_rows / select_bad_tiles
  d:button { text = "Set tile row", onclick = function() M.command(M.set_tile_row) end }
  d:button { text = "Fix tile rows", onclick = function() M.command(M.fix_tile_rows) end }
  d:button { text = "Select bad tiles", onclick = function() M.command(M.select_bad_tiles) end }
  d:newrow()
  d:canvas { id = "palette", width = per * 10, height = ((#img.palettes[1] + per - 1) // per) * 10,
    onpaint = function(ev) paint_palette(st, ev.context) end }
  d:newrow()
  d:canvas { id = "map", width = math.floor(img.width * scale), height = math.floor(img.height * scale),
    onpaint = function(ev) paint_map(st, ev.context, scale) end }
  for k = 1, MAX_LISTED do d:newrow(); d:label { id = "f" .. k, text = "" } end
  st.dlg = d
  d:show { wait = false }
  st.refresh = function()
    d:modify { id = "budgets", text = budget_text(st) }
    for k = 1, MAX_LISTED do d:modify { id = "f" .. k, text = list_text(st, k) } end
  end
  st.refresh()
  panel_shown(st)
end

-- ---------------------------------------------------------------------------------------------
-- Wiring: sitechange opens project PNGs; one menu command reopens the panel.
-- ---------------------------------------------------------------------------------------------
function M.open_and_show(force)
  local spr = app.sprite
  if not spr then return end
  if S and not S.dead and spr == S.sprite and valid(S.sprite) then M.show_panel(S); return end
  local png = spr.filename
  if not png:lower():match("%.png$") then
    if force then app.alert("SNES art works on PNG files listed in a ppu.json.") end
    return
  end
  if not force and tried[png] then return end
  local st, msg = M.open(spr)
  if st then
    tried[png] = nil
    M.show_panel(st)
  elseif msg then
    tried[png] = true
    app.alert { title = PANEL, text = msg }
  elseif force then
    app.alert { title = PANEL, text = png .. " is not listed in any ppu.json above it." }
  else
    tried[png] = true -- in no project: sitechange (every frame, layer, cel) must not run ppu again
  end
end

-- The watched SNES-art sprite, when it is the active one.
local function active_state()
  if S and not S.dead and valid(S.sprite) and app.sprite == S.sprite then return S end
  app.alert { title = PANEL, text = "Open a project PNG as SNES art first (Sprite > SNES art panel)." }
end

-- app.alert does not split its text on newlines: a table is one line per entry. Capped, so a long
-- list of unmatched colours still fits the screen.
local ALERT_LINES = 10
function M.alert_lines(text)
  local all = result_lines(text)
  if #all <= ALERT_LINES then return all end
  local t = { table.unpack(all, 1, ALERT_LINES) }
  t[#t + 1] = "…"
  return t
end

-- Runs one gesture and tells the author why when it could not happen.
function M.command(fn)
  local st = active_state()
  if not st then return end
  local ok, why = fn(st)
  if not ok and why then app.alert { title = PANEL, text = M.alert_lines(why) } end
end

-- New SNES art: the dialog only collects fields; `ppu art new` writes every file and refuses what is
-- not legal (sizes, bit depths), so its message is the answer. fields: png, kind, size ("WxH"),
-- bpp and cell (optional), and one of palette (an existing .gpl) or new_palette (a name).
-- ok, png: the PNG is made and opened (sitechange sets it up as SNES art). false, why: ppu's message.
function M.new_art(fields)
  local args = { "art", "new", tostring(fields.png or ""), "--kind", tostring(fields.kind or ""),
    "--size", tostring(fields.size or "") }
  if fields.bpp then args[#args + 1] = "--bpp"; args[#args + 1] = tostring(fields.bpp) end
  if fields.cell then args[#args + 1] = "--cell"; args[#args + 1] = tostring(fields.cell) end
  if fields.palette and fields.palette ~= "" then
    args[#args + 1] = "--palette"; args[#args + 1] = fields.palette
  elseif fields.new_palette and fields.new_palette ~= "" then
    args[#args + 1] = "--new-palette"; args[#args + 1] = fields.new_palette
  end
  local r = run_art(args)
  if r.code ~= 0 then return false, r.err ~= "" and r.err or "ppu art new failed (exit " .. tostring(r.code) .. ")" end
  -- A Lua closure, not pcall(app.open, ...): Aseprite's security check names the caller, and a bare
  -- C caller is "=[C]", which the extension's full trust does not cover, so the author got a prompt.
  local ok, spr = pcall(function() return app.open(fields.png) end)
  if not ok or not spr then return false, "ppu made " .. fields.png .. " but Aseprite cannot open it: " .. tostring(spr) end
  return true, fields.png
end

-- Works with no sprite open: it is a File command, not a sprite one.
function M.new_art_dialog()
  local d = Dialog("New SNES art")
  d:file { id = "png", label = "PNG", save = true, filetypes = { "png" } }
  d:combobox { id = "kind", label = "Kind", option = "bg", options = { "bg", "sheet", "obj", "m7" } }
  d:number { id = "w", label = "Width", text = "256", decimals = 0 }
  d:number { id = "h", label = "Height", text = "256", decimals = 0 }
  d:combobox { id = "bpp", label = "Bit depth", option = "kind's default", options = { "kind's default", "2", "4", "8" } }
  d:number { id = "cell", label = "Obj cell (0 = default)", text = "0", decimals = 0 }
  d:radio { id = "newpal", label = "Palette", text = "New palette named", selected = true }
  d:entry { id = "name" }
  d:radio { id = "oldpal", text = "Existing .gpl" }
  d:file { id = "gpl", open = true, filetypes = { "gpl" } }
  d:button { id = "ok", text = "Create", focus = true }
  d:button { text = "Cancel" }
  d:show()
  local v = d.data
  if not v.ok then return true end
  local f = { png = v.png, kind = v.kind, size = string.format("%dx%d", v.w, v.h) }
  if v.bpp ~= "kind's default" then f.bpp = v.bpp end
  if v.cell and v.cell > 0 then f.cell = math.floor(v.cell) end
  if v.oldpal then f.palette = v.gpl else f.new_palette = v.name end
  local ok, why = M.new_art(f)
  if not ok then app.alert { title = "New SNES art", text = M.alert_lines(why) } end
  return ok
end

-- ---------------------------------------------------------------------------------------------
-- Convert to SNES art: `ppu art convert` does all of it (the converter, the .gpl, ppu.json, the
-- backup); the plugin saves the sprite, passes the dialog's values as flags and reopens the result.
-- ---------------------------------------------------------------------------------------------
local KINDS = { "bg", "sheet", "obj", "m7" }
local DITHERS = { "none", "bayer", "diffusion" }
local function has(list, v) for _, x in ipairs(list) do if x == v then return true end end end

local function png_refusal(spr)
  if not spr.filename:lower():match("%.png$") then
    return "Convert to SNES art works on a PNG file: save the sprite as a PNG first."
  end
end

-- The dialog's starting values: what ppu.json stores for this PNG (ppu's check reports it as
-- `options`), else the converter's defaults. fields, or nil + why. listed: ppu.json lists the PNG.
-- fields.prefill keeps these values: convert passes only what the author changed, so ppu keeps
-- the entry's own options (and its defaults for the rest) instead of having them pinned.
function M.convert_fields(spr)
  local why = png_refusal(spr)
  if why then return nil, why end
  local png = spr.filename
  local res = open_check(png)
  local listed = not res.error
  -- unlisted inside a project is fine (convert lists it); outside any project ppu would refuse it
  if res.error and res.reason ~= "not_listed" then return nil, res.error end
  local kind = listed and res.kind or "bg"
  if not has(KINDS, kind) then return nil, png .. " is listed with the kind " .. tostring(kind) .. ", which is not art ppu can convert to" end
  local o = listed and res.options or {}
  -- json numbers are floats: 4.0 would print "4.0", which no combobox option matches (it showed "2")
  local function int(v) local n = tonumber(v); return n and (math.tointeger(n) or n) end
  local f = { listed = listed, kind = kind, new_palette = app.fs.fileTitle(png) .. "_pal",
    bpp = int(o.bit_depth) or (kind == "m7" and 8 or 4), cell = int(o.cell_size) or 8, stored_height = o.cell_height ~= nil,
    dither = o.dither and tostring(o.dither) or "none", dither_strength = int(o.dither_strength) or 50,
    alpha_threshold = int(o.alpha_threshold) or 128, max_palettes = int(o.max_palettes) or 8 }
  f.prefill = {}
  for k, v in pairs(f) do if k ~= "prefill" then f.prefill[k] = v end end
  return f
end

-- The flags `ppu art convert` reads for the kind, and only those (it refuses one a kind ignores).
-- With f.prefill, a value the author left as prefilled is not passed. args, or nil + why for a
-- value that is not what the dialog offers.
local function convert_args(png, f)
  -- "4" (a combobox) and 4 (the prefill) are the same value; "none" and "bayer" are not (both tonumber nil)
  local function kept(name)
    local p, v = f.prefill and f.prefill[name], f[name]
    if p == nil or v == nil then return false end
    return p == v or (tonumber(p) ~= nil and tonumber(p) == tonumber(v))
  end
  if not has(KINDS, f.kind) then return nil, "unknown kind " .. tostring(f.kind) .. " (bg, sheet, obj or m7)" end
  local args = { "art", "convert", png, "--kind", f.kind }
  local function num(flag, name)
    if kept(name) then return end
    local n = tonumber(f[name])
    if not n or n ~= math.floor(n) then return "the " .. name:gsub("_", " ") .. " must be a whole number, not " .. tostring(f[name]) end
    args[#args + 1] = flag; args[#args + 1] = string.format("%d", n)
  end
  local bad
  if f.kind == "bg" or f.kind == "sheet" then bad = num("--bpp", "bpp") end
  if not bad and f.kind == "obj" then
    bad = num("--cell", "cell")
    -- the dialog offers square cells: a new cell must not keep a stored height
    if not bad and not kept("cell") and f.stored_height then args[#args + 1] = "--cell-height"; args[#args + 1] = args[#args - 1] end
  end
  if f.kind ~= "m7" then
    if not has(DITHERS, f.dither) then return nil, "unknown dither " .. tostring(f.dither) .. " (none, bayer or diffusion)" end
    if not kept("dither") then args[#args + 1] = "--dither"; args[#args + 1] = f.dither end
    bad = bad or num("--dither-strength", "dither_strength") or num("--alpha-threshold", "alpha_threshold")
  end
  if not bad and (f.kind == "bg" or f.kind == "sheet") then bad = num("--max-palettes", "max_palettes") end
  if bad then return nil, bad end
  if f.new_palette and f.new_palette ~= "" then args[#args + 1] = "--new-palette"; args[#args + 1] = f.new_palette end
  return args
end

local function whole(v) local n = tonumber(v); return n and string.format("%d", n) or tostring(v) end -- json numbers are floats
local function used_of(v) return istab(v) and (whole(v.used) .. "/" .. whole(v.max)) or "?" end

-- Converts the sprite's PNG in place. true, lines (what the import made, where the original is kept),
-- the reopened sprite | false, why. A refusal leaves the sprite open as it was, and ppu wrote nothing
-- (the save before it stands);
-- the conversion is a file operation, so it has no undo.
function M.convert(spr, f)
  local why = png_refusal(spr)
  if why then return false, why end
  if #spr.frames ~= 1 then return false, "Convert to SNES art works on a sprite of one frame (this has " .. #spr.frames .. "): flatten it first." end
  local png = spr.filename
  local args, bad = convert_args(png, f)
  if not args then return false, bad end
  if spr.isModified then
    app.sprite = spr
    app.command.SaveFile()
    if spr.isModified then return false, "The sprite was not saved, so it was not converted." end
  end
  local r = run_art(args)
  local d = r.answer
  if r.code ~= 0 then
    return false, (r.err ~= "" and r.err) or ("ppu art convert failed (exit " .. tostring(r.code) .. ")")
  end
  if not d or d.ok ~= true then
    return false, "ppu exited 0 but its answer was not the expected JSON, so " .. png ..
      " may have been converted (reopen it): " .. r.out:sub(1, 120)
  end
  tried[png] = nil
  if S and S.sprite == spr then teardown(S) end
  spr:close() -- the file changed under it: the stale sprite must go before the PNG can open again
  local ok, new = pcall(function() return app.open(png) end) -- a closure: see new_art
  if not ok or not new then return false, "ppu converted " .. png .. " but Aseprite cannot open it: " .. tostring(new) end
  local lines = { string.format("Converted %s: %s colours, tiles %s, palettes %s.", app.fs.fileName(png),
    whole(d.colours), used_of(d.tiles), used_of(d.palettes)) }
  if istab(d.notes) then for _, n in ipairs(d.notes) do lines[#lines + 1] = tostring(n) end end
  lines[#lines + 1] = "Original kept in " .. tostring(d.backup)
  return true, lines, new
end

function M.convert_dialog()
  local spr = app.sprite
  if not spr then app.alert { title = "Convert to SNES art", text = "Open a PNG first." }; return end
  local f, why = M.convert_fields(spr)
  if not f then app.alert { title = "Convert to SNES art", text = M.alert_lines(why) }; return end
  local d = Dialog("Convert to SNES art")
  d:label { text = f.listed and "Listed in ppu.json: its kind and options are prefilled." or "Not listed in a ppu.json: it is added when converted." }
  d:combobox { id = "kind", label = "Kind", option = f.kind, options = KINDS }
  d:combobox { id = "bpp", label = "Bit depth (bg, sheet)", option = tostring(f.bpp), options = { "2", "4", "8" } }
  d:number { id = "cell", label = "Obj cell (obj)", text = tostring(f.cell), decimals = 0 }
  d:entry { id = "new_palette", label = "New palette", text = f.new_palette }
  d:combobox { id = "dither", label = "Dither (not m7)", option = f.dither, options = DITHERS }
  d:number { id = "strength", label = "Dither strength", text = tostring(f.dither_strength), decimals = 0 }
  d:number { id = "alpha", label = "Alpha threshold", text = tostring(f.alpha_threshold), decimals = 0 }
  d:number { id = "maxpal", label = "Palette cap (bg, sheet)", text = tostring(f.max_palettes), decimals = 0 }
  d:button { id = "ok", text = "Convert", focus = true }
  d:button { text = "Cancel" }
  d:show()
  local v = d.data
  if not v.ok then return end
  local fields = { kind = v.kind, bpp = v.bpp, cell = v.cell, stored_height = f.stored_height, prefill = f.prefill, new_palette = v.new_palette,
    dither = v.dither, dither_strength = v.strength, alpha_threshold = v.alpha, max_palettes = v.maxpal }
  local ok, lines = M.convert(spr, fields)
  if not ok then app.alert { title = "Convert to SNES art", text = M.alert_lines(lines) }; return end
  M.open_and_show(false) -- sitechange may have shown the panel already: this makes sure
  app.alert { title = "Convert to SNES art", text = lines }
end

function init(plugin)
  if app.apiVersion == nil or app.apiVersion < MIN_API then
    app.alert { title = PANEL, text = "SNES art needs Aseprite 1.3 or newer (this is " .. tostring(app.version) .. ")." }
    return
  end
  plugin:newCommand { id = "SnesArtPanel", title = "SNES art panel", group = "sprite_properties",
    onclick = function() M.open_and_show(true) end }
  for _, c in ipairs {
    { "SnesArtSetRow", "SNES art: set tile row", M.set_tile_row },
    { "SnesArtFixRows", "SNES art: fix tile rows", M.fix_tile_rows },
    { "SnesArtSelectBad", "SNES art: select bad tiles", M.select_bad_tiles },
  } do
    plugin:newCommand { id = c[1], title = c[2], group = "sprite_properties", onclick = function() M.command(c[3]) end }
  end
  plugin:newCommand { id = "SnesArtConvert", title = "Convert to SNES art…", group = "sprite_properties", onclick = M.convert_dialog }
  plugin:newCommand { id = "SnesArtNew", title = "New SNES art…", group = "file_new", onclick = M.new_art_dialog }
  app.events:on("sitechange", function() M.open_and_show(false) end)
  M.watch_saves()
  M.open_and_show(false)
end

function exit(plugin)
  if S then teardown(S) end
end
