//! A tile's layers (P-119): elevation, terrain, water, biome and plant; and an override of any
//! of them.

use factional_core::Fixed;

/// What a tile is drawn as and walked on: a closed vocabulary (DESIGN.md §21).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Terrain {
    DeepWater,
    ShallowWater,
    Sand,
    Grass,
    ForestFloor,
    Marsh,
    Rock,
    Snow,
    Ice,
}

/// A tile's biome: Azgaar's thirteen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Biome {
    Marine,
    HotDesert,
    ColdDesert,
    Savanna,
    Grassland,
    TropicalSeasonalForest,
    TemperateDeciduousForest,
    TropicalRainforest,
    TemperateRainforest,
    Taiga,
    Tundra,
    Glacier,
    Wetland,
}

/// Water on a tile: a river, with its flow in cubic metres a second, or a lake.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Water {
    River { flow: u32 },
    Lake,
}

/// What kind of plant stands on a tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PlantKind {
    Tree,
    Shrub,
    Grass,
}

/// A plant on a tile: its kind, and where it stands within the tile, in 256ths of a tile from
/// its top left, x then y; scattered, not tiled (§21).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Plant {
    pub kind: PlantKind,
    pub offset: [u8; 2],
}

/// One tile, every layer: its elevation in metres, kept fine for 3D (D-50); its terrain; its
/// water; its biome; and its plant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile {
    pub elevation: Fixed,
    pub terrain: Terrain,
    pub water: Option<Water>,
    pub biome: Biome,
    pub plant: Option<Plant>,
}

/// The designer's edit of one tile: each layer it sets, kept apart from what was generated.
/// Water and a plant are set to `Some(None)` to clear them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Override {
    pub elevation: Option<Fixed>,
    pub terrain: Option<Terrain>,
    pub water: Option<Option<Water>>,
    pub biome: Option<Biome>,
    pub plant: Option<Option<Plant>>,
}

impl Override {
    /// Whether it sets nothing.
    pub fn is_empty(&self) -> bool {
        *self == Override::default()
    }

    /// `later` added to this: each layer `later` sets replaces this one's.
    pub fn and(self, later: Override) -> Override {
        Override {
            elevation: later.elevation.or(self.elevation),
            terrain: later.terrain.or(self.terrain),
            water: later.water.or(self.water),
            biome: later.biome.or(self.biome),
            plant: later.plant.or(self.plant),
        }
    }

    /// `tile` with this override applied.
    pub fn apply(&self, tile: Tile) -> Tile {
        Tile {
            elevation: self.elevation.unwrap_or(tile.elevation),
            terrain: self.terrain.unwrap_or(tile.terrain),
            water: self.water.unwrap_or(tile.water),
            biome: self.biome.unwrap_or(tile.biome),
            plant: self.plant.unwrap_or(tile.plant),
        }
    }
}
