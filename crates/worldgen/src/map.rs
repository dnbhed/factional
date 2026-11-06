//! The map (P-119): its recipe, its generated tiles in chunks, in layers, and the designer's
//! overrides.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use factional_core::{Fixed, is_valid_id};

use crate::CHUNK;
use crate::suggestions::{Placed, Suggestions};
use crate::tile::{Biome, Override, Plant, PlantKind, Terrain, Tile, Water};

/// The most tiles a map may have each way.
pub const MAX_SIDE: u32 = 8192;

/// The detail's strengths, lowest and highest, as a share of the relief.
const STRENGTHS: [Fixed; 2] = [Fixed::ZERO, Fixed::ONE];

/// The detail's shortest longest wavelength, in source pixels.
const SHORTEST: Fixed = Fixed::ONE;

/// The fewest and most octaves of detail.
const OCTAVES: [u32; 2] = [1, 8];

/// The shallowest and deepest the sea's shallows may reach, in metres: none, and past any sea
/// Azgaar makes.
const SHALLOWS: [Fixed; 2] = [Fixed::ZERO, Fixed::from_hundredths(1000_00)];

/// The most tiles a speck may have: a chunk's.
const SPECKS: u32 = CHUNK * CHUNK;

/// The narrowest and widest spacing of levels, on Azgaar's height scale: from sea level at 20
/// to its top at 100 is one level at most.
const SPACINGS: [Fixed; 2] = [Fixed::ONE, Fixed::from_hundredths(80_00)];

/// The foliage's lowest and highest scale.
const FOLIAGE_SCALES: [Fixed; 2] = [Fixed::ZERO, Fixed::from_hundredths(10_00)];

/// The narrowest and widest spacing of plants, in tiles.
const PLANT_SPACINGS: [Fixed; 2] = [Fixed::ZERO, Fixed::from_hundredths(8_00)];

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
/// scale in tiles a source pixel, with which seed, with what detail, coast, levels and foliage.
/// Generation's other options join it as they land.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipe {
    /// The source's fingerprint; empty for a hand-made map.
    pub source: String,
    pub region: Region,
    pub scale: u32,
    pub seed: u32,
    pub detail: Detail,
    pub coast: Coast,
    pub levels: Levels,
    pub foliage: Foliage,
}

/// The detail added to the source's heights (W3a, P-136): seeded gradient noise, `strength`
/// times the local relief at most, its longest `wavelength` in source pixels, in `octaves`,
/// each half the wavelength and half the strength of the one before.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Detail {
    pub strength: Fixed,
    pub wavelength: Fixed,
    pub octaves: u32,
}

impl Default for Detail {
    /// A quarter of the relief, over 8 source pixels, in 4 octaves (DESIGN.md §21).
    fn default() -> Detail {
        Detail {
            strength: Fixed::from_hundredths(25),
            wavelength: Fixed::from_hundredths(8_00),
            octaves: 4,
        }
    }
}

/// How coasts are drawn (W3b, P-137): sea less deep than `shallows` metres is shallow, and a
/// group of land or water of fewer than `specks` tiles, away from the map's edge, becomes the
/// other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Coast {
    pub shallows: Fixed,
    pub specks: u32,
}

impl Default for Coast {
    /// Shallows to 10 metres, and specks of fewer than 4 tiles (DESIGN.md §21).
    fn default() -> Coast {
        Coast {
            shallows: Fixed::from_hundredths(10_00),
            specks: 4,
        }
    }
}

/// How elevation is drawn in 2D as levels (W3c, P-138): one every `spacing` points of
/// Azgaar's height scale above sea level, and plateaus and pits of fewer than `specks` tiles,
/// away from the map's edge, taking the nearest level around them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Levels {
    pub spacing: Fixed,
    pub specks: u32,
}

impl Default for Levels {
    /// A level every 5 points, 16 from sea level to the top, and specks of fewer than 4 tiles
    /// (DESIGN.md §21).
    fn default() -> Levels {
        Levels {
            spacing: Fixed::from_hundredths(5_00),
            specks: 4,
        }
    }
}

/// The plants scattered over the land (W5, P-143): each biome's chances of a tree, a shrub and
/// grass on a tile, all times `scale`, and how far apart each kind stands, in tiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Foliage {
    pub scale: Fixed,
    pub spacing: PerKind,
    /// Each biome's, in Azgaar's order (`Biome::ALL`).
    pub chances: [PerKind; 13],
}

/// A number for each kind of plant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerKind {
    pub tree: Fixed,
    pub shrub: Fixed,
    pub grass: Fixed,
}

impl PerKind {
    /// In hundredths: tree, shrub and grass.
    const fn of(tree: i64, shrub: i64, grass: i64) -> PerKind {
        PerKind {
            tree: Fixed::from_hundredths(tree),
            shrub: Fixed::from_hundredths(shrub),
            grass: Fixed::from_hundredths(grass),
        }
    }

    /// Its number for `kind`.
    pub fn of_kind(&self, kind: PlantKind) -> Fixed {
        match kind {
            PlantKind::Tree => self.tree,
            PlantKind::Shrub => self.shrub,
            PlantKind::Grass => self.grass,
        }
    }
}

impl Foliage {
    /// `biome`'s chances.
    pub fn chances_in(&self, biome: Biome) -> PerKind {
        self.chances[biome as usize]
    }
}

impl Default for Foliage {
    /// The chances by biome, at scale 1.00; trees 2 tiles apart, shrubs 1.5 and grass 1
    /// (DESIGN.md §21).
    fn default() -> Foliage {
        let none = PerKind::of(0, 0, 0);
        Foliage {
            scale: Fixed::ONE,
            spacing: PerKind::of(2_00, 1_50, 1_00),
            chances: [
                none,
                PerKind::of(0, 5, 2),
                PerKind::of(0, 5, 5),
                PerKind::of(5, 10, 40),
                PerKind::of(2, 5, 50),
                PerKind::of(40, 15, 10),
                PerKind::of(45, 15, 10),
                PerKind::of(60, 20, 5),
                PerKind::of(55, 20, 5),
                PerKind::of(45, 10, 5),
                PerKind::of(0, 5, 15),
                none,
                PerKind::of(10, 15, 40),
            ],
        }
    }
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
        let detail = self.detail;
        if !(STRENGTHS[0] <= detail.strength && detail.strength <= STRENGTHS[1]) {
            return Err(MapError::DetailStrength(detail.strength));
        }
        if detail.wavelength < SHORTEST {
            return Err(MapError::DetailWavelength(detail.wavelength));
        }
        if !(OCTAVES[0]..=OCTAVES[1]).contains(&detail.octaves) {
            return Err(MapError::DetailOctaves(detail.octaves));
        }
        let coast = self.coast;
        if !(SHALLOWS[0] <= coast.shallows && coast.shallows <= SHALLOWS[1]) {
            return Err(MapError::CoastShallows(coast.shallows));
        }
        if coast.specks > SPECKS {
            return Err(MapError::CoastSpecks(coast.specks));
        }
        let levels = self.levels;
        if !(SPACINGS[0] <= levels.spacing && levels.spacing <= SPACINGS[1]) {
            return Err(MapError::LevelSpacing(levels.spacing));
        }
        if levels.specks > SPECKS {
            return Err(MapError::LevelSpecks(levels.specks));
        }
        let foliage = self.foliage;
        let within = |value: Fixed, [low, high]: [Fixed; 2]| low <= value && value <= high;
        if !within(foliage.scale, FOLIAGE_SCALES) {
            return Err(MapError::FoliageScale(foliage.scale));
        }
        for kind in PlantKind::ALL {
            let spacing = foliage.spacing.of_kind(kind);
            if !within(spacing, PLANT_SPACINGS) {
                return Err(MapError::PlantSpacing(kind, spacing));
            }
        }
        for biome in Biome::ALL {
            let chances = foliage.chances_in(biome);
            for kind in PlantKind::ALL {
                let chance = chances.of_kind(kind);
                if !within(chance, [Fixed::ZERO, Fixed::ONE]) {
                    return Err(MapError::PlantChance(biome, kind, chance));
                }
            }
            let total = chances.tree + chances.shrub + chances.grass;
            if total > Fixed::ONE {
                return Err(MapError::PlantChances(biome, total));
            }
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
    DetailStrength(Fixed),
    DetailWavelength(Fixed),
    DetailOctaves(u32),
    CoastShallows(Fixed),
    CoastSpecks(u32),
    LevelSpacing(Fixed),
    LevelSpecks(u32),
    FoliageScale(Fixed),
    PlantSpacing(PlantKind, Fixed),
    PlantChance(Biome, PlantKind, Fixed),
    PlantChances(Biome, Fixed),
    OffTheMap { at: TilePos, size: Size },
    OtherSize { recipe: Size, map: Size },
    SuggestionId(String),
    SuggestionTwice(String),
    SuggestionOffTheMap { id: String, at: TilePos, size: Size },
    RouteTooShort { id: String, points: usize },
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
            MapError::DetailStrength(strength) => write!(
                f,
                "the detail's strength must be {} to {}, not {strength}",
                STRENGTHS[0], STRENGTHS[1]
            ),
            MapError::DetailWavelength(wavelength) => write!(
                f,
                "the detail's wavelength must be at least {SHORTEST} source pixel, not {wavelength}"
            ),
            MapError::DetailOctaves(octaves) => write!(
                f,
                "the detail needs {} to {} octaves, not {octaves}",
                OCTAVES[0], OCTAVES[1]
            ),
            MapError::CoastShallows(shallows) => write!(
                f,
                "the coast's shallows must be {} to {} metres, not {shallows}",
                SHALLOWS[0], SHALLOWS[1]
            ),
            MapError::CoastSpecks(specks) => write!(
                f,
                "the coast's specks must be 0 to {SPECKS} tiles, not {specks}"
            ),
            MapError::LevelSpacing(spacing) => write!(
                f,
                "the levels' spacing must be {} to {} of Azgaar's heights, not {spacing}",
                SPACINGS[0], SPACINGS[1]
            ),
            MapError::LevelSpecks(specks) => write!(
                f,
                "the levels' specks must be 0 to {SPECKS} tiles, not {specks}"
            ),
            MapError::FoliageScale(scale) => write!(
                f,
                "the foliage's scale must be {} to {}, not {scale}",
                FOLIAGE_SCALES[0], FOLIAGE_SCALES[1]
            ),
            MapError::PlantSpacing(kind, spacing) => write!(
                f,
                "the foliage's {} spacing must be {} to {} tiles, not {spacing}",
                kind.name(),
                PLANT_SPACINGS[0],
                PLANT_SPACINGS[1]
            ),
            MapError::PlantChance(biome, kind, chance) => write!(
                f,
                "the foliage's chance of {} in {} must be 0.00 to 1.00, not {chance}",
                match kind {
                    PlantKind::Grass => "grass",
                    PlantKind::Tree => "a tree",
                    PlantKind::Shrub => "a shrub",
                },
                biome.name()
            ),
            MapError::PlantChances(biome, total) => write!(
                f,
                "the foliage's chances in {} must add up to 1.00 at most, not {total}",
                biome.name()
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
            MapError::SuggestionId(id) => write!(
                f,
                "the suggestion id {id:?} isn't a valid id: a lowercase letter, then lowercase letters, digits or underscores"
            ),
            MapError::SuggestionTwice(id) => write!(f, "two suggestions are {id}"),
            MapError::SuggestionOffTheMap { id, at, size } => write!(
                f,
                "the suggestion {id} is at tile ({}, {}), off the map of {} × {} tiles",
                at.x, at.y, size.width, size.height
            ),
            MapError::RouteTooShort { id, points } => write!(
                f,
                "the route {id} has {points} point{}; a route needs at least two",
                if *points == 1 { "" } else { "s" }
            ),
        }
    }
}

/// Where a land tile meets a lower land neighbour (W3c, P-138): one level lower, a ledge,
/// which a game may let a walker jump down; more, a cliff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drop {
    Ledge,
    Cliff,
}

/// A tile's drop on each side, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drops {
    pub north: Option<Drop>,
    pub west: Option<Drop>,
    pub east: Option<Drop>,
    pub south: Option<Drop>,
}

/// Up to 64 × 64 tiles of the map, each layer row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    size: Size,
    elevation: Vec<Fixed>,
    level: Vec<u8>,
    terrain: Vec<Terrain>,
    water: Vec<Option<Water>>,
    biome: Vec<Biome>,
    temperature: Vec<i8>,
    precipitation: Vec<u16>,
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
            level: self.level[i],
            terrain: self.terrain[i],
            water: self.water[i],
            biome: self.biome[i],
            temperature: self.temperature[i],
            precipitation: self.precipitation[i],
            plant: self.plant[i],
        })
    }

    /// An empty chunk of `size`, to be filled row by row.
    fn empty(size: Size) -> Chunk {
        Chunk {
            size,
            elevation: Vec::new(),
            level: Vec::new(),
            terrain: Vec::new(),
            water: Vec::new(),
            biome: Vec::new(),
            temperature: Vec::new(),
            precipitation: Vec::new(),
            plant: Vec::new(),
        }
    }

    /// `tile` added after the last.
    fn push(&mut self, tile: Tile) {
        self.elevation.push(tile.elevation);
        self.level.push(tile.level);
        self.terrain.push(tile.terrain);
        self.water.push(tile.water);
        self.biome.push(tile.biome);
        self.temperature.push(tile.temperature);
        self.precipitation.push(tile.precipitation);
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

/// A world map: its recipe, the tiles it generated, in chunks, the designer's overrides, and
/// its suggestions (W6b).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldMap {
    recipe: Recipe,
    size: Size,
    chunks: Vec<Chunk>,
    overrides: BTreeMap<TilePos, Override>,
    suggestions: Suggestions,
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
            suggestions: Suggestions::default(),
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

    /// Where the tile at `at` drops to a lower neighbour, on each side, its override and theirs
    /// applied; if it's on the map.
    pub fn drops(&self, at: TilePos) -> Option<Drops> {
        let here = self.tile(at)?;
        let drop = |x: Option<u32>, y: Option<u32>| {
            let there = self.tile(TilePos { x: x?, y: y? })?;
            match here.level.checked_sub(there.level) {
                Some(1) => Some(Drop::Ledge),
                Some(2..) => Some(Drop::Cliff),
                _ => None,
            }
        };
        let (x, y) = (Some(at.x), Some(at.y));
        Some(Drops {
            north: drop(x, at.y.checked_sub(1)),
            west: drop(at.x.checked_sub(1), y),
            east: drop(at.x.checked_add(1), y),
            south: drop(x, at.y.checked_add(1)),
        })
    }

    /// Its suggestions: settlements, routes and points of interest.
    pub fn suggestions(&self) -> &Suggestions {
        &self.suggestions
    }

    /// Sets its suggestions to `suggestions`, or refuses them, unchanged, naming the first
    /// wrong: an id that isn't a valid content id or that two share, a place off the map, or a
    /// route of fewer than two points.
    pub fn suggest(&mut self, suggestions: Suggestions) -> Result<(), MapError> {
        {
            let mut ids = BTreeSet::new();
            let size = self.size;
            let mut check = |id: &str, places: &[Placed]| {
                if !is_valid_id(id) {
                    return Err(MapError::SuggestionId(id.to_owned()));
                }
                if !ids.insert(id.to_owned()) {
                    return Err(MapError::SuggestionTwice(id.to_owned()));
                }
                match places
                    .iter()
                    .find(|at| at.tile.x >= size.width || at.tile.y >= size.height)
                {
                    Some(off) => Err(MapError::SuggestionOffTheMap {
                        id: id.to_owned(),
                        at: off.tile,
                        size,
                    }),
                    None => Ok(()),
                }
            };
            for settlement in &suggestions.settlements {
                check(&settlement.id, std::slice::from_ref(&settlement.at))?;
            }
            for route in &suggestions.routes {
                check(&route.id, &route.points)?;
                if route.points.len() < 2 {
                    return Err(MapError::RouteTooShort {
                        id: route.id.clone(),
                        points: route.points.len(),
                    });
                }
            }
            for point in &suggestions.points {
                check(&point.id, std::slice::from_ref(&point.at))?;
            }
        }
        self.suggestions = suggestions;
        Ok(())
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
