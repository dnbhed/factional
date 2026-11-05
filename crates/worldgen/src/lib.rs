//! World maps (W1a, DESIGN.md §21, P-119): a map as tiles in 64 × 64 chunks, in layers, with
//! the recipe that made it and the designer's overrides, kept apart so that regenerating never
//! undoes them.
//!
//! No I/O lives here: no filesystem, network, stdout/stderr, environment variables, clock or
//! randomness. Generation is exact, so it uses integers and `Fixed`, never floats (P-119).
#![deny(clippy::float_arithmetic)]

mod coast;
mod levels;
mod map;
mod noise;
mod raster;
mod source;
mod tile;

pub use map::{
    Area, Chunk, ChunkPos, Coast, Detail, Drop, Drops, Levels, MAX_SIDE, MapError, Recipe, Region,
    Size, TilePos, WorldMap,
};
pub use raster::{
    HEIGHT_EXPONENTS, RasterError, cell_metres, cell_reliefs, height_metres, level_heights,
    rasterise,
};
pub use source::{Feature, Point, Source, SourceCell, SourceVertex};
pub use tile::{Biome, Override, Plant, PlantKind, Terrain, Tile, Water};

/// How many tiles a chunk has each way.
pub const CHUNK: u32 = 64;
