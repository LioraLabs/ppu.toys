//! Asset importers: PNG -> authentic VRAM/CGRAM/register data (m4/importer).
//! `quantize`/`tiles` are the shared primitives Mode-7 and OBJ import reuse;
//! this module's own surface is the tile-BG importer.

pub mod quantize;
