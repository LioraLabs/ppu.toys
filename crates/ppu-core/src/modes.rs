//! Per-mode static capability table: bits-per-pixel per BG layer + fixed layer
//! order. The rasterizer/compositor read this instead of hard-coding modes, so
//! new modes become table rows, not rewrites. Ships tile modes 0-4 + Mode 7;
//! hi-res (5/6) stays out of scope for M4.

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
    /// Whether the mode uses BG3 as the offset-per-tile control layer.
    pub offset_per_tile: bool,
}

impl ModeInfo {
    /// Number of BG layers that exist in this mode.
    pub fn layer_count(&self) -> u8 {
        self.bpp.iter().filter(|&&b| b != 0).count() as u8
    }
}

/// Mode 0: four 2bpp BG layers.
const MODE_0: ModeInfo = ModeInfo {
    mode: 0,
    bpp: [2, 2, 2, 2],
    priority_order: &[0, 1, 2, 3],
    offset_per_tile: false,
};

/// Mode 1: BG1/BG2 4bpp, BG3 2bpp. (The BG3-priority BGMODE bit is m4/compositing.)
const MODE_1: ModeInfo = ModeInfo {
    mode: 1,
    bpp: [4, 4, 2, 0],
    priority_order: &[0, 1, 2],
    offset_per_tile: false,
};

/// Mode 2: BG1/BG2 4bpp with offset-per-tile controls.
const MODE_2: ModeInfo = ModeInfo {
    mode: 2,
    bpp: [4, 4, 0, 0],
    priority_order: &[0, 1],
    offset_per_tile: true,
};

/// Mode 3: BG1 8bpp, BG2 4bpp.
const MODE_3: ModeInfo = ModeInfo {
    mode: 3,
    bpp: [8, 4, 0, 0],
    priority_order: &[0, 1],
    offset_per_tile: false,
};

/// Mode 4: BG1/BG2 4bpp with offset-per-tile controls.
const MODE_4: ModeInfo = ModeInfo {
    mode: 4,
    bpp: [4, 4, 0, 0],
    priority_order: &[0, 1],
    offset_per_tile: true,
};

/// Mode 7: one 8bpp affine BG over the interleaved VRAM layout (m4/mode7).
const MODE_7: ModeInfo = ModeInfo {
    mode: 7,
    bpp: [8, 0, 0, 0],
    priority_order: &[0],
    offset_per_tile: false,
};

/// Look up a mode's static capabilities. Unsupported modes return `None`.
pub fn mode_info(mode: u8) -> Option<&'static ModeInfo> {
    match mode {
        0 => Some(&MODE_0),
        1 => Some(&MODE_1),
        2 => Some(&MODE_2),
        3 => Some(&MODE_3),
        4 => Some(&MODE_4),
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
    fn mode_0_is_four_2bpp_layers() {
        let m = mode_info(0).unwrap();
        assert_eq!(m.bpp, [2, 2, 2, 2]);
        assert_eq!(m.layer_count(), 4);
        assert_eq!(m.priority_order, &[0, 1, 2, 3]);
        assert!(!m.offset_per_tile);
    }

    #[test]
    fn modes_2_and_4_mark_offset_per_tile_without_drawing_bg3() {
        for mode in [2u8, 4] {
            let m = mode_info(mode).unwrap();
            assert_eq!(m.bpp, [4, 4, 0, 0]);
            assert_eq!(m.layer_count(), 2);
            assert_eq!(m.priority_order, &[0, 1]);
            assert!(m.offset_per_tile);
        }
    }

    #[test]
    fn mode_3_is_8bpp_bg1_plus_4bpp_bg2() {
        let m = mode_info(3).unwrap();
        assert_eq!(m.bpp, [8, 4, 0, 0]);
        assert_eq!(m.layer_count(), 2);
        assert_eq!(m.priority_order, &[0, 1]);
        assert!(!m.offset_per_tile);
    }

    #[test]
    fn still_unsupported_modes_are_none() {
        for mode in [5u8, 6, 9] {
            assert!(
                mode_info(mode).is_none(),
                "mode {mode} should remain unsupported"
            );
        }
    }
}
