local function pair() return 10, 20 end
local function count(...) return select("#", ...) end
local function grouped_return() return (pair()) end
local function grouped_varargs(...) return (...) end
local t = { (pair()) }
assert(#t == 1 and t[1] == 10)
assert(count((pair())) == 1)
assert(count(grouped_return()) == 1)
assert(count(grouped_varargs(10, 20)) == 1)
assert(count(pair()) == 2)
assert(count(grouped_varargs()) == 1)
local a, b = (pair())
assert(a == 10 and b == nil)
local object = { pair = pair }
assert(count((object:pair())) == 1)
