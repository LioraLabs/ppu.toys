"""apu_oracle.py CORE ROM: play the APU timing test ROM (tests/apu_timing.rs) in a libretro core and print,
per experiment (KON, KOFF, timer), each run's index of the first read that differs from its first read, and
with --dump every run's 64 reads. Black-box only: the audio RAM is found in a save state by the program's
done marker."""
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
for _ in range(600):
    core.retro_run()
    buf = C.create_string_buffer(core.retro_serialize_size())
    assert core.retro_serialize(buf, len(buf))
    at = buf.raw.find(b'ENVXDONE')  # ARAM $E000: the 384 runs' reads start $6000 before it
    if at >= 0x6000:
        break
assert at >= 0x6000, 'the APU timing program did not finish'
reads = buf.raw[at - 0x6000:at]
runs = [reads[i:i + 64] for i in range(0, len(reads), 64)]
first = [next((j for j, x in enumerate(r) if x != r[0]), 64) for r in runs]
for k, name in enumerate(['KON', 'KOFF', 'timer', 'KON+3', 'KOFF+3', 'timer+3']):
    print(f'{name}: {first[k * 64:k * 64 + 64]}')
if '--dump' in sys.argv:
    for i, r in enumerate(runs):
        print(i, list(r))
