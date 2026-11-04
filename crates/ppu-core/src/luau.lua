-- Luau standard-library pieces and the Luau authoring surface, run before user chunks
-- (see LuaEngine::set_sources). A user chunk defining the same name wins.

function math.clamp(x, lo, hi)
  assert(lo <= hi, "math.clamp: max must not be less than min")
  if x < lo then return lo end
  if x > hi then return hi end
  return x
end

-- Luau rounds halves away from zero.
function math.round(x)
  if x >= 0 then return math.floor(x + 0.5) end
  return -math.floor(-x + 0.5)
end

function table.insert(t, position, value)
  if value == nil then
    t[#t + 1] = position
    return
  end
  for i = #t, position, -1 do t[i + 1] = t[i] end
  t[position] = value
end

function table.freeze(t) return t end

-- Luau's coroutine.wrap, which the VM lacks: a generator loops as `for x in g() do`.
function coroutine.wrap(f)
  local thread = coroutine.create(f)
  return function(...)
    local result = table.pack(coroutine.resume(thread, ...))
    if not result[1] then error(result[2], 0) end
    return table.unpack(result, 2, result.n)
  end
end

-- Region and update rate are project build settings (ppu.json `targetHz`), not source.
function target()
  error("region and update rate are build settings: set them in the Studio's Cartridge settings or ppu.json (\"targetHz\": 60, 30, 50 or 25)", 2)
end

-- Grid axes in callback parameter order; `names` is supplied by the Luau frontend.
local function __ppu_axes(api, grid, names, skip)
  assert(type(names) == "table", api .. " needs its function written inline: function(" .. "...) end")
  local axes, seen = {}, 0
  for i = skip + 1, #names do
    local range = grid[names[i]]
    assert(range ~= nil, api .. " grid has no range for parameter " .. names[i])
    local lo, hi, step = range[1], range[2], range[3] or 1
    assert(lo <= hi and step > 0 and (hi - lo) % step == 0,
      api .. " grid " .. names[i] .. " needs min <= max and a positive step that lands on max")
    axes[#axes + 1] = {name = names[i], lo = lo, hi = hi, step = step}
  end
  for _ in pairs(grid) do seen = seen + 1 end
  assert(seen == #axes, api .. " grid names an axis that is not a callback parameter")
  return axes
end

-- Nearest grid sample; an exact tie selects the higher sample. Outside the domain is an error.
local function __ppu_sample(api, axis, value)
  if value < axis.lo or value > axis.hi then
    error(api .. " input " .. axis.name .. "=" .. tostring(value) .. " is outside "
      .. tostring(axis.lo) .. ".." .. tostring(axis.hi), 3)
  end
  return axis.lo + math.floor((value - axis.lo) / axis.step + 0.5) * axis.step
end

-- Results keep 1/1024 precision, as the cartridge stores fractional lookup tables.
local function __ppu_q10(...)
  local out = table.pack(...)
  for i = 1, out.n do
    if type(out[i]) == "number" then out[i] = math.floor(out[i] * 1024 + 0.5) / 1024 end
  end
  return table.unpack(out, 1, out.n)
end

-- While a lookup samples its function, helpers it calls keep f64 too: __ppu_mul, __ppu_div
-- and fractional literals (frontend::fixed_point) read this.
-- shortcut: an error inside a lookup leaves this set; it ends the program, so nothing reads it after.
__ppu_sampling = false

function lookup(grid, evaluate, names)
  local axes = __ppu_axes("lookup", grid, names, 0)
  local count = #axes
  return function(a, b, c, d, e)
    assert(count <= 5, "lookup supports at most five inputs")
    local args = {a, b, c, d, e}
    for i = 1, count do args[i] = __ppu_sample("lookup", axes[i], args[i]) end
    local outer = __ppu_sampling
    __ppu_sampling = true
    local results = table.pack(evaluate(table.unpack(args, 1, count)))
    __ppu_sampling = outer
    return __ppu_q10(table.unpack(results, 1, results.n))
  end
end

function bakeHdma(grid, first, last, line, names)
  local axes = __ppu_axes("bakeHdma", grid, names, 1)
  local count = #axes
  return function(a, b, c, d, e)
    assert(count <= 5, "bakeHdma supports at most five inputs")
    local args = {a, b, c, d, e}
    for i = 1, count do args[i] = __ppu_sample("bakeHdma", axes[i], args[i]) end
    hdma(first, last, function(y) line(y, table.unpack(args, 1, count)) end)
  end
end

-- A routine: one cooperative invocation, resumed once per owner step.
local __ppu_running = nil

function yield(updates)
  local n = updates or 1
  assert(__ppu_running ~= nil, "yield is only allowed inside a routine")
  assert(n >= 1 and n == math.floor(n), "yield needs a positive whole number of updates")
  coroutine.yield(n)
end

function routine(body)
  local handle = {}
  local thread, wait = nil, 0
  function handle:start(...)
    if thread ~= nil then return false end
    local args = table.pack(...)
    thread = coroutine.create(function() body(table.unpack(args, 1, args.n)) end)
    wait = 0
    return true
  end
  function handle:step()
    if thread == nil then return end
    if wait > 0 then
      wait = wait - 1
      return
    end
    local outer = __ppu_running
    __ppu_running = handle
    local ok, result = coroutine.resume(thread)
    __ppu_running = outer
    if not ok then
      thread = nil
      error(result, 0)
    end
    if coroutine.status(thread) == "dead" then
      thread = nil
    else
      wait = result - 1
    end
  end
  function handle:cancel()
    thread, wait = nil, 0
  end
  function handle:running()
    return thread ~= nil
  end
  return handle
end

-- Modules: LuaEngine::set_sources registers each required file's chunk here.
__ppu_modules = {}
local __ppu_loaded, __ppu_loading = {}, {}

-- An engine library, require("@ppu/name"), is registered as the file @ppu/name.luau.
function require(path)
  local library = type(path) == "string" and string.sub(path, 1, 5) == "@ppu/"
  assert(library or type(path) == "string" and string.sub(path, 1, 2) == "./",
    "require needs a relative path such as \"./state\" or an engine library such as \"@ppu/blink\"")
  local name = library and path or string.sub(path, 3)
  local file = name .. ".luau"
  if __ppu_modules[file] == nil and not library then file = name .. ".lua" end
  local chunk = __ppu_modules[file]
  assert(chunk ~= nil or not library, "require: library " .. path
    .. " is not loaded; require an engine library by its literal name, require(\"" .. path .. "\")")
  assert(chunk ~= nil, "require: no file named " .. name .. ".luau or " .. name .. ".lua")
  if __ppu_loaded[file] ~= nil then return __ppu_loaded[file] end
  assert(not __ppu_loading[file], "require cycle through " .. file)
  __ppu_loading[file] = true
  local scene = __ppu_scene_file
  if __ppu_scene_files[file] then __ppu_scene_file = file end
  local value = chunk()
  __ppu_scene_file = scene
  __ppu_loading[file] = nil
  if value == nil then value = true end
  __ppu_loaded[file] = value
  return value
end

-- Scenes (docs/scenes.md). LuaEngine::set_sources lists the scene modules (frontend::scene_files)
-- in __ppu_scene_files before any file runs; while one loads, __ppu_scene_file names it, so what
-- its load records (placements, timers, songs) is the scene's. The engine reads __ppu_scenes after
-- the files run and __ppu_scene_next whenever an update starts.
__ppu_scene_files = {}
__ppu_scene_file = nil
__ppu_scenes = {}
__ppu_scenes_open = true
__ppu_scene_next = nil

scenes = {}

function scenes.add(scene)
  assert(__ppu_scenes_open, "scenes.add runs at file level, while the program loads")
  local file = nil
  for name in pairs(__ppu_scene_files) do
    if __ppu_loaded[name] == scene then file = name end
  end
  assert(file ~= nil, "scenes.add needs its module written inline: scenes.add(require(\"./title\"))")
  assert(type(scene) == "table" and type(scene.frame) == "function"
    and (scene.init == nil or type(scene.init) == "function"),
    "scenes.add: " .. file .. " must return {init = init, frame = frame}")
  for _, s in ipairs(__ppu_scenes) do
    assert(s.file ~= file, "scenes.add: " .. file .. " is already a scene")
  end
  __ppu_scenes[#__ppu_scenes + 1] = {file = file, init = scene.init, frame = scene.frame}
  return #__ppu_scenes
end

function scenes.go(scene)
  assert(type(scene) == "number" and scene == math.floor(scene) and scene >= 1 and scene <= #__ppu_scenes,
    "scenes.go needs a scene from scenes.add")
  __ppu_scene_next = scene
end

-- Record fields declared with makeFixed/makeBounded (the Luau frontend writes these calls).
-- Each store is checked against the field's contract; fixed fields keep their binary grid.
local function __ppu_round(x)
  if x >= 0 then return math.floor(x + 0.5) end
  return -math.floor(-x + 0.5)
end

-- Fixed-point arithmetic (frontend::fixed_point): Q10 operands, products and quotients
-- truncated toward zero, integer products staying integers; the canonical precision law.
-- Code a lookup is sampling keeps f64, like the lookup's own function.
local function __ppu_raw(x)
  if math.type(x) == "integer" then return x * 1024 end
  return __ppu_round(x * 1024)
end

local function __ppu_trunc(x)
  if x >= 0 then return math.floor(x) end
  return -math.floor(-x)
end

function __ppu_mul(a, b)
  if __ppu_sampling or math.type(a) == "integer" and math.type(b) == "integer" then return a * b end
  return __ppu_trunc(__ppu_raw(a) * __ppu_raw(b) / 1024) / 1024
end

function __ppu_div(a, b)
  if __ppu_sampling then return a / b end
  local divisor = __ppu_raw(b)
  if divisor == 0 then error("division by zero", 2) end
  return __ppu_trunc(__ppu_raw(a) * 1024 / divisor) / 1024
end

local function __ppu_contract(name, contract, value)
  local fixed, lo, hi, denominator = contract[1], contract[2], contract[3], contract[4]
  local kind = fixed and "makeFixed" or "makeBounded"
  if type(value) ~= "number" then
    error(kind .. " field " .. name .. " requires a number; got " .. tostring(value), 3)
  end
  if fixed then
    -- Q10 first (nearest, as literals), then down to the field's step.
    value = math.floor(__ppu_round(value * 1024) * denominator / 1024) / denominator
  elseif value ~= math.floor(value) then
    error(kind .. " field " .. name .. " requires an integer; got " .. tostring(value), 3)
  end
  if value < lo or value > hi then
    error(kind .. " field " .. name .. " requires a value in " .. tostring(lo) .. ".."
      .. tostring(hi) .. "; got " .. tostring(value), 3)
  end
  return value
end

function __ppu_initial(value, lo, hi, denominator)
  assert(type(lo) == "number" and type(hi) == "number" and lo <= hi and lo == math.floor(lo)
    and hi == math.floor(hi), "numeric constructor bounds must be ordered integers")
  assert(denominator == nil or (denominator >= 1 and denominator <= 1024
    and 1024 % denominator == 0), "makeFixed denominator must be a power of two from 1 through 1024")
  return value
end

function __ppu_record(values, contracts)
  for name, contract in pairs(contracts) do
    values[name] = __ppu_contract(name, contract, values[name])
  end
  return setmetatable({}, {
    __index = values,
    __newindex = function(_, name, value)
      local contract = contracts[name]
      if contract ~= nil then value = __ppu_contract(name, contract, value) end
      values[name] = value
    end,
    __ppu_values = values,
  })
end

-- Save data (ppu-core save.rs): a record kept in battery-backed RAM, as bytes in the cartridge's layout.

-- This VM has no tonumber or string.format: hex by table.
local __ppu_hex = "0123456789abcdef"
local __ppu_nibble = {}
for i = 1, 16 do
  __ppu_nibble[string.sub(__ppu_hex, i, i)] = i - 1
end

-- The host's save slot as SRAM words: image[i] is little-endian word i of `sram["ppu.save"]`, hex
-- whose missing tail is zeros, of `sram["ppu.save.size"]` bytes (save.rs `srm_from_slot`). Cart mode's
-- program keeps its SRAM here too. A write edits the hex in place: one copy, not a rebuild.
function __ppu_save_image(size)
  local slot = type(sram) == "table" and sram or {}
  if slot["ppu.save.size"] ~= size then slot["ppu.save.size"] = size end
  local bytes = {}
  local hex = slot["ppu.save"]
  if type(hex) ~= "string" then hex = "" end
  hex = string.sub(hex, 1, math.min(#hex // 2, size) * 2)
  for i = 1, #hex // 2 do
    local hi, lo = __ppu_nibble[string.sub(hex, i * 2 - 1, i * 2 - 1)], __ppu_nibble[string.sub(hex, i * 2, i * 2)]
    bytes[i - 1] = hi and lo and hi * 16 + lo or 0
  end
  local function digit(n) return string.sub(__ppu_hex, n + 1, n + 1) end
  return setmetatable({}, {
    __index = function(_, i)
      return (bytes[i * 2] or 0) | (bytes[i * 2 + 1] or 0) << 8
    end,
    __newindex = function(_, i, w)
      assert(i >= 0 and i < size // 2, "save RAM word out of range")
      local lo, hi = w & 255, (w >> 8) & 255
      if (bytes[i * 2] or 0) == lo and (bytes[i * 2 + 1] or 0) == hi then return end
      bytes[i * 2], bytes[i * 2 + 1] = lo, hi
      while #hex < i * 4 + 4 do hex = hex .. "00" end
      hex = string.sub(hex, 1, i * 4) .. digit(lo // 16) .. digit(lo % 16) .. digit(hi // 16) .. digit(hi % 16)
        .. string.sub(hex, i * 4 + 5, #hex)
      while #hex >= 2 and string.sub(hex, #hex - 1, #hex) == "00" do hex = string.sub(hex, 1, #hex - 2) end
      slot["ppu.save"] = hex
    end,
  })
end

-- The frontend passes `layout`: {size =, id =, {name, word, words, lo, hi, boolean}, ...}. Every step
-- below is the cartridge's, word for word, so both leave the same bytes.
function saveData(record, layout)
  assert(type(layout) == "table", "saveData needs its table written inline in a Luau file: saveData({hiscore = makeBounded(0, 0, 999999)})")
  local mt = getmetatable(record)
  local values = mt and mt.__ppu_values or record
  local image = __ppu_save_image(layout.size)
  local function read(f)
    local s = image[f[2]]
    if f[3] == 2 then s = s | image[f[2] + 1] << 16 end
    return s
  end
  local function stored(f, value)
    if f[6] then return value and 1 or 0 end
    return value - f[4]
  end
  local sum = 0
  for i = 3, layout[#layout][2] + layout[#layout][3] - 1 do sum = sum ~ image[i] end
  local ok = image[0] == 0x5350 and image[1] == layout.id and sum == image[2]
  for i = 1, #layout do ok = ok and read(layout[i]) <= layout[i][5] - layout[i][4] end
  if ok then
    for i = 1, #layout do
      local f = layout[i]
      if f[6] then record[f[1]] = read(f) ~= 0 else record[f[1]] = read(f) + f[4] end
    end
  else
    for i = 0, layout.size // 2 - 1 do image[i] = 0 end
    image[0], image[1] = 0x5350, layout.id
    local check = 0
    for i = 1, #layout do
      local f = layout[i]
      local s = stored(f, values[f[1]])
      for k = 0, f[3] - 1 do
        image[f[2] + k] = (s >> (16 * k)) & 0xffff
        check = check ~ ((s >> (16 * k)) & 0xffff)
      end
    end
    image[2] = check
  end
  local fields = {}
  for i = 1, #layout do fields[layout[i][1]] = layout[i] end
  return setmetatable({}, {
    __index = record,
    __newindex = function(_, name, value)
      local f = fields[name]
      if f == nil then error("saveData has no field " .. tostring(name) .. "; declare it in the saveData table", 2) end
      if f[6] then
        if type(value) ~= "boolean" then error("saveData field " .. name .. " requires true or false; got " .. tostring(value), 2) end
        values[name] = value
      else
        record[name] = value
        value = values[name]
      end
      -- Each word, after the checksum by old XOR new.
      local s = stored(f, value)
      local check = image[2]
      for k = 0, f[3] - 1 do check = check ~ image[f[2] + k] ~ ((s >> (16 * k)) & 0xffff) end
      image[2] = check
      for k = 0, f[3] - 1 do image[f[2] + k] = (s >> (16 * k)) & 0xffff end
    end,
  })
end
