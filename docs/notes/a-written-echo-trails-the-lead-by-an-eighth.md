---
id: a-written-echo-trails-the-lead-by-an-eighth
kind: recipe
title: A written echo is a second voice playing the lead an eighth later, quieter and panned the other way
codes: []
---
The chip's echo is the first choice ([[echo-is-the-snes-texture]]): it costs no voice and no song bytes and
plays on a cartridge. Write the echo instead when it must change per section, or when the delay you want is
longer than the chip's 240 ms.

Copy the lead's notes to a second row of the same sound, start each one an eighth note later (24 units,
at 48 to the beat), and give that row about half the lead's `vol` and the opposite pan. It sounds like
the chip's echo, costs no sound RAM, and can change per section: leave it out the first time a phrase
plays and bring it in on the repeat ([[one-hook-and-a-difference-every-repeat]]).

Delay the copy by whole eighths only. Offset by a sixteenth, it falls between the beats and the
music seems to stumble. It costs a voice for as long as the lead plays, so count it in the song's voices
([[song-voices-steal-the-oldest-note]]), and shorten the lead's long notes so the copy does not pile on
the next one.
