//! Terrain (W4b, P-142): each land tile's terrain from its biome by a fixed table, rock where it
//! drops by a cliff and sand beside the sea, a glacier ice whatever is around it.

use crate::tile::{Biome, Terrain};

/// What a tile is, for its terrain: land on its biome, the sea, or a lake's or river's water.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Land(Biome),
    Sea,
    Fresh,
}

/// The terrain land on `biome` takes, by the table.
pub(crate) fn of(biome: Biome) -> Terrain {
    match biome {
        Biome::Marine | Biome::HotDesert => Terrain::Sand,
        Biome::ColdDesert => Terrain::Rock,
        Biome::Savanna | Biome::Grassland => Terrain::Grass,
        Biome::TropicalSeasonalForest
        | Biome::TemperateDeciduousForest
        | Biome::TropicalRainforest
        | Biome::TemperateRainforest
        | Biome::Taiga => Terrain::ForestFloor,
        Biome::Tundra => Terrain::Snow,
        Biome::Glacier => Terrain::Ice,
        Biome::Wetland => Terrain::Marsh,
    }
}

/// The terrain of each land tile of `kinds`, `width` a row, at its `levels`; none for water.
/// A glacier is ice; otherwise a tile that drops by a cliff, two levels or more, to any of its
/// four neighbours on the map is rock, and one beside the sea sand; otherwise its biome's.
pub(crate) fn terrains(width: usize, kinds: &[Kind], levels: &[u8]) -> Vec<Option<Terrain>> {
    let height = kinds.len() / width;
    (0..kinds.len())
        .map(|i| {
            let Kind::Land(biome) = kinds[i] else {
                return None;
            };
            let (x, y) = (i % width, i / width);
            let neighbours = [
                (y > 0).then(|| i - width),
                (x > 0).then(|| i - 1),
                (x + 1 < width).then(|| i + 1),
                (y + 1 < height).then(|| i + width),
            ];
            let neighbours = neighbours.into_iter().flatten();
            Some(if biome == Biome::Glacier {
                Terrain::Ice
            } else if neighbours.clone().any(|n| levels[n] + 2 <= levels[i]) {
                Terrain::Rock
            } else if neighbours.into_iter().any(|n| kinds[n] == Kind::Sea) {
                Terrain::Sand
            } else {
                of(biome)
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `kinds` drawn a row a string, `~` the sea, `=` a lake's or river's water and a letter a
    /// biome's land; at `levels`, a digit a tile; their terrains drawn again, `~` for water.
    fn drawn(kinds: &[&str], levels: &[&str]) -> Vec<String> {
        let width = kinds[0].len();
        let kinds: Vec<Kind> = kinds
            .iter()
            .flat_map(|row| row.chars())
            .map(|c| match c {
                '~' => Kind::Sea,
                '=' => Kind::Fresh,
                _ => Kind::Land(biome(c)),
            })
            .collect();
        let levels: Vec<u8> = levels
            .iter()
            .flat_map(|row| row.chars())
            .map(|c| c.to_digit(10).expect("a digit") as u8)
            .collect();
        terrains(width, &kinds, &levels)
            .chunks(width)
            .map(|row| row.iter().map(|terrain| letter(*terrain)).collect())
            .collect()
    }

    fn biome(c: char) -> Biome {
        match c {
            'M' => Biome::Marine,
            'H' => Biome::HotDesert,
            'C' => Biome::ColdDesert,
            'S' => Biome::Savanna,
            'G' => Biome::Grassland,
            'P' => Biome::TropicalSeasonalForest,
            'D' => Biome::TemperateDeciduousForest,
            'R' => Biome::TropicalRainforest,
            'T' => Biome::TemperateRainforest,
            'A' => Biome::Taiga,
            'U' => Biome::Tundra,
            'I' => Biome::Glacier,
            _ => Biome::Wetland,
        }
    }

    fn letter(terrain: Option<Terrain>) -> char {
        match terrain {
            None => '~',
            Some(Terrain::Sand) => 's',
            Some(Terrain::Grass) => 'g',
            Some(Terrain::ForestFloor) => 'f',
            Some(Terrain::Marsh) => 'm',
            Some(Terrain::Rock) => 'r',
            Some(Terrain::Snow) => 'n',
            Some(Terrain::Ice) => 'i',
            Some(Terrain::DeepWater | Terrain::ShallowWater) => '!',
        }
    }

    #[test]
    fn each_biome_takes_its_terrain_by_the_table() {
        // Marine, hot and cold desert, savanna, grassland, the four forests, taiga, tundra,
        // glacier and wetland, on level ground with no sea.
        assert_eq!(
            drawn(&["MHCSGPDRTAUIW"], &["0000000000000"]),
            ["ssrggfffffnim"]
        );
    }

    #[test]
    fn a_tile_dropping_by_a_cliff_is_rock_and_by_a_ledge_is_not() {
        // (0, 1) drops one level to its neighbours, (0, 2) two to (0, 1) and three to (1, 2);
        // the lower tiles of a cliff keep their terrain.
        assert_eq!(
            drawn(&["GGG", "GGG", "GGG"], &["000", "100", "300"]),
            ["ggg", "ggg", "rgg"]
        );
        // A cliff on any side: north, west, east and south.
        for levels in [
            ["000", "020", "000"],
            ["222", "022", "222"],
            ["222", "220", "222"],
            ["222", "222", "202"],
        ] {
            let terrains = drawn(&["DDD", "DDD", "DDD"], &levels);
            assert_eq!(terrains[1].as_bytes()[1], b'r', "{levels:?}");
        }
    }

    #[test]
    fn land_beside_the_sea_is_sand_a_tile_deep_unless_it_drops_by_a_cliff() {
        assert_eq!(
            drawn(&["~~~", "GGG", "GGG"], &["000", "000", "000"]),
            ["~~~", "sss", "ggg"]
        );
        assert_eq!(
            drawn(&["~~~", "GGG", "GGG"], &["000", "121", "000"]),
            ["~~~", "srs", "ggg"]
        );
        // Beside it on any side.
        assert_eq!(
            drawn(&["GGG", "G~G", "GGG"], &["000", "000", "000"]),
            ["gsg", "s~s", "gsg"]
        );
    }

    #[test]
    fn a_lakes_or_rivers_shore_keeps_its_biomes_terrain() {
        assert_eq!(
            drawn(&["===", "AAA", "~AA"], &["000", "000", "000"]),
            ["~~~", "sff", "~sf"]
        );
    }

    #[test]
    fn a_glacier_is_ice_by_the_sea_and_at_a_cliff() {
        assert_eq!(drawn(&["~I", "II"], &["00", "30"]), ["~i", "ii"]);
    }

    #[test]
    fn the_maps_edge_is_no_neighbour() {
        assert_eq!(drawn(&["UU", "UU"], &["55", "55"]), ["nn", "nn"]);
        assert_eq!(drawn(&["W"], &["9"]), ["m"]);
    }
}
