-- Sequencer sugar, run as the "kit" chunk before every user chunk (see
-- LuaEngine::set_sources). Defines note/instrument/sfx/bank/song/midi/score
-- as globals; a user chunk defining the same name wins (chunks share one
-- global env and run in order). Never shadow `voice`/`kon`/`koff`/`timer`
-- with locals here — they must resolve as globals at call time so a later
-- chunk can replace them too (and so a user's `song{}` sees a shadowed
-- `timer`/`kon`/`koff`).
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

local function split_tokens(s)
  local out, len, start = {}, string.len(s), nil
  for i = 1, len do
    local c = string.sub(s, i, i)
    if c == " " or c == "\t" or c == "\n" then
      if start then
        out[#out + 1] = string.sub(s, start, i - 1)
        start = nil
      end
    elseif not start then
      start = i
    end
  end
  if start then
    out[#out + 1] = string.sub(s, start, len)
  end
  return out
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

-- song{ tempo=, steps=16?, tracks={ {voice=,inst=,pattern=}, ... } } ->
-- registers ONE timer(0, div, hook) stepping every track's pattern on the
-- 16th-note grid, auto-playing. `div` is the closest achievable timer-0
-- divider (1..255, ticks of 4 samples) to the tempo's 16th rate; `k` (>=1)
-- is how many hook fires make one step when a single tick can't reach it.
-- Tokens (parsed once here): a note name, "." rest, "-" hold, "^" key off.
-- `timer` is setup-only, so `song{}` is too (engine's own error, unwrapped).
-- A song is meant to be heard: power-on master volume is silence, so a
-- song{}/midi{} started while nothing has set `dsp.mvol` opens it fully. A
-- program that sets its own level (before or after) keeps it.
local function ensure_audible()
  local mv = dsp.mvol
  if mv == nil or ((mv.l or 0) == 0 and (mv.r or 0) == 0) then
    dsp.mvol = { l = 127, r = 127 }
  end
end

function song(cfg)
  if cfg == nil or cfg.tempo == nil or cfg.tempo <= 0 then
    error("song: tempo must be > 0")
  end
  ensure_audible()
  if type(cfg.tracks) ~= "table" then
    error("song: tracks must be a table")
  end
  local steps = cfg.steps or 16
  local tracks = {}
  for i = 1, #cfg.tracks do
    local tr = cfg.tracks[i]
    if type(tr.voice) ~= "number" or tr.voice ~= math.floor(tr.voice) or tr.voice < 0 or tr.voice > 7 then
      error("song: track " .. i .. " voice must be 0..7")
    end
    if type(tr.inst) ~= "table" or tr.inst.sample == nil then
      error("song: track " .. i .. " needs voice and inst")
    end
    if type(tr.pattern) ~= "string" then
      error("song: track " .. i .. " pattern must have at least one token")
    end
    local tokens = split_tokens(tr.pattern)
    if #tokens == 0 then
      error("song: track " .. i .. " pattern must have at least one token")
    end
    for j = 1, #tokens do
      local tok = tokens[j]
      if tok ~= "." and tok ~= "-" and tok ~= "^" and parse_note_name(tok) == nil then
        error("song: bad token '" .. tok .. "' in track " .. i)
      end
    end
    tracks[#tracks + 1] = { voice = tr.voice, inst = tr.inst, tokens = tokens }
  end

  -- 8000 timer-0 ticks/sec, 4 sixteenths/beat -> step_ticks ticks/16th.
  local step_ticks = 8000 * 60 / (cfg.tempo * 4)
  local k = math.ceil(step_ticks / 255)
  local div = math.max(1, math.min(255, math.floor(step_ticks / k + 0.5)))

  local h = {
    step = 0, beat = 0, playing = true,
    div = div, rate = 8000 / (div * k),
  }
  local pos, tick = 0, 0

  timer(0, div, function(off)
    if h.playing and tick % k == 0 then
      h.step = pos % steps
      h.beat = pos
      for i = 1, #tracks do
        local tr = tracks[i]
        local tok = tr.tokens[(pos % #tr.tokens) + 1]
        if tok == "^" then
          koff(tr.voice)
        elseif tok ~= "." and tok ~= "-" then
          sfx(tr.inst, tr.voice, tok)
        end
      end
      pos = pos + 1
    end
    tick = tick + 1
  end)

  h.play = function() h.playing = true end
  h.stop = function()
    h.playing = false
    for i = 1, #tracks do
      koff(tracks[i].voice)
    end
  end
  return h
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

-- General MIDI -> bank names, used by midi{} when no tracks are given.
local GM_FAMILY = { "piano", "bell", "organ", "pluck", "bass", "strings", "strings", "lead",
  "flute", "flute", "lead", "strings", "bell", "pluck", "pluck", "lead" }
local GM_DRUM = {
  [35] = "kick", [36] = "kick", [37] = "hat", [38] = "snare", [39] = "clap", [40] = "snare",
  [41] = "tom", [42] = "hat", [43] = "tom", [44] = "hat", [45] = "tom", [46] = "ohat",
  [47] = "tom", [48] = "tom", [49] = "crash", [50] = "tom", [51] = "ohat", [52] = "crash",
  [53] = "ohat", [54] = "hat", [55] = "crash", [56] = "hat", [57] = "crash", [59] = "ohat",
}

-- One placement per distinct name across a midi{} call: a built-in goes
-- through bank() (its preset pitch/adsr), an uploaded sample name becomes a
-- plain instrument recorded at C4. Shared by auto_tracks and the
-- `inst = "name"` / `drums = true` / `inst = "gm"` shorthands midi{} accepts
-- (what the Studio's Audio panel writes).
local function inst_cache()
  local insts = {}
  return function(name)
    if insts[name] == nil then
      if BANK[name] ~= nil then
        insts[name] = bank(name)
      else
        insts[name] = instrument{ sample = dma(name).id }
      end
    end
    return insts[name]
  end
end

-- The GM drum kit for the keys the given data tracks actually hit.
local function drum_kit(data, indices, inst_of)
  local kit = {}
  for _, i in ipairs(indices) do
    local notes = data.tracks[i].notes or {}
    for j = 1, #notes do
      local key = notes[j][3]
      if kit[key] == nil then
        kit[key] = inst_of(GM_DRUM[key] or "hat")
      end
    end
  end
  return kit
end

local function gm_name(tr)
  return GM_FAMILY[math.floor((tr.prog or 0) / 8) + 1] or "piano"
end

-- Auto-map every data track to the built-in bank: channel 10 tracks get the
-- GM drum kit (only the keys they use) on 2 voices, the rest split the remaining voices evenly (at
-- least one each; tracks past the eighth stay silent). One bank() per
-- distinct sound, so two piano tracks share a placement.
local function auto_tracks(data)
  local inst_of = inst_cache()
  local drums, melodic = {}, {}
  for i = 1, #data.tracks do
    local tr = data.tracks[i]
    if tr.ch == 9 then
      drums[#drums + 1] = i
    else
      melodic[#melodic + 1] = i
    end
  end
  local voices_left = 8
  local out = {}
  if #drums > 0 then
    -- Only the drums the song actually hits get placed in sound RAM.
    local kit = drum_kit(data, drums, inst_of)
    local dv = { 6, 7 }
    voices_left = 6
    for _, i in ipairs(drums) do
      out[i] = { insts = kit, voices = dv }
    end
  end
  if #melodic > 0 then
    local per = math.max(1, math.floor(voices_left / #melodic))
    local v = 0
    for _, i in ipairs(melodic) do
      if v + per > voices_left then
        break
      end
      local vs = {}
      for k = 1, per do
        vs[k] = v + k - 1
      end
      v = v + per
      out[i] = { inst = inst_of(gm_name(data.tracks[i])), voices = vs }
    end
  end
  return out
end

-- midi{ data=, tracks={ [i]={ voices={...}, inst=?, insts=? } }, loop=true?,
-- speed=1? } -> plays a table a .mid upload generated (see web
-- assets/midi.ts): { length, tracks = { { name, ch, notes = { {t, dur, key,
-- vel}, ... } } } }, times in seconds. cfg.tracks is keyed by data track
-- index; unmapped tracks stay silent. A mapped track plays on its `voices`
-- round-robin (chords need several; a stolen voice just restarts), `inst`
-- pitches the sample by key, `insts[key]` picks a preset per key at that
-- preset's own pitch (drum kits). Velocity scales the preset volume. One
-- timer(0, 32, ...) = a 4 ms grid. Returns { t, playing, length, play(),
-- stop() }; stop() pauses, play() resumes, a finished non-looping song
-- rewinds. Setup-only, like song{}.
function midi(cfg)
  if cfg == nil or type(cfg.data) ~= "table" or type(cfg.data.tracks) ~= "table" then
    error("midi: data must be the table a .mid upload generated")
  end
  ensure_audible()
  if cfg.tracks == nil then
    cfg.tracks = auto_tracks(cfg.data)
  elseif type(cfg.tracks) ~= "table" then
    error("midi: tracks must be a table")
  end
  local speed = cfg.speed or 1
  if type(speed) ~= "number" or speed <= 0 then
    error("midi: speed must be > 0")
  end
  -- Shorthands: `inst = "piano"` (built-in or uploaded sample name),
  -- `inst = "gm"` (this track's General MIDI program, drums on ch10), and
  -- `drums = true` (the GM kit for the keys this track uses).
  local inst_of = inst_cache()
  for i = 1, #cfg.data.tracks do
    local tr = cfg.tracks[i]
    if type(tr) == "table" then
      if tr.inst == "gm" then
        tr.inst = nil
        if cfg.data.tracks[i].ch == 9 then
          tr.drums = true
        else
          tr.inst = inst_of(gm_name(cfg.data.tracks[i]))
        end
      elseif type(tr.inst) == "string" then
        tr.inst = inst_of(tr.inst)
      end
      if tr.drums then
        tr.insts = drum_kit(cfg.data, { i }, inst_of)
      end
    end
  end
  local length = cfg.data.length or 0
  local tracks = {}
  for i = 1, #cfg.data.tracks do
    local tr = cfg.tracks[i]
    if tr ~= nil then
      if type(tr.voices) ~= "table" or #tr.voices == 0 then
        error("midi: track " .. i .. " needs voices = { ... }")
      end
      for j = 1, #tr.voices do
        local v = tr.voices[j]
        if type(v) ~= "number" or v ~= math.floor(v) or v < 0 or v > 7 then
          error("midi: track " .. i .. " voices must be 0..7")
        end
      end
      if (type(tr.inst) ~= "table" or tr.inst.sample == nil) and type(tr.insts) ~= "table" then
        error("midi: track " .. i .. " needs inst or insts")
      end
      local notes = cfg.data.tracks[i].notes or {}
      for j = 1, #notes do
        local e = notes[j][1] + notes[j][2]
        if e > length then
          length = e
        end
      end
      tracks[#tracks + 1] = { notes = notes, inst = tr.inst, insts = tr.insts, voices = tr.voices, rr = 0, next = 1, active = {} }
    end
  end

  local h = { t = 0, playing = true, length = length }
  local loop = cfg.loop ~= false
  local dt = 32 / 8000 * speed

  -- Forget active notes on voice v without koff (kon restarts it anyway).
  local function steal(tr, v)
    local n = 0
    for j = 1, #tr.active do
      local a = tr.active[j]
      if a.v ~= v then
        n = n + 1
        tr.active[n] = a
      end
    end
    for j = #tr.active, n + 1, -1 do
      tr.active[j] = nil
    end
  end
  local function all_off()
    for i = 1, #tracks do
      local tr = tracks[i]
      for j = 1, #tr.active do
        koff(tr.active[j].v)
      end
      tr.active = {}
    end
  end
  local function rewind()
    for i = 1, #tracks do
      tracks[i].next = 1
    end
  end

  timer(0, 32, function(off)
    if not h.playing then
      return
    end
    local t = h.t
    for i = 1, #tracks do
      local tr = tracks[i]
      local n = 0
      for j = 1, #tr.active do
        local a = tr.active[j]
        if a.e <= t then
          koff(a.v)
        else
          n = n + 1
          tr.active[n] = a
        end
      end
      for j = #tr.active, n + 1, -1 do
        tr.active[j] = nil
      end
      while tr.next <= #tr.notes and tr.notes[tr.next][1] <= t do
        local n = tr.notes[tr.next]
        tr.next = tr.next + 1
        local kit_inst = tr.insts and tr.insts[n[3]]
        local inst = kit_inst or tr.inst
        if inst ~= nil then
          local v = tr.voices[tr.rr + 1]
          tr.rr = (tr.rr + 1) % #tr.voices
          steal(tr, v)
          if kit_inst then
            sfx(inst, v)
          else
            sfx(inst, v, n[3])
          end
          local vo = voice[v]
          local g = n[4] / 127
          vo.vol = { l = math.floor(vo.vol.l * g + 0.5), r = math.floor(vo.vol.r * g + 0.5) }
          tr.active[#tr.active + 1] = { v = v, e = n[1] + n[2] }
        end
      end
    end
    h.t = t + dt
    if h.length > 0 and h.t >= h.length then
      all_off()
      rewind()
      if loop then
        h.t = h.t - h.length
      else
        h.t = 0
        h.playing = false
      end
    end
  end)

  -- stop() pauses (voices released, position kept); play() resumes.
  h.play = function()
    h.playing = true
  end
  h.stop = function()
    h.playing = false
    all_off()
  end
  return h
end

-- score{ song = "<id>" | data = seq_<id>(), loop = true? } -> plays a
-- sequencer song on the shared 8-voice pool. `song` names the global
-- seq_<id> function and binds the score to it (h.song = id): when the
-- engine re-runs an edited song file, __score_prepare(id) recompiles the
-- score in place, keeping its step. `data` is a one-off table, never
-- reloaded. The song is { tempo, swing = 0 (0..75), rows = {
-- { sound, note = ? } }, patterns = { A = { "4...", "..3-" } }, arrangement
-- = { "A", "B" } }: one step string per row, its length (8/16/32) the step
-- count, each step a 16th. `1`-`4` hit at volume 32/64/96/127, `-` holds
-- the previous hit, `.` rests. Swing delays odd steps by that percentage of
-- a step; every step time is rounded from its absolute index, so a
-- fractional step never drifts.
--
-- Setup compiles the whole arrangement into h.events, ordered by (start,
-- row): { start, ["end"], voice, row, pitch, l, r } in 4 ms ticks
-- (timer(0, 32)). Voices are chosen per note here, not at run time: the
-- lowest voice whose note has ended (end <= start), else the one whose
-- note started earliest (lowest voice on a tie) is stolen and that note's
-- end cut to the stealer's start. The web panel mirrors this allocator;
-- tests/fixtures/score_alloc.json holds both to the same answers.
--
-- A built-in sound goes through bank() (preset ADSR/pitch); any other name
-- is an uploaded sample with a flat ADSR. `note` pitches the row with
-- note(row.note, base); without one a drum keeps its preset pitch.
-- Returns { tick, length, playing, events, song, play(), stop() }; stop() keys
-- every voice off and pauses, play() resumes, a finished loop = false
-- song rewinds. Setup-only, like song{}.
local VELOCITY = { ["1"] = 32, ["2"] = 64, ["3"] = 96, ["4"] = 127 }
local STEP_COUNTS = { [8] = true, [16] = true, [32] = true }
local FLAT = { a = 15, d = 0, s = 7, r = 0 }

-- The song's data: `data` as given, or `seq_<id>()` for `song = "<id>"`.
local function song_data(cfg)
  if cfg.song ~= nil then
    if cfg.data ~= nil then
      error("score: give data or song, not both")
    end
    local f = _ENV["seq_" .. tostring(cfg.song)]
    if type(f) ~= "function" then
      error("score: no song function seq_" .. tostring(cfg.song) .. "()")
    end
    return f()
  end
  return cfg.data
end

-- Validates `d` and compiles it into (rows, events, length, at), `at(i)`
-- being the tick step `i` (counted across the arrangement) starts at. `place(name)`
-- turns a row's sound name into an instrument; returning nil (a reload
-- that names a sound not placed at setup) makes compile return nil.
local function compile(d, place)
  if type(d) ~= "table" then
    error("score: data must be a song table, e.g. seq_beat1()")
  end
  -- Capped at 400: a step is then >= 9.375 ticks, so even at swing 75 two
  -- steps never round to the same tick, keeping compile order (slot, step,
  -- row) identical to (start, row).
  if type(d.tempo) ~= "number" or d.tempo < 1 or d.tempo > 400 then
    error("score: tempo must be 1..400")
  end
  local swing = d.swing or 0
  if type(swing) ~= "number" or swing < 0 or swing > 75 then
    error("score: swing must be 0..75")
  end
  if type(d.rows) ~= "table" or #d.rows == 0 then
    error("score: rows must list at least one row")
  end
  if type(d.patterns) ~= "table" then
    error("score: patterns must be a table")
  end
  if type(d.arrangement) ~= "table" or #d.arrangement == 0 then
    error("score: arrangement must name at least one pattern")
  end

  local rows = {}
  for i = 1, #d.rows do
    local row = d.rows[i]
    local name = type(row) == "table" and row.sound
    if type(name) ~= "string" then
      error("score: row " .. i .. " needs sound = \"name\"")
    end
    local inst = place(name, i)
    if inst == nil then
      return nil
    end
    local pitch = inst.pitch or 0x1000
    if row.note ~= nil then
      local ok, p = pcall(note, row.note, inst.base)
      if not ok then
        error("score: row " .. i .. " bad note '" .. tostring(row.note) .. "'")
      end
      pitch = p
    end
    local pan = math.max(-1, math.min(1, inst.pan or 0))
    local vol = inst.vol or 127
    rows[i] = { inst = inst, pitch = pitch, l = vol * math.min(1, 1 - pan), r = vol * math.min(1, 1 + pan) }
  end

  local steps = {}
  for pname, pat in pairs(d.patterns) do
    local where = "score: pattern " .. tostring(pname)
    if type(pat) ~= "table" or #pat ~= #rows then
      error(where .. " needs one step string per row (" .. #rows .. ")")
    end
    for r = 1, #rows do
      local s = pat[r]
      if type(s) ~= "string" then
        error(where .. " row " .. r .. " must be a step string")
      end
      local n = string.len(s)
      if not STEP_COUNTS[n] then
        error(where .. " row " .. r .. " has " .. n .. " steps; use 8, 16 or 32")
      end
      if steps[pname] and n ~= steps[pname] then
        error(where .. " row " .. r .. " has " .. n .. " steps, row 1 has " .. steps[pname])
      end
      steps[pname] = n
      local prev = "."
      for k = 1, n do
        local c = string.sub(s, k, k)
        if c == "-" and prev == "." then
          error(where .. " row " .. r .. " step " .. k .. ": '-' holds nothing")
        elseif c ~= "-" and c ~= "." and VELOCITY[c] == nil then
          error(where .. " row " .. r .. " step " .. k .. ": bad '" .. c .. "' (use 1-4, - or .)")
        end
        prev = c
      end
    end
  end
  for i = 1, #d.arrangement do
    if steps[d.arrangement[i]] == nil then
      error("score: arrangement slot " .. i .. " names unknown pattern '" .. tostring(d.arrangement[i]) .. "'")
    end
  end

  -- 250 ticks/s, 4 sixteenths/beat.
  local step_ticks = 3750 / d.tempo
  local function at(i)
    if i % 2 == 1 then
      i = i + swing / 100
    end
    return math.floor(i * step_ticks + 0.5)
  end

  local events, busy, base = {}, {}, 0
  for slot = 1, #d.arrangement do
    local pat = d.patterns[d.arrangement[slot]]
    local n = steps[d.arrangement[slot]]
    for k = 1, n do
      for r = 1, #rows do
        local vel = VELOCITY[string.sub(pat[r], k, k)]
        if vel then
          local e = k + 1
          while e <= n and string.sub(pat[r], e, e) == "-" do
            e = e + 1
          end
          local row = rows[r]
          local ev = {
            start = at(base + k - 1), ["end"] = at(base + e - 1), row = r, pitch = row.pitch,
            l = math.floor(row.l * vel / 127 + 0.5), r = math.floor(row.r * vel / 127 + 0.5),
          }
          local v
          for u = 0, 7 do
            if busy[u] == nil or busy[u]["end"] <= ev.start then
              v = u
              break
            end
          end
          if v == nil then
            v = 0
            for u = 1, 7 do
              if busy[u].start < busy[v].start then
                v = u
              end
            end
            busy[v]["end"] = ev.start
          end
          ev.voice = v
          busy[v] = ev
          events[#events + 1] = ev
        end
      end
    end
    base = base + n
  end
  return rows, events, at(base), at
end

-- Live `song = "<id>"` scores, by id: each entry is that handle's reloader.
__score_songs = {}
-- True once any `data = ` score is set up: its table may have come from any
-- seq_ function, so an edit to a song no score names can't reload in place.
__score_data = false

-- Called by the engine after it re-runs a changed song file in the live VM.
-- Compiles every score bound to `id` from the new seq_<id>() and returns a
-- function that swaps them all in, keeping each one's step; nothing changes
-- until the engine calls it, after every changed song has prepared. A song
-- no score plays by name commits as a no-op: the engine's re-run already
-- replaced seq_<id>. Returns false, touching nothing, when a `data =` score
-- is live (its table may be this song's; the recompile picks the edit up)
-- or a row names a sound its score didn't place at setup (placement needs
-- the setup window). A data error raises: the old events keep playing.
function __score_prepare(id)
  local list = __score_songs[id]
  if list == nil then
    if __score_data then
      return false
    end
    return function() end
  end
  local d = song_data({ song = id })
  local commits = {}
  for i = 1, #list do
    local commit = list[i](d)
    if commit == nil then
      return false
    end
    commits[i] = commit
  end
  return function()
    for i = 1, #commits do
      commits[i]()
    end
  end
end

function score(cfg)
  cfg = cfg or {}
  local insts = {}
  local rows, events, length, at = compile(song_data(cfg), function(name, i)
    if insts[name] == nil then
      if BANK[name] ~= nil then
        local ok, inst = pcall(bank, name)
        if not ok then
          error("score: row " .. i .. " sound '" .. name .. "': " .. tostring(inst))
        end
        insts[name] = inst
      else
        local ok, placed = pcall(dma, name)
        if not ok then
          error("score: row " .. i .. " sound '" .. name .. "': " .. tostring(placed))
        end
        insts[name] = instrument{ sample = placed.id, adsr = FLAT }
      end
    end
    return insts[name]
  end)

  if cfg.song == nil then
    __score_data = true
  end
  ensure_audible()
  local loop = cfg.loop ~= false
  local h = { tick = 0, length = length, playing = true, events = events }
  -- The engine reads __score after every frame (LuaEngine::score_view) for
  -- the studio's playhead: the most recently started score is the one shown.
  __score = h
  local cursor, ends = 1, {}
  local function all_off()
    for v = 0, 7 do
      koff(v)
      ends[v] = nil
    end
  end

  if cfg.song ~= nil then
    local id = tostring(cfg.song)
    h.song = id
    local list = __score_songs[id] or {}
    __score_songs[id] = list
    list[#list + 1] = function(d)
      local new_rows, new_events, new_length, new_at = compile(d, function(name)
        return insts[name]
      end)
      if new_rows == nil then
        return nil
      end
      return function()
        if h.playing then
          all_off()
        end
        -- Keep the musical position, not the tick: the step the next tick
        -- falls in and how far into it, placed on the new step times (the
        -- offset kept inside that step). Unchanged step times map a tick to
        -- itself.
        local i = 0
        while at(i + 1) <= h.tick do
          i = i + 1
        end
        local offset = math.min(h.tick - at(i), new_at(i + 1) - new_at(i) - 1)
        h.tick = new_at(i) + offset
        rows, events, at, h.events, h.length = new_rows, new_events, new_at, new_events, new_length
        -- A song now shorter than its position ends here, as the timer's end
        -- branch would: it wraps, or stops (loop = false), rewound.
        if h.tick >= h.length then
          h.tick = 0
          h.playing = h.playing and loop
        end
        -- Re-seek: the next event due at or after the next tick to play.
        cursor = 1
        while events[cursor] and events[cursor].start < h.tick do
          cursor = cursor + 1
        end
      end
    end
  end

  timer(0, 32, function()
    if not h.playing then
      return
    end
    local t = h.tick
    if t >= h.length then
      all_off()
      cursor, t, h.tick = 1, 0, 0
      if not loop then
        h.playing = false
        return
      end
    end
    for v = 0, 7 do
      if ends[v] and ends[v] <= t then
        koff(v)
        ends[v] = nil
      end
    end
    while events[cursor] and events[cursor].start <= t do
      local ev = events[cursor]
      sfx(rows[ev.row].inst, ev.voice)
      voice[ev.voice].pitch = ev.pitch
      voice[ev.voice].vol = { l = ev.l, r = ev.r }
      ends[ev.voice] = ev["end"]
      cursor = cursor + 1
    end
    h.tick = t + 1
  end)

  h.play = function()
    h.playing = true
    __score = h
  end
  h.stop = function()
    h.playing = false
    all_off()
  end
  return h
end
