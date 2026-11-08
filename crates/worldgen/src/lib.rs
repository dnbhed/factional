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
mod preview;
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
pub use preview::{Preview, Shade, preview};
pub use raster::{
    HEIGHT_EXPONENTS, RasterError, cell_metres, cell_reliefs, height_metres, level_heights,
    rasterise, rasterised_size,
};
pub use source::{
    Buildings, Extent, Feature, Point, ReligionKind, RouteKind, SettlementKind, Source, SourceCell,
    SourceCulture, SourceMarker, SourceReligion, SourceRiver, SourceRoute, SourceSettlement,
    SourceState, SourceVertex, Stance, StateForm,
};
pub use suggestions::{
    Culture, Placed, PointOfInterest, Religion, Route, Settlement, State, Suggestions,
};
pub use tile::{Biome, Override, Plant, PlantKind, Terrain, Tile, Water};

/// How many tiles a chunk has each way.
pub const CHUNK: u32 = 64;
