-- Sample toy: scrolls bg1 by the frame counter.
function frame(t, f)
  brightness = 15
  screen.main.bg1 = true
  bg[1].hofs = f
end
