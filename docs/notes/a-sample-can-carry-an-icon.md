---
id: a-sample-can-carry-an-icon
kind: recipe
title: A sample source can carry an icon, meta.icon in ppu.json, that the Studio's Staff view draws for it
codes: []
---
The Staff view draws each built-in sound as its own pixel-art icon and an uploaded sample as a tinted
note-block with its first letter, unless the sample has an icon of its own. Pick one in the Staff palette,
or write it as `meta.icon` on the sample's source in ppu.json: a catalogue id from the list below. It is
the author's, so it saves with the toy, survives the sample's rebuild from its WAV
([[a-wav-file-is-a-sample-source]]), and `ppu connect` carries it both ways. An id not in the catalogue
draws the note-block. The icon is only a picture in the Studio: the game and the cartridge never see it.

- Built-in sounds: piano bass lead strings organ bell flute pluck kick snare hat ohat tom clap crash
- Instruments: trumpet tuba trombone sax french-horn clarinet recorder music-box harmonica kazoo ukulele
  acoustic-guitar banjo electric-guitar violin cello harp marimba xylophone glockenspiel steel-drum
  cowbell triangle tambourine maracas bongos conga timpani gong accordion microphone
- Animals: cat dog bunny frog duck bear penguin fox owl pig fish bee
- Vehicles: car scooter bus train plane rocket boat tractor helicopter submarine ufo
- Faces: smile grin cool wink love-eyes surprised sleepy silly angry cry ghost robot
- Food & treats: apple cherry banana strawberry carrot donut ice-cream cupcake pizza sushi cookie candy
- Nature & weather: sun moon cloud rain lightning snowflake rainbow flower tulip tree cactus mushroom
- Space & magic: star shooting-star planet alien potion wand crown heart gem sparkle crystal-ball wizard-hat

```json
{"name": "zap", "kind": "sample", "file": "sfx/zap.wav", "meta": {"icon": "rocket"}}
```
