"""timing_oracle.py CORE ROM N: play the timing test ROM (tests/timing.rs) in a libretro core and
print its N experiments' results, one experiment per line: `i h0 v0 h1 v1 hn vn` (start latch, end latch,
interrupt-handler latch). Black-box only: the core is loaded, run and read through libretro (work RAM is found
in a save state by the ROM's done marker, since not every core exposes it)."""
import ctypes as C, sys
from pathlib import Path

core = C.CDLL(str(Path(sys.argv[1]).resolve()))
rom = Path(sys.argv[2]).read_bytes()
OPTIONS = {b'bsnes_ppu_fast': b'OFF', b'bsnes_dsp_fast': b'OFF', b'bsnes_cpu_fast_math': b'OFF'}


class Variable(C.Structure):
    _fields_ = [('key', C.c_char_p), ('value', C.c_char_p)]


@C.CFUNCTYPE(C.c_bool, C.c_uint, C.c_void_p)
def env(c, data):
    if c == 15:
        v = C.cast(data, C.POINTER(Variable))[0]
        if v.key in OPTIONS:
            C.cast(data, C.POINTER(Variable))[0].value = OPTIONS[v.key]
            return True
        return False
    if c in (9, 31):
        C.cast(data, C.POINTER(C.c_char_p))[0] = b'/tmp'
        return True
    return c == 10


cbs = {'environment': env, 'video_refresh': C.CFUNCTYPE(None, C.c_void_p, C.c_uint, C.c_uint, C.c_size_t)(lambda *a: None),
       'audio_sample': C.CFUNCTYPE(None, C.c_int16, C.c_int16)(lambda l, r: None),
       'audio_sample_batch': C.CFUNCTYPE(C.c_size_t, C.c_void_p, C.c_size_t)(lambda a, n: n),
       'input_poll': C.CFUNCTYPE(None)(lambda: None),
       'input_state': C.CFUNCTYPE(C.c_int16, C.c_uint, C.c_uint, C.c_uint, C.c_uint)(lambda *a: 0)}
for n, cb in cbs.items():
    f = getattr(core, 'retro_set_' + n)
    f.argtypes = [type(cb)]
    f(cb)
core.retro_init()


class G(C.Structure):
    _fields_ = [('path', C.c_char_p), ('data', C.c_void_p), ('size', C.c_size_t), ('meta', C.c_char_p)]


data = C.create_string_buffer(rom)
g = G(str(Path(sys.argv[2]).resolve()).encode(), C.cast(data, C.c_void_p), len(rom), None)
core.retro_load_game.argtypes = [C.POINTER(G)]
core.retro_load_game.restype = C.c_bool
assert core.retro_load_game(C.byref(g)), 'the core refused the ROM'
core.retro_serialize_size.restype = C.c_size_t
core.retro_serialize.argtypes = [C.c_void_p, C.c_size_t]
core.retro_serialize.restype = C.c_bool
for _ in range(200):
    core.retro_run()
    buf = C.create_string_buffer(core.retro_serialize_size())
    assert core.retro_serialize(buf, len(buf))
    at = buf.raw.find(b'TIMEDONE')  # $7E:17F8: the results start 0x7F8 before it
    if at >= 0x7f8:
        break
assert at >= 0x7f8, 'the timing ROM did not finish'
s = buf.raw[at - 0x7f8:at + 8]
w = lambda a: int.from_bytes(s[a:a + 2], 'little') & 0x1ff
for i in range(int(sys.argv[3])):
    print(i, *(w(i * 8 + k) for k in (0, 2, 4, 6)), w(0x400 + i * 8), w(0x400 + i * 8 + 2))
