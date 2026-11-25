//! Which of the S-DSP's 8 voices an automatic sound effect takes. Songs and effects share the
//! voices: an effect borrows the voice it is given until it ends (the song's notes there are
//! silent meanwhile and its next note after the effect plays normally). The preview and the
//! cartridge's sound driver both choose by [`choose`], held to it by [`CASES`].
//!
//! Lengths are song ticks (250 a second).

/// Who holds a voice for an effect right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Held {
    /// No effect.
    Free,
    /// An effect that named its voice: never taken by an automatic one.
    Explicit,
    /// An automatic effect started at `since` (smaller is older; the caller keeps it rising).
    Auto(u16),
}

/// One voice as the choice sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Voice {
    /// A playing song has notes on this voice.
    pub song: bool,
    /// Ticks until the song's next key-on here, while the song rests on the voice. `None` while
    /// a song note sounds, or when the next key-on is not known (past the current slot's end,
    /// or none left).
    pub rest: Option<u16>,
    pub held: Held,
}

impl Voice {
    /// No song notes and no effect.
    pub const FREE: Voice = Voice {
        song: false,
        rest: None,
        held: Held::Free,
    };
    /// A song voice sounding a note (or resting with its next key-on unknown).
    pub const SOUNDING: Voice = Voice {
        song: true,
        rest: None,
        held: Held::Free,
    };

    /// A song voice resting `ticks` before its next key-on.
    pub const fn resting(ticks: u16) -> Voice {
        Voice {
            song: true,
            rest: Some(ticks),
            held: Held::Free,
        }
    }

    /// The same voice, held by an effect.
    pub const fn held(self, held: Held) -> Voice {
        Voice { held, ..self }
    }
}

/// An effect's length in song ticks: `frames` video frames (60 a second) rounded up to whole
/// ticks (250 a second). `u16::MAX` frames (until koff()) stays `u16::MAX`. The preview's borrow
/// and the cartridge driver's count both convert here.
pub fn ticks(frames: u16) -> u32 {
    if frames == u16::MAX {
        u16::MAX as u32
    } else {
        (frames as u32 * 250).div_ceil(60)
    }
}

/// The voice an automatic effect `len` ticks long takes, or `None` when every voice is held by
/// an explicit effect or kept. `keep` is the mask of song voices never stolen. In order, highest
/// voice first within each step:
/// 1. a voice no song uses and no effect holds;
/// 2. a song voice resting, not kept, whose next key-on is later than `len`;
/// 3. a song voice not kept and not held (the highest: the song allocator fills the lowest
///    voices first, so the highest carry only its busiest moments);
/// 4. the oldest automatic effect's voice.
pub fn choose(voices: &[Voice; 8], keep: u8, len: u16) -> Option<u8> {
    let free = |v: &Voice| v.held == Held::Free;
    let open = |i: usize| keep & (1 << i) == 0;
    let highest = |pick: &dyn Fn(usize, &Voice) -> bool| {
        (0..8).rev().find(|&i| pick(i, &voices[i])).map(|i| i as u8)
    };
    highest(&|_, v| !v.song && free(v))
        .or_else(|| highest(&|i, v| free(v) && open(i) && v.rest.is_some_and(|r| r > len)))
        .or_else(|| highest(&|i, v| v.song && free(v) && open(i)))
        .or_else(|| {
            (0..8)
                .rev()
                .filter_map(|i| match voices[i].held {
                    Held::Auto(since) => Some((since, i as u8)),
                    _ => None,
                })
                .min_by_key(|&(since, _)| since)
                .map(|(_, i)| i)
        })
}

/// One case of the choice: inputs and the voice [`choose`] must return.
#[derive(Clone, Copy, Debug)]
pub struct Case {
    pub name: &'static str,
    pub voices: [Voice; 8],
    pub keep: u8,
    pub len: u16,
    pub want: Option<u8>,
}

const F: Voice = Voice::FREE;
const S: Voice = Voice::SOUNDING;
const fn r(ticks: u16) -> Voice {
    Voice::resting(ticks)
}
const fn auto(v: Voice, since: u16) -> Voice {
    v.held(Held::Auto(since))
}
const fn exp(v: Voice) -> Voice {
    v.held(Held::Explicit)
}

/// The choice's case table: every reader of [`choose`] (the cartridge driver included) runs it.
pub const CASES: &[Case] = &[
    Case {
        name: "silence: the highest voice",
        voices: [F; 8],
        keep: 0,
        len: 38,
        want: Some(7),
    },
    Case {
        name: "a song on 0-4 leaves 7 free",
        voices: [S, S, S, S, S, F, F, F],
        keep: 0,
        len: 38,
        want: Some(7),
    },
    Case {
        name: "a free voice beats a long rest",
        voices: [S, S, F, r(1000), r(1000), r(1000), r(1000), r(1000)],
        keep: 0,
        len: 38,
        want: Some(2),
    },
    Case {
        name: "an effect on a free voice: the next free one",
        voices: [S, S, S, F, F, F, F, exp(F)],
        keep: 0,
        len: 38,
        want: Some(6),
    },
    Case {
        name: "the highest rest longer than the effect",
        voices: [S, r(100), S, r(100), S, S, r(20), S],
        keep: 0,
        len: 38,
        want: Some(3),
    },
    Case {
        name: "a rest exactly the effect's length is too short",
        voices: [S, S, S, S, S, S, r(38), S],
        keep: 0,
        len: 38,
        want: Some(7),
    },
    Case {
        name: "a rest one tick longer fits",
        voices: [S, S, S, S, S, S, r(39), S],
        keep: 0,
        len: 38,
        want: Some(6),
    },
    Case {
        name: "all sounding: steal the highest",
        voices: [S; 8],
        keep: 0,
        len: 38,
        want: Some(7),
    },
    Case {
        name: "the highest is held: steal the next",
        voices: [S, S, S, S, S, S, S, auto(S, 3)],
        keep: 0,
        len: 38,
        want: Some(6),
    },
    Case {
        name: "kept voices are never stolen",
        voices: [S; 8],
        keep: 0b1110_0000,
        len: 38,
        want: Some(4),
    },
    Case {
        name: "a kept resting voice is not borrowed",
        voices: [S, S, S, S, S, S, S, r(1000)],
        keep: 0b1000_0000,
        len: 38,
        want: Some(6),
    },
    Case {
        name: "nothing left: the oldest automatic effect",
        voices: [
            auto(S, 9),
            auto(S, 4),
            exp(S),
            auto(S, 12),
            auto(S, 5),
            auto(S, 7),
            exp(S),
            auto(S, 6),
        ],
        keep: 0,
        len: 38,
        want: Some(1),
    },
    Case {
        name: "kept and all else held: the oldest automatic effect",
        voices: [S, S, S, S, auto(S, 2), auto(S, 1), exp(S), auto(S, 3)],
        keep: 0b0000_1111,
        len: 38,
        want: Some(5),
    },
    Case {
        name: "every voice explicit: none",
        voices: [exp(S); 8],
        keep: 0,
        len: 38,
        want: None,
    },
    Case {
        name: "a resting voice an effect holds is not free",
        voices: [S, S, S, S, S, S, S, exp(r(1000))],
        keep: 0,
        len: 38,
        want: Some(6),
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_case() {
        for c in CASES {
            assert_eq!(choose(&c.voices, c.keep, c.len), c.want, "{}", c.name);
        }
    }

    #[test]
    fn ticks_round_frames_up_and_keep_until_koff() {
        assert_eq!([0, 1, 6, 9, 60].map(ticks), [0, 5, 25, 38, 250]);
        assert_eq!(ticks(u16::MAX), u16::MAX as u32);
    }
}
