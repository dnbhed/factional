//! Coasts (W3b, P-137): land and water by each tile's height against the water near it, the
//! sea's shallows by depth, and specks too small to keep turned to what surrounds them.

use factional_core::Fixed;

use crate::tile::{Biome, Terrain, Tile, Water};

/// The water a tile can lie under: the sea, its surface at 0 m, or a lake at its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Body {
    pub(crate) surface: Fixed,
    pub(crate) lake: bool,
}

/// The tile at `elevation` under `water`, or land: sea less deep than `shallows` is shallow,
/// and deeper sea deep; a lake is shallow water, whatever its depth.
pub(crate) fn tile(elevation: Fixed, water: Option<Body>, shallows: Fixed) -> Tile {
    let (terrain, water, biome) = match water {
        None => (Terrain::Grass, None, Biome::Grassland),
        Some(Body { lake: true, .. }) => (Terrain::ShallowWater, Some(Water::Lake), Biome::Marine),
        Some(Body { surface, .. }) if surface - elevation < shallows => {
            (Terrain::ShallowWater, None, Biome::Marine)
        }
        Some(_) => (Terrain::DeepWater, None, Biome::Marine),
    };
    Tile {
        elevation,
        level: 0,
        terrain,
        water,
        biome,
        temperature: 0,
        precipitation: 0,
        plant: None,
    }
}

/// `tiles`, `width` a row and each the water it lies under or none for land, with every
/// speck turned to the other: each 4-connected group of land, or of water, of fewer than
/// `specks` tiles that doesn't touch the edge. Water goes first, to land; then land takes the
/// water above its first tile, which is the first tile bordering it, row by row.
pub(crate) fn remove_specks<T: Copy>(width: usize, tiles: &mut [Option<T>], specks: usize) {
    let height = tiles.len() / width.max(1);
    for water in [true, false] {
        for group in groups(width, tiles, water) {
            let edge = |i: usize| {
                let (x, y) = (i % width, i / width);
                x == 0 || x == width - 1 || y == 0 || y == height - 1
            };
            if group.len() < specks && !group.iter().any(|&i| edge(i)) {
                let other = match water {
                    true => None,
                    false => tiles[group[0] - width],
                };
                for i in group {
                    tiles[i] = other;
                }
            }
        }
    }
}

/// The 4-connected groups of `tiles` that are water, or land, each from its first tile row by
/// row: since the tiles are searched in that order, the one each group is found from.
fn groups<T>(width: usize, tiles: &[Option<T>], water: bool) -> Vec<Vec<usize>> {
    let mut seen = vec![false; tiles.len()];
    let mut groups = Vec::new();
    for first in 0..tiles.len() {
        if seen[first] || tiles[first].is_some() != water {
            continue;
        }
        seen[first] = true;
        let mut group = vec![first];
        // A tile joins a group once, so a group's tiles are reached in as many steps as there
        // are tiles.
        for step in 0..tiles.len() {
            let Some(&i) = group.get(step) else {
                break;
            };
            let (x, y) = (i % width, i / width);
            let around = [
                (y > 0).then(|| i - width),
                (x > 0).then(|| i - 1),
                (x + 1 < width).then(|| i + 1),
                (i + width < tiles.len()).then(|| i + width),
            ];
            for j in around.into_iter().flatten() {
                if !seen[j] && tiles[j].is_some() == water {
                    seen[j] = true;
                    group.push(j);
                }
            }
        }
        groups.push(group);
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A grid drawn a row a string: `.` is land, and a digit the water it lies under.
    fn grid(rows: &[&str]) -> (usize, Vec<Option<u32>>) {
        let tiles = rows
            .iter()
            .flat_map(|row| row.chars().map(|c| c.to_digit(10)))
            .collect();
        (rows[0].len(), tiles)
    }

    /// `rows` with its specks of fewer than `specks` tiles removed, drawn again.
    fn removed(rows: &[&str], specks: usize) -> Vec<String> {
        let (width, mut tiles) = grid(rows);
        remove_specks(width, &mut tiles, specks);
        tiles
            .chunks(width)
            .map(|row| {
                row.iter()
                    .map(|tile| {
                        tile.map_or('.', |water| char::from_digit(water, 10).expect("a digit"))
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn a_speck_touching_any_edge_is_kept() {
        // Each lake touches one edge; the land in the middle touches none, and takes the water
        // above it.
        let rows = [".1.", "2.3", ".4."];
        assert_eq!(removed(&rows, 4), [".1.", "213", ".4."]);
    }

    #[test]
    fn a_group_of_the_specks_size_is_kept() {
        let rows = ["111111", "1..1.1", "1..111", "111111"];
        assert_eq!(removed(&rows, 4), ["111111", "1..111", "1..111", "111111"]);
        assert_eq!(removed(&rows, 5), ["111111"; 4]);
    }

    #[test]
    fn tiles_meeting_only_at_a_corner_are_apart() {
        let rows = ["11111", "1.111", "11.11", "11111"];
        assert_eq!(removed(&rows, 2), ["11111"; 4]);
        let rows = ["....", ".2..", "..2.", "...."];
        assert_eq!(removed(&rows, 2), ["...."; 4]);
    }

    #[test]
    fn water_specks_go_before_land() {
        // A ring of 8 land tiles round a lake tile, in the sea: the lake becomes land, and
        // then the 9 tiles of land become sea. Taking the land first would leave the lake.
        let rows = ["11111", "1...1", "1.2.1", "1...1", "11111"];
        assert_eq!(removed(&rows, 10), ["11111"; 5]);
        assert_eq!(
            removed(&rows, 9),
            ["11111", "1...1", "1...1", "1...1", "11111"]
        );
    }

    #[test]
    fn land_takes_the_water_above_its_first_tile() {
        // Not the water left of it, nor above any other tile.
        let rows = ["13111", "2..11", "11.11", "11111"];
        assert_eq!(removed(&rows, 4), ["13111", "23311", "11311", "11111"]);
        // The first tile, row by row, is not the leftmost.
        let rows = ["11411", "11.11", "1..11", "11111"];
        assert_eq!(removed(&rows, 4), ["11411", "11411", "14411", "11111"]);
    }

    #[test]
    fn no_specks_removes_nothing() {
        let rows = ["111", "1.1", "111"];
        for specks in [0, 1] {
            assert_eq!(removed(&rows, specks), rows);
        }
        assert_eq!(removed(&rows, 2), ["111"; 3]);
    }
}
