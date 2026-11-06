//! A tile's layers (P-119): elevation, its level, terrain, water, biome, climate and plant, and
//! the culture, religion and state it lies in (W6f); and an override of any of them.

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

impl Terrain {
    /// Its name, as a designer reads it.
    pub fn name(self) -> &'static str {
        match self {
            Terrain::DeepWater => "deep water",
            Terrain::ShallowWater => "shallow water",
            Terrain::Sand => "sand",
            Terrain::Grass => "grass",
            Terrain::ForestFloor => "forest floor",
            Terrain::Marsh => "marsh",
            Terrain::Rock => "rock",
            Terrain::Snow => "snow",
            Terrain::Ice => "ice",
        }
    }
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

impl Biome {
    /// Every biome, in Azgaar's order.
    pub const ALL: [Biome; 13] = [
        Biome::Marine,
        Biome::HotDesert,
        Biome::ColdDesert,
        Biome::Savanna,
        Biome::Grassland,
        Biome::TropicalSeasonalForest,
        Biome::TemperateDeciduousForest,
        Biome::TropicalRainforest,
        Biome::TemperateRainforest,
        Biome::Taiga,
        Biome::Tundra,
        Biome::Glacier,
        Biome::Wetland,
    ];

    /// Its name, as a designer reads it.
    pub fn name(self) -> &'static str {
        match self {
            Biome::Marine => "marine",
            Biome::HotDesert => "hot desert",
            Biome::ColdDesert => "cold desert",
            Biome::Savanna => "savanna",
            Biome::Grassland => "grassland",
            Biome::TropicalSeasonalForest => "tropical seasonal forest",
            Biome::TemperateDeciduousForest => "temperate deciduous forest",
            Biome::TropicalRainforest => "tropical rainforest",
            Biome::TemperateRainforest => "temperate rainforest",
            Biome::Taiga => "taiga",
            Biome::Tundra => "tundra",
            Biome::Glacier => "glacier",
            Biome::Wetland => "wetland",
        }
    }
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

impl PlantKind {
    /// Every kind, in order.
    pub const ALL: [PlantKind; 3] = [PlantKind::Tree, PlantKind::Shrub, PlantKind::Grass];

    /// Its name, as a designer reads it.
    pub fn name(self) -> &'static str {
        match self {
            PlantKind::Tree => "tree",
            PlantKind::Shrub => "shrub",
            PlantKind::Grass => "grass",
        }
    }
}

/// A plant on a tile: its kind, and where it stands within the tile, in 256ths of a tile from
/// its top left, x then y; scattered, not tiled (§21).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Plant {
    pub kind: PlantKind,
    pub offset: [u8; 2],
}

/// One tile, every layer: its elevation in metres, kept fine for 3D (D-50); its level, as 2D
/// draws it (W3c), water's that of its surface (W3e); its terrain; its water; its biome; its
/// yearly temperature in whole °C and precipitation in mm (W4a); its plant; and the culture,
/// religion and state it lies in, by Azgaar's id, if any (W6f).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile {
    pub elevation: Fixed,
    pub level: u8,
    pub terrain: Terrain,
    pub water: Option<Water>,
    pub biome: Biome,
    pub temperature: i8,
    pub precipitation: u16,
    pub plant: Option<Plant>,
    pub culture: Option<u32>,
    pub religion: Option<u32>,
    pub state: Option<u32>,
}

/// The designer's edit of one tile: each layer it sets, kept apart from what was generated.
/// Water, a plant, a culture, a religion and a state are set to `Some(None)` to clear them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Override {
    pub elevation: Option<Fixed>,
    pub level: Option<u8>,
    pub terrain: Option<Terrain>,
    pub water: Option<Option<Water>>,
    pub biome: Option<Biome>,
    pub temperature: Option<i8>,
    pub precipitation: Option<u16>,
    pub plant: Option<Option<Plant>>,
    pub culture: Option<Option<u32>>,
    pub religion: Option<Option<u32>>,
    pub state: Option<Option<u32>>,
}

impl Tile {
    /// Whether it's under water, deep or shallow: as terrain has it, so an override of the
    /// terrain moves it.
    pub fn is_water(&self) -> bool {
        matches!(self.terrain, Terrain::DeepWater | Terrain::ShallowWater)
    }
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
            level: later.level.or(self.level),
            terrain: later.terrain.or(self.terrain),
            water: later.water.or(self.water),
            biome: later.biome.or(self.biome),
            temperature: later.temperature.or(self.temperature),
            precipitation: later.precipitation.or(self.precipitation),
            plant: later.plant.or(self.plant),
            culture: later.culture.or(self.culture),
            religion: later.religion.or(self.religion),
            state: later.state.or(self.state),
        }
    }

    /// `tile` with this override applied.
    pub fn apply(&self, tile: Tile) -> Tile {
        Tile {
            elevation: self.elevation.unwrap_or(tile.elevation),
            level: self.level.unwrap_or(tile.level),
            terrain: self.terrain.unwrap_or(tile.terrain),
            water: self.water.unwrap_or(tile.water),
            biome: self.biome.unwrap_or(tile.biome),
            temperature: self.temperature.unwrap_or(tile.temperature),
            precipitation: self.precipitation.unwrap_or(tile.precipitation),
            plant: self.plant.unwrap_or(tile.plant),
            culture: self.culture.unwrap_or(tile.culture),
            religion: self.religion.unwrap_or(tile.religion),
            state: self.state.unwrap_or(tile.state),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_terrain_is_named_as_a_designer_reads_it() {
        use Terrain::*;
        let names = [
            DeepWater,
            ShallowWater,
            Sand,
            Grass,
            ForestFloor,
            Marsh,
            Rock,
            Snow,
            Ice,
        ]
        .map(Terrain::name);
        assert_eq!(
            names,
            [
                "deep water",
                "shallow water",
                "sand",
                "grass",
                "forest floor",
                "marsh",
                "rock",
                "snow",
                "ice"
            ]
        );
    }
}
