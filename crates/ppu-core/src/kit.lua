-- Sequencer sugar, run as the "kit" chunk before every user chunk (see
-- LuaEngine::set_sources). Defines note/instrument/sfx/bank/score (and a
-- `song` stub that only errors) as globals; a user chunk defining the same
-- name wins (chunks share one global env and run in order). Never shadow `voice`/`kon`/`koff`/`timer`
-- with locals here — they must resolve as globals at call time so a later
-- chunk can replace them too.
--
-- Note-name parsing has no stdlib help (no tonumber/string.byte/find in
-- this VM), so it's done with string.sub + table lookups, one char at a
-- time.

local NOTE_INDEX = { c = 0, d = 2, e = 4, f = 5, g = 7, a = 9, b = 11 }
local DIGIT = {}
for d = 0, 9 do
  DIGIT[tostring(d)] = d
end

-- Parse the octave substring of `s` starting at index `i` (1-based),
-- allowing a leading "-". Returns nil on anything but an optional sign
-- followed by one or more digits.
local function parse_octave(s, i)
  local len = string.len(s)
  local neg = false
  if string.sub(s, i, i) == "-" then
    neg = true
    i = i + 1
  end
  local n = 0
  local saw_digit = false
  while i <= len do
    local d = DIGIT[string.sub(s, i, i)]
    if d == nil then
      return nil
    end
    n = n * 10 + d
    saw_digit = true
    i = i + 1
  end
  if not saw_digit then
    return nil
  end
  if neg then
    n = -n
  end
  return n
end

-- Parse a note name ("C4", "C#4", "Db4", "A-1", case-insensitive letter)
-- into a MIDI number (C4 = 60, C-1 = 0), or nil if it isn't one.
local function parse_note_name(s)
  if string.len(s) < 2 then
    return nil
  end
  local step = NOTE_INDEX[string.lower(string.sub(s, 1, 1))]
  if step == nil then
    return nil
  end
  local i, accidental = 2, 0
  local c2 = string.sub(s, 2, 2)
  if c2 == "#" then
    accidental, i = 1, 3
  elseif string.lower(c2) == "b" then
    accidental, i = -1, 3
  end
  local octave = parse_octave(s, i)
  if octave == nil then
    return nil
  end
  return (octave + 1) * 12 + step + accidental
end

local function midi_of(x)
  if type(x) == "number" then
    return math.floor(x)
  end
  if type(x) == "string" then
    local m = parse_note_name(x)
    if m ~= nil then
      return m
    end
  end
  error("note: bad note '" .. tostring(x) .. "'")
end

-- note(n [, base]) -> 14-bit pitch for a sample recorded at `base`
-- (default "C4" / MIDI 60). n/base are a note name or a MIDI integer.
function note(n, base)
  local pitch = math.floor(4096 * (2 ^ ((midi_of(n) - midi_of(base or "C4")) / 12)) + 0.5)
  return math.max(0, math.min(0x3fff, pitch))
end

-- instrument{ sample=, adsr=?, gain=?, vol=127?, pan=0?, base="C4"?,
-- pitch=?, noise=?, pmod=?, echo=? } -> the same table, defaults filled in.
-- `pitch` is what sfx() plays when it is given no note (default 0x1000):
-- a drum recorded at 16 kHz sets pitch = 0x0800.
function instrument(t)
  if t == nil or t.sample == nil then
    error("instrument: sample is required")
  end
  t.vol = t.vol or 127
  t.pan = t.pan or 0
  t.base = t.base or "C4"
  return t
end

-- sfx(inst, v [, n]) -> apply preset `inst` to voice[v] and key it on.
function sfx(inst, v, n)
  local vo = voice[v]
  vo.sample = inst.sample
  if inst.adsr then
    vo.adsr = { a = inst.adsr.a, d = inst.adsr.d, s = inst.adsr.s, r = inst.adsr.r }
  end
  -- Engine treats `gain ~= nil` as GAIN mode, so an adsr preset with no gain
  -- of its own must still clear a stale GAIN-mode value left by an earlier
  -- preset.
  if inst.adsr or inst.gain ~= nil then
    vo.gain = inst.gain
  end
  if inst.noise ~= nil then
    vo.noise = inst.noise
  end
  if inst.pmod ~= nil then
    vo.pmod = inst.pmod
  end
  if inst.echo ~= nil then
    vo.echo = inst.echo
  end
  local pan = math.max(-1, math.min(1, inst.pan or 0))
  local vol = inst.vol or 127
  vo.vol = {
    l = math.floor(vol * math.min(1, 1 - pan) + 0.5),
    r = math.floor(vol * math.min(1, 1 + pan) + 0.5),
  }
  vo.pitch = n and note(n, inst.base) or inst.pitch or 0x1000
  kon(v)
end

-- A song is meant to be heard: power-on master volume is silence, so a
-- score{} started while nothing has set `dsp.mvol` opens it fully. A
-- program that sets its own level (before or after) keeps it.
local function ensure_audible()
  local mv = dsp.mvol
  if mv == nil or ((mv.l or 0) == 0 and (mv.r or 0) == 0) then
    dsp.mvol = { l = 127, r = 127 }
  end
end

-- song{} was retired in favor of song sources played with score{}. Level 2
-- attributes the error to the user's chunk, like the old validation errors.
function song()
  error('song{} is gone: make a song source (the Studio sequencer, or a .mid) and play it with score{ song = "<name>" }', 2)
end

-- Built-in sample presets (see crates/ppu-core/src/bank.rs for the sounds).
-- Melodic loops are recorded at C4; drums at 16 kHz, hence pitch 0x0800.
BANK = {
  piano   = { adsr = { a = 15, d = 4, s = 5, r = 10 } },
  bass    = { adsr = { a = 15, d = 5, s = 6, r = 12 }, base = "C4" },
  lead    = { adsr = { a = 14, d = 7, s = 7, r = 14 } },
  strings = { adsr = { a = 9, d = 7, s = 7, r = 10 } },
  organ   = { adsr = { a = 15, d = 7, s = 7, r = 16 } },
  bell    = { adsr = { a = 15, d = 3, s = 3, r = 8 } },
  flute   = { adsr = { a = 12, d = 7, s = 7, r = 12 } },
  pluck   = { adsr = { a = 15, d = 4, s = 4, r = 12 } },
  kick    = { adsr = { a = 15, d = 7, s = 7, r = 0 }, pitch = 0x0800 },
  snare   = { adsr = { a = 15, d = 7, s = 7, r = 0 }, pitch = 0x0800 },
  hat     = { adsr = { a = 15, d = 7, s = 7, r = 0 }, pitch = 0x0800, vol = 110 },
  ohat    = { adsr = { a = 15, d = 7, s = 7, r = 0 }, pitch = 0x0800, vol = 100 },
  tom     = { adsr = { a = 15, d = 7, s = 7, r = 0 }, pitch = 0x0800 },
  clap    = { adsr = { a = 15, d = 7, s = 7, r = 0 }, pitch = 0x0800 },
  crash   = { adsr = { a = 15, d = 7, s = 7, r = 0 }, pitch = 0x0800, vol = 100 },
}

-- bank(name [, opts]) -> an instrument{} over the built-in sample `name`,
-- placed in sound RAM by dma() (which chains placements upward on its
-- own; opts.addr pins one). Any other opts field (vol, pan, adsr, ...)
-- overrides the preset. Setup-only, like dma().
function bank(name, opts)
  local preset = BANK[name]
  if preset == nil then
    error("bank: no built-in sample '" .. tostring(name) .. "'")
  end
  opts = opts or {}
  local placed = dma(name, opts.addr and { addr = opts.addr } or nil)
  local inst = { sample = placed.id, addr = placed.addr, next_addr = placed.next_addr }
  for _, k in ipairs({ "adsr", "gain", "vol", "pan", "base", "pitch", "noise", "pmod", "echo" }) do
    if opts[k] ~= nil then
      inst[k] = opts[k]
    elseif preset[k] ~= nil then
      inst[k] = preset[k]
    end
  end
  return instrument(inst)
end

-- score{ song = "<name>", loop = true?, at = 0? } -> plays a `song`-kind source
-- named `<name>` (PSNG bytes, decoded/compiled/validated in Rust — see
-- __song_load) on the shared 8-voice pool: the whole song plays natively on
-- the audio tick, with no Lua timer, and the returned handle reads live
-- through __song_get (see below). Re-adding that source (add_source, same
-- name) reloads it in place in Rust, keeping its musical position
-- (LuaEngine::reload_song). No source named `<name>` is a setup error.
--
-- A built-in sound goes through bank() (preset ADSR/pitch); any other name
-- is an uploaded sample with a flat ADSR (`place`, below), so a toy playing
-- several songs places each sound once. `note` pitches the row with
-- note(row.note, base); without one a drum keeps its preset pitch.
--
-- The song is anchored to the timeline: its position is (t - anchor) * 250
-- ticks, so seeking t seeks the song (silent before the anchor; a finished
-- loop = false song stays silent). `at` is the anchor in seconds (default
-- 0), a marker name looked up in the toy's `markers` table, or false: set
-- up but not playing until play().
--
-- Returns { tick, length, playing, loop, song, play(), stop() }, all but
-- `song` read live through __song_get; stop() keys the song's own voices
-- (its mask) off and silences,
-- play() restarts from the top, anchored at the start of the frame being
-- rendered (from frame(): that frame's t; from a timer hook: the next
-- frame's). Setup-only.
local FLAT = { a = 15, d = 0, s = 7, r = 0 }

-- Placed sounds by name, shared by every score{}: a toy that plays several
-- songs places each sample in sound RAM once. The kit re-runs on every
-- set_sources, so this starts empty with the toy's placements.
local score_insts = {}

-- Turn a row's sound name into a placed instrument, placing it in sound RAM
-- the first time any score{} names it — see score_insts above. `i` is
-- 1-based, used only in error messages.
local function place(name, i)
  if score_insts[name] == nil then
    if BANK[name] ~= nil then
      local ok, inst = pcall(bank, name)
      if not ok then
        error("score: row " .. i .. " sound '" .. name .. "': " .. tostring(inst))
      end
      score_insts[name] = inst
    else
      local ok, placed = pcall(dma, name)
      if not ok then
        error("score: row " .. i .. " sound '" .. name .. "': " .. tostring(placed))
      end
      score_insts[name] = instrument{ sample = placed.id, adsr = FLAT }
    end
  end
  return score_insts[name]
end

-- A song-source row's pitch: note(row.note, inst.base) if it has a note,
-- else the instrument's own pitch (or 0x1000).
local function row_pitch(row, inst, i)
  if row.note == nil then
    return inst.pitch or 0x1000
  end
  local ok, p = pcall(note, row.note, inst.base)
  if not ok then
    error("score: row " .. i .. " bad note '" .. tostring(row.note) .. "'")
  end
  return p
end

-- A reloaded song source's rows (as __song_load returns them), resolved
-- against the sounds already placed: its per-row insts and pitches, or nil
-- when a row names a sound nothing placed at setup (placing needs the
-- setup window, so the engine recompiles instead). Called by the engine's
-- add_source, outside setup.
function __song_insts(rows)
  local insts, pitches = {}, {}
  for i = 1, #rows do
    local inst = score_insts[rows[i].sound]
    if inst == nil then
      return nil
    end
    insts[i] = inst
    pitches[i] = row_pitch(rows[i], inst, i)
  end
  return insts, pitches
end

-- Every score{} handle by song id: a recompile that carries a play()-started
-- song over republishes its handle as __score (LuaEngine::carry_songs).
__scores = {}

function score(cfg)
  cfg = cfg or {}
  if cfg.song == nil then
    error("score: give song = \"<name>\" (a song source)")
  end
  local id, song_rows = __song_load(tostring(cfg.song))
  if id == nil then
    error("score: no song source named '" .. tostring(cfg.song) .. "'")
  end
  local insts, pitches = {}, {}
  for i = 1, #song_rows do
    local row = song_rows[i]
    local inst = place(row.sound, i)
    insts[i] = inst
    pitches[i] = row_pitch(row, inst, i)
  end
  local at = cfg.at
  if at == nil then
    at = 0
  elseif type(at) == "string" then
    local m = type(markers) == "table" and markers[at] or nil
    if type(m) ~= "number" then
      error("score: no marker named '" .. at .. "'")
    end
    at = m
  elseif at ~= false and type(at) ~= "number" then
    error("score: at must be seconds, a marker name, or false")
  end
  ensure_audible()
  __song_start(id, insts, pitches, cfg.loop ~= false, at)
  -- The engine reads __score after every frame (LuaEngine::score_view) for
  -- the studio's playhead: the most recently started score is the one shown.
  local h = setmetatable({ song = tostring(cfg.song), __song = id }, {
    __index = function(_, k) return __song_get(id, k) end,
  })
  h.play = function()
    __song_play(id)
    __score = h
  end
  h.stop = function() __song_stop(id) end
  __scores[id] = h
  if at ~= false then
    __score = h
  end
  return h
end
