// Ported from snes-apu 0.1.12 (BSD-2-Clause); see mod.rs.
//
// The eight-tap echo FIR filter (SNES DSP register 0xF, coefficients c0..c7).
// Two independent instances run per Dsp (left/right), each with its own
// sample-history ring buffer; coefficients are duplicated into both by
// `Dsp::set_filter_coefficient`, exactly as the reference keeps them.

const NUM_TAPS: usize = 8;

pub(crate) struct Filter {
    pub(crate) coefficients: [u8; NUM_TAPS],

    buffer: [i32; NUM_TAPS],
    buffer_pos: i32,
}

impl Filter {
    pub(crate) fn new() -> Filter {
        Filter {
            coefficients: [0; NUM_TAPS],

            buffer: [0; NUM_TAPS],
            buffer_pos: 0,
        }
    }

    pub(crate) fn next(&mut self, value: i32) -> i32 {
        self.buffer[self.buffer_pos as usize] = value;

        let mut ret = 0;
        for i in 0..NUM_TAPS {
            ret += (self.buffer[((self.buffer_pos + (i as i32)) as usize) % NUM_TAPS]
                * ((self.coefficients[i] as i8) as i32))
                >> 7;
        }

        self.buffer_pos = match self.buffer_pos {
            0 => (NUM_TAPS as i32) - 1,
            _ => self.buffer_pos - 1,
        };

        ret
    }
}
