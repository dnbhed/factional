//! Levels (W3c, P-138): elevation quantised for drawing in 2D, with plateaus and pits too
//! small to keep taking the nearest level around them.

/// `levels`, `width` a row, each a land tile's level or none for water, with every plateau and
/// pit cleaned: each 4-connected group of land at one level, of fewer than `specks` tiles and
/// not touching the edge, whose land neighbours are all lower takes the highest of them, and
/// whose land neighbours are all higher takes the lowest. Every group is judged by the levels
/// as given, before any is cleaned.
pub(crate) fn clean(width: usize, levels: &mut [Option<u8>], specks: usize) {
    let height = levels.len() / width.max(1);
    let edge = |i: usize| {
        let (x, y) = (i % width, i / width);
        x == 0 || x == width - 1 || y == 0 || y == height - 1
    };
    let mut cleaned = Vec::new();
    for group in groups(width, levels) {
        if group.len() >= specks || group.iter().any(|&i| edge(i)) {
            continue;
        }
        // Its land neighbours: any at its own level would be in the group.
        let level = levels[group[0]];
        let around: Vec<u8> = group
            .iter()
            .flat_map(|&i| [i - width, i - 1, i + 1, i + width])
            .filter_map(|j| levels[j].filter(|&l| Some(l) != level))
            .collect();
        let nearest = match (around.iter().min(), around.iter().max()) {
            (_, Some(&highest)) if Some(highest) < level => highest,
            (Some(&lowest), _) if Some(lowest) > level => lowest,
            _ => continue,
        };
        cleaned.extend(group.into_iter().map(|i| (i, nearest)));
    }
    for (i, level) in cleaned {
        levels[i] = Some(level);
    }
}

/// The 4-connected groups of land in `levels` at one level, each from its first tile row by
/// row: since the tiles are searched in that order, the one each group is found from.
fn groups(width: usize, levels: &[Option<u8>]) -> Vec<Vec<usize>> {
    let mut seen = vec![false; levels.len()];
    let mut groups = Vec::new();
    for first in 0..levels.len() {
        if seen[first] || levels[first].is_none() {
            continue;
        }
        seen[first] = true;
        let mut group = vec![first];
        // A tile joins a group once, so a group's tiles are reached in as many steps as there
        // are tiles.
        for step in 0..levels.len() {
            let Some(&i) = group.get(step) else {
                break;
            };
            let (x, y) = (i % width, i / width);
            let around = [
                (y > 0).then(|| i - width),
                (x > 0).then(|| i - 1),
                (x + 1 < width).then(|| i + 1),
                (i + width < levels.len()).then(|| i + width),
            ];
            for j in around.into_iter().flatten() {
                if !seen[j] && levels[j] == levels[first] {
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

    /// `rows`, drawn a row a string, `~` water and a digit a level, cleaned of plateaus and
    /// pits of fewer than `specks` tiles, drawn again.
    fn cleaned(rows: &[&str], specks: usize) -> Vec<String> {
        let width = rows[0].len();
        let mut levels: Vec<Option<u8>> = rows
            .iter()
            .flat_map(|row| row.chars().map(|c| c.to_digit(10).map(|l| l as u8)))
            .collect();
        clean(width, &mut levels, specks);
        levels
            .chunks(width)
            .map(|row| {
                row.iter()
                    .map(|level| {
                        level.map_or('~', |l| {
                            char::from_digit(u32::from(l), 10).expect("a digit")
                        })
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn a_plateau_takes_the_highest_level_around_it() {
        // The 3 is bordered by 1s and a 2; the 2s touch the edge.
        assert_eq!(
            cleaned(&["1111", "1322", "1111"], 4),
            ["1111", "1222", "1111"]
        );
    }

    #[test]
    fn a_pit_takes_the_lowest_level_around_it() {
        assert_eq!(
            cleaned(&["5555", "5266", "5555"], 4),
            ["5555", "5566", "5555"]
        );
    }

    #[test]
    fn a_group_with_land_above_and_below_is_kept() {
        // A staircase: the 2 and the 3 each have a higher and a lower neighbour; the 4 is a
        // plateau.
        assert_eq!(
            cleaned(&["11111", "12341", "11111"], 4),
            ["11111", "12331", "11111"]
        );
    }

    #[test]
    fn water_is_no_neighbour() {
        // The 3 has only water around it; the 4 has water and lower land.
        assert_eq!(
            cleaned(&["~~~~~", "~~3~~", "~~~~~"], 4),
            ["~~~~~", "~~3~~", "~~~~~"]
        );
        assert_eq!(
            cleaned(&["1111", "~421", "1111"], 4),
            ["1111", "~221", "1111"]
        );
    }

    #[test]
    fn a_group_touching_the_edge_is_kept() {
        assert_eq!(
            cleaned(&["1311", "1111", "1131", "1111"], 4),
            ["1311", "1111", "1111", "1111"]
        );
        assert_eq!(cleaned(&["111", "111", "131"], 4), ["111", "111", "131"]);
        assert_eq!(cleaned(&["111", "311", "111"], 4), ["111", "311", "111"]);
        assert_eq!(cleaned(&["111", "113", "111"], 4), ["111", "113", "111"]);
    }

    #[test]
    fn a_group_of_the_specks_size_is_kept() {
        let rows = ["11111", "13311", "13111", "11111"];
        assert_eq!(cleaned(&rows, 3), rows);
        assert_eq!(cleaned(&rows, 4), ["11111"; 4]);
    }

    #[test]
    fn every_group_is_judged_by_the_levels_as_given() {
        // The 3 is a plateau and becomes 2; the 2 beside it had a 3 above it, so it stays,
        // though the two together are then a plateau of 2 over 1.
        assert_eq!(
            cleaned(&["11111", "13211", "11111"], 4),
            ["11111", "12211", "11111"]
        );
    }

    #[test]
    fn no_specks_cleans_nothing() {
        let rows = ["111", "131", "111"];
        for specks in [0, 1] {
            assert_eq!(cleaned(&rows, specks), rows);
        }
        assert_eq!(cleaned(&rows, 2), ["111"; 3]);
    }
}
