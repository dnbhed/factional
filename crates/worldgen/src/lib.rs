//! World maps (W1a, DESIGN.md §21, P-119): a map as tiles in 64 × 64 chunks, in layers, with
//! the recipe that made it and the designer's overrides, kept apart so that regenerating never
//! undoes them.
//!
//! No I/O lives here: no filesystem, network, stdout/stderr, environment variables, clock or
//! randomness. Generation is exact, so it uses integers and `Fixed`, never floats (P-119).
#![deny(clippy::float_arithmetic)]

mod coast;
mod foliage;
mod levels;
mod map;
mod noise;
mod raster;
mod rivers;
mod source;
mod suggestions;
mod terrain;
mod tile;

pub use map::{
    Area, Chunk, ChunkPos, Coast, Detail, Drop, Drops, Foliage, Levels, MAX_SIDE, MapError,
    PerKind, Recipe, Region, Size, TilePos, WorldMap,
};
pub use raster::{
    HEIGHT_EXPONENTS, RasterError, cell_metres, cell_reliefs, height_metres, level_heights,
    rasterise,
};
pub use source::{
    Buildings, Feature, Point, RouteKind, SettlementKind, Source, SourceCell, SourceMarker,
    SourceRiver, SourceRoute, SourceSettlement, SourceVertex,
};
pub use suggestions::{Placed, PointOfInterest, Route, Settlement, Suggestions};
pub use tile::{Biome, Override, Plant, PlantKind, Terrain, Tile, Water};

/// How many tiles a chunk has each way.
pub const CHUNK: u32 = 64;
