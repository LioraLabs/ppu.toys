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

-- The frame rate is a cartridge requirement; the preview always runs at 60 Hz.
function target(options)
  assert(options.region == "ntsc" or options.region == "pal", "target region must be \"ntsc\" or \"pal\"")
  assert(options.hz == 60 or options.hz == 30 or options.hz == 50 or options.hz == 25, "target hz must be 60, 30, 50 or 25")
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

-- A spread routine: one cooperative invocation, resumed once per owner step.
local __ppu_running = nil

function yield(updates)
  local n = updates or 1
  assert(__ppu_running ~= nil, "yield is only allowed inside a spread routine")
  assert(n >= 1 and n == math.floor(n), "yield needs a positive whole number of updates")
  coroutine.yield(n)
end

function spread(body)
  local routine = {}
  local thread, wait = nil, 0
  function routine:start(...)
    if thread ~= nil then return false end
    local args = table.pack(...)
    thread = coroutine.create(function() body(table.unpack(args, 1, args.n)) end)
    wait = 0
    return true
  end
  function routine:step()
    if thread == nil then return end
    if wait > 0 then
      wait = wait - 1
      return
    end
    local outer = __ppu_running
    __ppu_running = routine
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
  function routine:cancel()
    thread, wait = nil, 0
  end
  function routine:running()
    return thread ~= nil
  end
  return routine
end

-- Modules: LuaEngine::set_sources registers each required file's chunk here.
__ppu_modules = {}
local __ppu_loaded, __ppu_loading = {}, {}

function require(path)
  assert(type(path) == "string" and string.sub(path, 1, 2) == "./",
    "require needs a relative path such as \"./state\"")
  local name = string.sub(path, 3)
  local file = name .. ".luau"
  if __ppu_modules[file] == nil then file = name .. ".lua" end
  local chunk = __ppu_modules[file]
  assert(chunk ~= nil, "require: no file named " .. name .. ".luau or " .. name .. ".lua")
  if __ppu_loaded[file] ~= nil then return __ppu_loaded[file] end
  assert(not __ppu_loading[file], "require cycle through " .. file)
  __ppu_loading[file] = true
  local value = chunk()
  __ppu_loading[file] = nil
  if value == nil then value = true end
  __ppu_loaded[file] = value
  return value
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
    value = __ppu_round(__ppu_round(value * 1024) * denominator / 1024) / denominator
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
  })
end
