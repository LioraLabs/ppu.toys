---
id: validate-data-with-error
kind: rule
title: Check authored data when it loads with error() that names the culprit
codes: []
---
Data written by hand (dialog pages, a map's objects, a wave list) breaks in ways the code cannot fix at
run time: a line longer than the text box, an NPC with no lines, a page with one line too many. Check it
once, where it loads, and stop with `error()` whose message names the file and the entry. A silent fallback
would draw a cut-off line or a mute NPC that someone finds weeks later.

```luau
local WIDTH = 25
local LINES = {
  elder = {{"THE CAVE IS NORTH.", "TAKE THIS."}},
}

local function has(name: string): boolean
  local pages = LINES[name]
  if pages == nil then return false end
  if #pages > 8 then error("dialog: " .. name .. " has more than 8 pages") end
  for _, page in pages do
    if #page > 3 then error("dialog: a page of " .. name .. "'s has more than 3 lines") end
    for _, l in page do
      if #l > WIDTH then error("dialog: a line of " .. name .. "'s is longer than 25 characters") end
    end
  end
  return true
end

if not has("elder") then error("the elder has no lines in the dialog") end
```
