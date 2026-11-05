//! A source map (W2a, P-119): what Factional keeps of an Azgaar export, read and checked by
//! `factional-content`, so every reference here resolves.

use factional_core::Fixed;

use crate::Size;

/// A point on the source map, in its pixels, to the hundredth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: Fixed,
    pub y: Fixed,
}

/// One of the source's cells: its centre, its height (0 to 100, sea level at 20, as Azgaar
/// has it), the feature it's in, its neighbouring cells and the vertices at its corners,
/// each by its place in the source's lists, and its flux, the water flowing through it in
/// Azgaar's cubic metres a second.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceCell {
    pub centre: Point,
    pub height: u8,
    pub feature: u32,
    pub neighbours: Vec<u32>,
    pub corners: Vec<u32>,
    pub flux: u32,
}

/// One of the source's vertices: where it is, and the three cells that meet there, `None`
/// where one is past the map's edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceVertex {
    pub at: Point,
    pub cells: [Option<u32>; 3],
}

/// A feature of the source: an ocean, a lake with its surface height on Azgaar's scale, or
/// an island.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    Ocean,
    Lake { surface: Fixed },
    Island,
}

/// One of the source's rivers (W3d, P-139): its cells from its source to where it ends, the
/// water it flows into or the river it joins, or past the map's edge if `off_map`; and the
/// width Azgaar draws it at, its factor and its half-width at the source in pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRiver {
    pub cells: Vec<u32>,
    pub off_map: bool,
    pub width_factor: Fixed,
    pub source_width: Fixed,
}

/// A source map: its name, the generator's version that made it, its size in pixels, the
/// exponent its heights are raised to in metres, and its cells, vertices, features and
/// rivers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub name: String,
    pub version: String,
    pub size: Size,
    pub height_exponent: Fixed,
    pub cells: Vec<SourceCell>,
    pub vertices: Vec<SourceVertex>,
    pub features: Vec<Feature>,
    pub rivers: Vec<SourceRiver>,
}
