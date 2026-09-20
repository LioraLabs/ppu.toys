-- Built-in VRAM helper, available before any sketch file runs.
-- vr(addr, words) writes consecutive VRAM words starting at `addr`: one call
-- per run instead of one `vram[addr] = word` line per word, so a tilemap row
-- reads as a row. Same meaning, same authority as the raw writes it replaces.
function vr(addr, words)
  for i = 1, #words do
    vram[addr + i - 1] = words[i]
  end
end
