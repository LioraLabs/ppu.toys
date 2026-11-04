-- Two scenes. The program boots into the first one added; the title enters the game on A.
scenes.add(require("./title"))
GAME = scenes.add(require("./game"))
