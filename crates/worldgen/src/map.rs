//! The map (P-119): its recipe, its generated tiles in chunks, in layers, and the designer's
//! overrides.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;

use factional_core::Fixed;

use crate::CHUNK;
use crate::tile::{Biome, Override, Plant, Terrain, Tile, Water};

/// The most tiles a map may have each way.
pub const MAX_SIDE: u32 = 8192;

/// A width and a height, in tiles or source pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

/// A tile, by its column and row from the map's top left.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TilePos {
    pub x: u32,
    pub y: u32,
}

impl Ord for TilePos {
    /// Row by row: by row, then by column.
    fn cmp(&self, other: &TilePos) -> Ordering {
        (self.y, self.x).cmp(&(other.y, other.x))
    }
}

impl PartialOrd for TilePos {
    fn partial_cmp(&self, other: &TilePos) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A chunk, by its column and row of chunks from the map's top left.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkPos {
    pub x: u32,
    pub y: u32,
}

/// A rectangle of tiles: its top left, and its size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Area {
    pub from: TilePos,
    pub size: Size,
}

/// A rectangle of the source map, in its pixels: its top left, and its size (D-53).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub size: Size,
}

/// How a map was made (P-119): from which source, which of its layers, which region at what
/// scale in tiles a source pixel, and with which seed. Generation's options join it as they
/// land.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipe {
    /// The source's fingerprint; empty for a hand-made map.
    pub source: String,
    pub region: Region,
    pub scale: u32,
    pub seed: u64,
}

impl Recipe {
    /// The size of the map it makes, in tiles, or why it can't make one.
    pub fn size(&self) -> Result<Size, MapError> {
        if self.scale == 0 {
            return Err(MapError::ScaleZero);
        }
        let region = self.region.size;
        if region.width == 0 || region.height == 0 {
            return Err(MapError::EmptyRegion);
        }
        let side = |pixels: u32| u64::from(pixels) * u64::from(self.scale);
        let (width, height) = (side(region.width), side(region.height));
        match (u32::try_from(width), u32::try_from(height)) {
            (Ok(w), Ok(h)) if w <= MAX_SIDE && h <= MAX_SIDE => Ok(Size {
                width: w,
                height: h,
            }),
            _ => Err(MapError::TooLarge { width, height }),
        }
    }
}

/// Why a map can't be made or changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapError {
    ScaleZero,
    EmptyRegion,
    TooLarge { width: u64, height: u64 },
    OffTheMap { at: TilePos, size: Size },
    OtherSize { recipe: Size, map: Size },
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MapError::ScaleZero => write!(f, "the scale must be at least 1 tile a source pixel"),
            MapError::EmptyRegion => {
                write!(f, "the region must be at least 1 source pixel each way")
            }
            MapError::TooLarge { width, height } => write!(
                f,
                "the map would be {width} × {height} tiles; at most {MAX_SIDE} a side"
            ),
            MapError::OffTheMap { at, size } => write!(
                f,
                "tile ({}, {}) is off the map, which is {} × {} tiles",
                at.x, at.y, size.width, size.height
            ),
            MapError::OtherSize { recipe, map } => write!(
                f,
                "the recipe makes a map of {} × {} tiles, and this one is {} × {}; regenerating can't move the overrides",
                recipe.width, recipe.height, map.width, map.height
            ),
        }
    }
}

/// Up to 64 × 64 tiles of the map, each layer row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    size: Size,
    elevation: Vec<Fixed>,
    terrain: Vec<Terrain>,
    water: Vec<Option<Water>>,
    biome: Vec<Biome>,
    plant: Vec<Option<Plant>>,
}

impl Chunk {
    /// Its size: 64 × 64, or less at the map's right and bottom edges.
    pub fn size(&self) -> Size {
        self.size
    }

    /// The tile at `x, y` within it, if it's there.
    pub fn tile(&self, x: u32, y: u32) -> Option<Tile> {
        if x >= self.size.width || y >= self.size.height {
            return None;
        }
        let i = (y * self.size.width + x) as usize;
        Some(Tile {
            elevation: self.elevation[i],
            terrain: self.terrain[i],
            water: self.water[i],
            biome: self.biome[i],
            plant: self.plant[i],
        })
    }

    /// An empty chunk of `size`, to be filled row by row.
    fn empty(size: Size) -> Chunk {
        Chunk {
            size,
            elevation: Vec::new(),
            terrain: Vec::new(),
            water: Vec::new(),
            biome: Vec::new(),
            plant: Vec::new(),
        }
    }

    /// `tile` added after the last.
    fn push(&mut self, tile: Tile) {
        self.elevation.push(tile.elevation);
        self.terrain.push(tile.terrain);
        self.water.push(tile.water);
        self.biome.push(tile.biome);
        self.plant.push(tile.plant);
    }
}

/// The chunks of a map of `size`, row by row, each tile from `generate`, asked once for each,
/// row by row across the whole map.
fn generate_chunks(size: Size, mut generate: impl FnMut(TilePos) -> Tile) -> Vec<Chunk> {
    let count = chunk_count(size);
    let mut chunks: Vec<Chunk> = (0..count.height)
        .flat_map(|cy| (0..count.width).map(move |cx| (cx, cy)))
        .map(|(cx, cy)| {
            Chunk::empty(Size {
                width: (size.width - cx * CHUNK).min(CHUNK),
                height: (size.height - cy * CHUNK).min(CHUNK),
            })
        })
        .collect();
    for y in 0..size.height {
        for x in 0..size.width {
            let chunk = (y / CHUNK * count.width + x / CHUNK) as usize;
            chunks[chunk].push(generate(TilePos { x, y }));
        }
    }
    chunks
}

/// How many chunks a map of `size` has each way.
fn chunk_count(size: Size) -> Size {
    Size {
        width: size.width.div_ceil(CHUNK),
        height: size.height.div_ceil(CHUNK),
    }
}

/// A world map: its recipe, the tiles it generated, in chunks, and the designer's overrides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldMap {
    recipe: Recipe,
    size: Size,
    chunks: Vec<Chunk>,
    overrides: BTreeMap<TilePos, Override>,
}

impl WorldMap {
    /// The map `recipe` makes, each tile from `generate`, asked once for each, row by row.
    pub fn new(
        recipe: Recipe,
        generate: impl FnMut(TilePos) -> Tile,
    ) -> Result<WorldMap, MapError> {
        let size = recipe.size()?;
        Ok(WorldMap {
            recipe,
            size,
            chunks: generate_chunks(size, generate),
            overrides: BTreeMap::new(),
        })
    }

    pub fn recipe(&self) -> &Recipe {
        &self.recipe
    }

    /// Its size in tiles.
    pub fn size(&self) -> Size {
        self.size
    }

    /// How many chunks it has each way.
    pub fn chunk_count(&self) -> Size {
        chunk_count(self.size)
    }

    /// Each chunk, row by row.
    pub fn chunks(&self) -> impl Iterator<Item = (ChunkPos, &Chunk)> {
        let across = self.chunk_count().width;
        self.chunks.iter().zip(0u32..).map(move |(chunk, i)| {
            let pos = ChunkPos {
                x: i % across,
                y: i / across,
            };
            (pos, chunk)
        })
    }

    /// The tile at `at` as generated, if it's on the map.
    pub fn generated(&self, at: TilePos) -> Option<Tile> {
        if at.x >= self.size.width || at.y >= self.size.height {
            return None;
        }
        let chunk = (at.y / CHUNK * self.chunk_count().width + at.x / CHUNK) as usize;
        self.chunks[chunk].tile(at.x % CHUNK, at.y % CHUNK)
    }

    /// The tile at `at`, its override applied, if it's on the map.
    pub fn tile(&self, at: TilePos) -> Option<Tile> {
        let generated = self.generated(at)?;
        Some(match self.overrides.get(&at) {
            Some(edit) => edit.apply(generated),
            None => generated,
        })
    }

    /// The designer's overrides, row by row.
    pub fn overrides(&self) -> &BTreeMap<TilePos, Override> {
        &self.overrides
    }

    /// Adds `edit` to the override of the tile at `at`; one that then sets nothing is removed.
    pub fn edit(&mut self, at: TilePos, edit: Override) -> Result<(), MapError> {
        if self.generated(at).is_none() {
            return Err(MapError::OffTheMap {
                at,
                size: self.size,
            });
        }
        let edited = self
            .overrides
            .get(&at)
            .copied()
            .unwrap_or_default()
            .and(edit);
        match edited.is_empty() {
            true => self.overrides.remove(&at),
            false => self.overrides.insert(at, edited),
        };
        Ok(())
    }

    /// Clears the overrides in `area`, back to what was generated; how many it cleared.
    pub fn reset(&mut self, area: Area) -> usize {
        let inside = |at: &TilePos| {
            let across = u64::from(at.x).checked_sub(u64::from(area.from.x));
            let down = u64::from(at.y).checked_sub(u64::from(area.from.y));
            across.is_some_and(|across| across < u64::from(area.size.width))
                && down.is_some_and(|down| down < u64::from(area.size.height))
        };
        let before = self.overrides.len();
        self.overrides.retain(|at, _| !inside(at));
        before - self.overrides.len()
    }

    /// Every tile made again from `recipe`, which must make a map this size, and `generate`;
    /// the overrides kept.
    pub fn regenerate(
        &mut self,
        recipe: Recipe,
        generate: impl FnMut(TilePos) -> Tile,
    ) -> Result<(), MapError> {
        let size = recipe.size()?;
        if size != self.size {
            return Err(MapError::OtherSize {
                recipe: size,
                map: self.size,
            });
        }
        self.chunks = generate_chunks(size, generate);
        self.recipe = recipe;
        Ok(())
    }
}
