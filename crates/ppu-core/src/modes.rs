//! Per-mode static capability table: bits-per-pixel per BG layer + fixed layer
//! order. The rasterizer/compositor read this instead of hard-coding modes, so
//! new modes (0/2/3) become table rows, not rewrites. Ships Mode 1 + Mode 7;
//! offset-per-tile (2/4/6) and hi-res (5/6) stay out of scope for M4.

/// Static capabilities of one BG mode.
#[derive(Debug, PartialEq, Eq)]
pub struct ModeInfo {
    /// The BGMODE value this row describes.
    pub mode: u8,
    /// Bits per pixel for BG1..BG4 (index 0..3); 0 = the layer does not exist
    /// in this mode.
    pub bpp: [u8; 4],
    /// Fixed front-to-back BG order at equal tile priority (0-based indices).
    /// Per-tile priority interleaving is the m4/compositing pass's concern.
    pub priority_order: &'static [u8],
}

impl ModeInfo {
    /// Number of BG layers that exist in this mode.
    pub fn layer_count(&self) -> u8 {
        self.bpp.iter().filter(|&&b| b != 0).count() as u8
    }
}

/// Mode 1: BG1/BG2 4bpp, BG3 2bpp. (The BG3-priority BGMODE bit is m4/compositing.)
const MODE_1: ModeInfo = ModeInfo {
    mode: 1,
    bpp: [4, 4, 2, 0],
    priority_order: &[0, 1, 2],
};

/// Mode 7: one 8bpp affine BG over the interleaved VRAM layout (m4/mode7).
const MODE_7: ModeInfo = ModeInfo {
    mode: 7,
    bpp: [8, 0, 0, 0],
    priority_order: &[0],
};

/// Look up a mode's static capabilities. Modes 0/2/3 are trivial future rows
/// (one `ModeInfo` each); unsupported modes return `None`.
pub fn mode_info(mode: u8) -> Option<&'static ModeInfo> {
    match mode {
        1 => Some(&MODE_1),
        7 => Some(&MODE_7),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_1_is_4_4_2() {
        let m = mode_info(1).unwrap();
        assert_eq!(m.bpp, [4, 4, 2, 0]);
        assert_eq!(m.layer_count(), 3);
        assert_eq!(m.priority_order, &[0, 1, 2]);
    }

    #[test]
    fn mode_7_is_single_8bpp_layer() {
        let m = mode_info(7).unwrap();
        assert_eq!(m.bpp, [8, 0, 0, 0]);
        assert_eq!(m.layer_count(), 1);
        assert_eq!(m.priority_order, &[0]);
    }

    #[test]
    fn unshipped_modes_are_none() {
        for mode in [0u8, 2, 3, 4, 5, 6] {
            assert!(mode_info(mode).is_none(), "mode {mode} should be unshipped");
        }
        assert!(mode_info(9).is_none()); // out of range entirely
    }
}
