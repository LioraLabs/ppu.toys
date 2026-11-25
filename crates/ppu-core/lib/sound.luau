--!strict
-- @ppu/sound: named sound effects, named songs (one plays at a time), and the master volume.
-- local sound = require("@ppu/sound")
-- local cues = sound.cues({shot = {bank("pluck"), "C6"}, boom = {bank("crash"), "C3", 2}})
-- cues:play("shot")
-- local music = sound.songs({title = score{song = "title", at = false, loop = true}})
-- music:play("title"); music:stop(); sound.volume(100)
-- Make cues and songs at file level, from literal tables. A cue is {preset, note, priority}: the sound
-- driver picks its voice, as sfx(preset, note) does. {preset, voice, note, priority} names the voice, so
-- cues on one voice cut each other. Set a looping sample's length on its bank() preset.
-- The cartridge compiles these calls itself (its own copy of this file): change both together.

-- Plays a cue by name. A cue with a priority plays only above the highest priority already
-- played this update: two ships firing in one update key one shot, not two.
local function play(cues, name: string)
  local c = cues.list[name]
  local auto = type(c[2]) == "string"
  local p = (if auto then c[3] else c[4]) or 0
  if cues.at ~= __ppu_update then
    cues.at, cues.played = __ppu_update, 0
  end
  if p ~= 0 and p <= cues.played then
    return
  end
  if auto then
    sfx(c[1], c[2])
  else
    sfx(c[1], c[2], c[3])
  end
  if p ~= 0 then
    cues.played = p
  end
end

local function cues(list)
  return {list = list, played = 0, at = __ppu_update, play = play}
end

-- Stops every song.
local function stop(songs)
  for _, song in songs.list do
    song.stop()
  end
end

-- Plays the named song. One score plays at a time: playing one stops whichever is playing, from
-- this set, another set or a bare score{}. Each scene starts its own song in init(): a scene
-- switch restarts the sound chip.
local function play_song(songs, name: string)
  songs.list[name].play()
end

local function songs(list)
  return {list = list, play = play_song, stop = stop}
end

-- The master volume, 0 (silent) to 127, both sides. The chip powers on at 0; score{} opens a
-- silent chip to 127 by itself, sound effects alone do not.
local function volume(v: number)
  dsp.mvol.l, dsp.mvol.r = v, v
end

return {cues = cues, songs = songs, volume = volume}
