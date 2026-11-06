//! Foliage (W5, P-143): trees, shrubs and grass scattered over the land by a local Poisson-disc
//! rule. Each tile hashes its whole-map position with the seed into a roll, an offset and a
//! priority; it's a candidate when its roll falls under its biome's chances; and a candidate
//! stands unless another candidate too close to it outranks it. Each plant is decided from the
//! tiles around it alone, so a region has the whole map's plants away from its edges.

use factional_core::Fixed;

use crate::map::PerKind;
use crate::noise;
use crate::tile::{Plant, PlantKind};

/// The first part of a tile's hash, apart from every octave's of the detail.
const FOLIAGE: u64 = 1 << 32;

/// A plant that may stand on a tile: its kind, its offset within the tile in 256ths, x then y,
/// and its priority over the candidates around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub(crate) kind: PlantKind,
    pub(crate) offset: [u8; 2],
    pub(crate) priority: u64,
}

/// The hash of the tile at `at` on the whole map, with `seed`.
pub(crate) fn hash(seed: u32, [x, y]: [u64; 2]) -> u64 {
    noise::hash(seed, [FOLIAGE, x, y])
}

/// The candidate on a tile with `hash`, under its biome's `chances` at `scale`, if any: its roll
/// in ten-thousandths from the hash's low 32 bits, its offset from the next two bytes, and its
/// priority the hash mixed again. A tree when the roll is under the tree's chance, a shrub
/// under the tree's and shrub's, grass under all three's, each times the scale and the sum
/// capped at certainty.
pub(crate) fn candidate(hash: u64, chances: PerKind, scale: Fixed) -> Option<Candidate> {
    let roll = ((hash & 0xffff_ffff) * 10_000) >> 32;
    let scale = u64::try_from(scale.hundredths()).expect("a scale of 0.00 or more");
    let mut under = 0;
    let kind = PlantKind::ALL.into_iter().find(|&kind| {
        let chance = u64::try_from(chances.of_kind(kind).hundredths()).expect("0.00 or more");
        under = (under + chance * scale).min(10_000);
        roll < under
    })?;
    Some(Candidate {
        kind,
        offset: [(hash >> 32) as u8, (hash >> 40) as u8],
        priority: noise::mix(hash),
    })
}

/// Whether points `a` and `b`, in 256ths of a tile, are nearer than `spacing` tiles: exactly,
/// with no rounding.
pub(crate) fn too_close(a: [i64; 2], b: [i64; 2], spacing: Fixed) -> bool {
    let squared = i128::from(a[0] - b[0]).pow(2) + i128::from(a[1] - b[1]).pow(2);
    squared * 10_000 < (i128::from(spacing.hundredths()) * 256).pow(2)
}

/// The plants of `candidates`, `width` a row, each on its tile: a candidate stands unless
/// another is too close, nearer than the larger of their kinds' `spacing`, and outranks it,
/// with a higher priority, ties to the earlier tile in reading order.
pub(crate) fn plants(
    width: usize,
    candidates: &[Option<Candidate>],
    spacing: PerKind,
) -> Vec<Option<Plant>> {
    let height = candidates.len() / width;
    let widest = PlantKind::ALL
        .map(|kind| spacing.of_kind(kind).hundredths())
        .into_iter()
        .max()
        .expect("three kinds");
    let reach = usize::try_from(widest).expect("0.00 or more").div_ceil(100);
    let point = |i: usize, offset: [u8; 2]| {
        let at = |tile: usize, offset: u8| tile as i64 * 256 + i64::from(offset);
        [at(i % width, offset[0]), at(i / width, offset[1])]
    };
    (0..candidates.len())
        .map(|i| {
            let here = candidates[i]?;
            let (x, y) = (i % width, i / width);
            let rows = y.saturating_sub(reach)..(y + reach + 1).min(height);
            let columns = x.saturating_sub(reach)..(x + reach + 1).min(width);
            let outranked = rows
                .flat_map(|row| columns.clone().map(move |column| row * width + column))
                // The candidate itself is among them, too close at any spacing above 0.00,
                // and never outranks itself.
                .filter_map(|j| Some((j, candidates[j]?)))
                .any(|(j, there)| {
                    let apart = spacing.of_kind(here.kind).max(spacing.of_kind(there.kind));
                    too_close(point(i, here.offset), point(j, there.offset), apart)
                        && (there.priority, std::cmp::Reverse(j))
                            > (here.priority, std::cmp::Reverse(i))
                });
            (!outranked).then_some(Plant {
                kind: here.kind,
                offset: here.offset,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hundredths(tree: i64, shrub: i64, grass: i64) -> PerKind {
        PerKind {
            tree: Fixed::from_hundredths(tree),
            shrub: Fixed::from_hundredths(shrub),
            grass: Fixed::from_hundredths(grass),
        }
    }

    /// The smallest low 32 bits whose roll is `roll`.
    fn rolling(roll: u64) -> u64 {
        (roll << 32).div_ceil(10_000)
    }

    fn kind(roll: u64, chances: PerKind, scale: i64) -> Option<PlantKind> {
        candidate(rolling(roll), chances, Fixed::from_hundredths(scale)).map(|c| c.kind)
    }

    #[test]
    fn a_tiles_hash_folds_its_position_into_the_seed() {
        // From foliage_ref.py's SplitMix64, written apart from the code.
        assert_eq!(hash(7, [3, 4]), 0xbb0f_e2d8_6e38_f14c);
        assert_eq!(hash(0, [0, 0]), 0x831a_81bd_9331_5937);
    }

    #[test]
    fn the_roll_picks_a_tree_then_a_shrub_then_grass_by_the_biomes_chances() {
        let forest = hundredths(40, 15, 10);
        let kinds = [0, 3999, 4000, 5499, 5500, 6499, 6500, 9999].map(|r| kind(r, forest, 100));
        use PlantKind::*;
        assert_eq!(
            kinds,
            [
                Some(Tree),
                Some(Tree),
                Some(Shrub),
                Some(Shrub),
                Some(Grass),
                Some(Grass),
                None,
                None
            ]
        );
        // With no chance of a tree, a roll of 0 is the next kind's.
        assert_eq!(kind(0, hundredths(0, 0, 50), 100), Some(Grass));
        assert_eq!(kind(0, hundredths(0, 0, 0), 100), None);
    }

    #[test]
    fn the_scale_multiplies_each_chance_and_the_sum_stops_at_certainty() {
        let forest = hundredths(40, 15, 10);
        use PlantKind::*;
        // At 2.00: a tree under 8000, a shrub under 10000 (8000 + 3000, capped), grass never.
        assert_eq!(kind(7999, forest, 200), Some(Tree));
        assert_eq!(kind(8000, forest, 200), Some(Shrub));
        assert_eq!(kind(9999, forest, 200), Some(Shrub));
        // At 0.50: 2000, 2750 and 3250.
        assert_eq!(kind(1999, forest, 50), Some(Tree));
        assert_eq!(kind(2749, forest, 50), Some(Shrub));
        assert_eq!(kind(3249, forest, 50), Some(Grass));
        assert_eq!(kind(3250, forest, 50), None);
        // At 0.00, nothing.
        assert_eq!(kind(0, forest, 0), None);
    }

    #[test]
    fn the_offset_is_the_hashs_next_two_bytes_and_the_priority_it_mixed() {
        let certain = hundredths(100, 0, 0);
        let c = candidate(0x0000_ab12_0000_0000, certain, Fixed::ONE).expect("a tree");
        assert_eq!(c.offset, [0x12, 0xab]);
        let c = candidate(0xbb0f_e2d8_6e38_f14c, certain, Fixed::ONE).expect("a tree");
        assert_eq!(c.offset, [0xd8, 0xe2]);
        assert_eq!(c.priority, 0x0503_cedc_8e2f_e405);
    }

    #[test]
    fn too_close_is_nearer_than_the_spacing_exactly() {
        let two = Fixed::from_hundredths(2_00);
        assert!(too_close([0, 0], [511, 0], two));
        assert!(!too_close([0, 0], [512, 0], two));
        assert!(too_close([0, 0], [0, -511], two));
        assert!(!too_close([0, 0], [0, -512], two));
        // 362² × 2 is 262,088, under 512²'s 262,144; 362² + 363² is 262,813, over it.
        assert!(too_close([10, 10], [372, 372], two));
        assert!(!too_close([10, 10], [373, 372], two));
        let one_and_a_half = Fixed::from_hundredths(1_50);
        assert!(too_close([0, 0], [383, 0], one_and_a_half));
        assert!(!too_close([0, 0], [384, 0], one_and_a_half));
        // At 0.00 nothing is too close, not even the same point.
        assert!(!too_close([5, 5], [5, 5], Fixed::ZERO));
    }

    fn at(kind: PlantKind, offset: [u8; 2], priority: u64) -> Option<Candidate> {
        Some(Candidate {
            kind,
            offset,
            priority,
        })
    }

    /// The kinds that stand, row by row, `.` for none.
    fn standing(width: usize, candidates: &[Option<Candidate>], spacing: PerKind) -> String {
        plants(width, candidates, spacing)
            .iter()
            .map(|plant| match plant.map(|p| p.kind) {
                Some(PlantKind::Tree) => 't',
                Some(PlantKind::Shrub) => 's',
                Some(PlantKind::Grass) => 'g',
                None => '.',
            })
            .collect()
    }

    const DEFAULT: PerKind = PerKind {
        tree: Fixed::from_hundredths(2_00),
        shrub: Fixed::from_hundredths(1_50),
        grass: Fixed::from_hundredths(1_00),
    };

    #[test]
    fn a_candidate_too_close_to_a_higher_priority_falls() {
        use PlantKind::*;
        // Grass a tile apart exactly both stand; 128 256ths apart, the higher priority alone.
        assert_eq!(
            standing(2, &[at(Grass, [0, 0], 1), at(Grass, [0, 0], 2)], DEFAULT),
            "gg"
        );
        assert_eq!(
            standing(2, &[at(Grass, [128, 0], 1), at(Grass, [0, 0], 2)], DEFAULT),
            ".g"
        );
        assert_eq!(
            standing(2, &[at(Grass, [128, 0], 2), at(Grass, [0, 0], 1)], DEFAULT),
            "g."
        );
        // A tile with no candidate is no plant, and blocks nothing.
        assert_eq!(
            standing(
                3,
                &[at(Grass, [0, 0], 1), None, at(Grass, [0, 0], 2)],
                DEFAULT
            ),
            "g.g"
        );
    }

    #[test]
    fn the_larger_of_two_kinds_spacings_applies() {
        use PlantKind::*;
        // 300 256ths apart: too close for a tree (512), not for two grasses (256).
        let pair =
            |first, second| standing(2, &[at(first, [0, 0], 1), at(second, [44, 0], 2)], DEFAULT);
        assert_eq!(pair(Grass, Grass), "gg");
        assert_eq!(pair(Tree, Grass), ".g");
        assert_eq!(pair(Grass, Tree), ".t");
        // Shrubs, 1.50 or 384 apart: 300 is too close, and 400 isn't.
        assert_eq!(pair(Shrub, Grass), ".g");
        assert_eq!(
            standing(2, &[at(Shrub, [0, 0], 1), at(Shrub, [144, 0], 2)], DEFAULT),
            "ss"
        );
    }

    #[test]
    fn a_fallen_candidate_still_blocks_those_it_outranks() {
        use PlantKind::*;
        // Trees at 0, 300 and 600: each too close to the next, not to the one beyond.
        let row = |priorities: [u64; 3]| {
            standing(
                3,
                &[
                    at(Tree, [0, 0], priorities[0]),
                    at(Tree, [44, 0], priorities[1]),
                    at(Tree, [88, 0], priorities[2]),
                ],
                DEFAULT,
            )
        };
        assert_eq!(row([1, 3, 2]), ".t.");
        assert_eq!(row([3, 1, 2]), "t.t");
        // The middle falls to the first, and the last to the middle all the same.
        assert_eq!(row([3, 2, 1]), "t..");
    }

    #[test]
    fn a_tie_goes_to_the_earlier_tile_in_reading_order() {
        use PlantKind::*;
        assert_eq!(
            standing(2, &[at(Grass, [200, 0], 5), at(Grass, [0, 0], 5)], DEFAULT),
            "g."
        );
        // A tile in the row above is earlier, though it lies to the right.
        let grid = [None, at(Grass, [0, 200], 5), at(Grass, [200, 0], 5), None];
        assert_eq!(standing(2, &grid, DEFAULT), ".g..");
    }

    #[test]
    fn candidates_are_seen_as_far_as_the_widest_spacing_reaches() {
        use PlantKind::*;
        // Down a column: 2 rows apart, 257 256ths, too close for trees (512).
        let column = [at(Tree, [0, 255], 1), None, at(Tree, [0, 0], 2)];
        assert_eq!(standing(1, &column, DEFAULT), "..t");
        // 3 rows apart, 513 256ths, too close at 2.50 (640), which reaches 3 tiles.
        let wide = PerKind {
            tree: Fixed::from_hundredths(2_50),
            ..DEFAULT
        };
        let column = [at(Tree, [0, 255], 1), None, None, at(Tree, [0, 0], 2)];
        assert_eq!(standing(1, &column, wide), "...t");
        assert_eq!(standing(1, &column, DEFAULT), "t..t");
        // And across a row, 257 apart.
        assert_eq!(
            standing(
                3,
                &[at(Tree, [255, 0], 1), None, at(Tree, [0, 0], 2)],
                DEFAULT
            ),
            "..t"
        );
        // Two columns right and a row up, at (512, 255) from (255, 256); and two left and a
        // row up, at (255, 255) from (512, 256).
        let mut grid = [None; 9];
        grid[2] = at(Tree, [0, 255], 2);
        grid[3] = at(Tree, [255, 0], 1);
        assert_eq!(standing(3, &grid, DEFAULT), "..t......");
        let mut grid = [None; 9];
        grid[0] = at(Tree, [255, 255], 2);
        grid[5] = at(Tree, [0, 0], 1);
        assert_eq!(standing(3, &grid, DEFAULT), "t........");
    }

    #[test]
    fn a_plant_keeps_its_candidates_kind_and_offset() {
        let plants = plants(1, &[at(PlantKind::Shrub, [17, 230], 9)], DEFAULT);
        assert_eq!(
            plants,
            [Some(Plant {
                kind: PlantKind::Shrub,
                offset: [17, 230]
            })]
        );
    }
}
