//! What the keep path would refuse, as structured findings.
//!
//! [`check_indexed`] runs the very code [`super::keep::convert_indexed_source`] imports with and
//! reports every refusal it finds instead of the first, so "no findings" is "the import accepts".

use crate::render::IndexedPng;
use crate::source::{ConvertOptions, PaletteSource, SourceKind};
use serde::Serialize;

/// The rule a [`Finding`] breaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Rule {
    /// A tile or cell uses colours from two palette rows.
    MixedRows,
    /// A tile or cell is on a palette row past the budget.
    PaletteBudget,
    /// More tiles than the char budget keeps.
    TileBudget,
    /// A bg wider or taller than the 512 px (64 tile) map.
    MapBudget,
    /// The PLTE is wrong: not the palette's, or breaks a hardware rule.
    Plte,
    /// A quantizing import (a PNG that is not SNES art) refuses it.
    Import,
    /// The `tRNS` does not mark the image as SNES art (or a pixel names no PLTE entry).
    NotSnesArt,
}

/// One refusal. `x`,`y` are unit coordinates (pixel = x * unit width); rules without a place omit them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub rule: Rule,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<usize>,
    /// The palette rows involved (mixed rows: the two; palette budget: the one past it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<Vec<usize>>,
    /// The first PLTE entry at fault.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
    /// The refusal text `convert_indexed_source` returns for the first finding.
    pub message: String,
}

impl Finding {
    pub fn new(rule: Rule, message: String) -> Self {
        Finding {
            rule,
            x: None,
            y: None,
            rows: None,
            index: None,
            message,
        }
    }
    pub fn rows(mut self, rows: Vec<usize>) -> Self {
        self.rows = Some(rows);
        self
    }
    pub fn at(mut self, x: usize, y: usize) -> Self {
        (self.x, self.y) = (Some(x), Some(y));
        self
    }
}

/// A budget's use and ceiling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Budget {
    pub used: usize,
    pub max: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Budgets {
    pub tiles: Budget,
    pub palettes: Budget,
}

/// A unit's size in pixels: a tile, or an obj cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct UnitSize {
    pub width: usize,
    pub height: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ArtCheck {
    pub unit: UnitSize,
    pub budgets: Budgets,
    pub findings: Vec<Finding>,
}

/// Every refusal the keep path has for this image, in the order import meets them. `gpl`, when
/// given, is the palette the PLTE must equal. Errs on what is no image at all (a kind without
/// pixels, a bad `bit_depth` or cell size), which import refuses the same way.
pub fn check_indexed(
    kind: SourceKind,
    opts: &ConvertOptions,
    png: &IndexedPng,
    gpl: Option<&PaletteSource>,
) -> Result<ArtCheck, String> {
    super::keep::analyze(kind, opts, png, gpl).map(|a| a.check)
}
