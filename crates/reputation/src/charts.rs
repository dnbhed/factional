//! Pictures of a world for tuning (DESIGN.md §12.3, U5): the alignment plane as a faction
//! sees it, and how every observer regards each character. They're queries, so the CLI's
//! `map` and `matrix` and the editor's Previews tab show the same thing; neither works
//! anything out itself.

use std::collections::BTreeMap;

use factional_core::Fixed;

use crate::{Alignment, Axis, CharacterId, Disposition, Distance, FactionId, Observer, World};

/// The map's cells along each axis: 10 apart, from −100 to 100.
pub const MAP_CELLS: usize = 21;

/// Cells either side of the middle one, at 0.
const HALF: i64 = 10;
/// How far apart the cells' centres are, in whole points.
const SPACING: i64 = 10;

/// Which cell along an axis a value falls in, from 0 at −100 to 20 at 100: the nearest 10,
/// halves away from zero.
fn cell_along(value: Fixed) -> usize {
    let hundredths = value.hundredths();
    let tens = (hundredths.abs() + 50 * SPACING) / (100 * SPACING);
    usize::try_from(hundredths.signum() * tens + HALF).expect("alignments stay on the plane")
}

/// A cell's centre on an axis, such as −100 for cell 0.
fn centre(index: usize) -> i64 {
    (i64::try_from(index).expect("a map cell") - HALF) * SPACING
}

/// The cell an alignment falls in.
fn cell_of(alignment: Alignment) -> MapCell {
    MapCell {
        row: MAP_CELLS - 1 - cell_along(alignment.on(Axis::Good)),
        column: cell_along(alignment.on(Axis::Law)),
    }
}

/// The letter for the `index`th character: `A` to `Z`, then `a` to `z`, then `?`.
fn letter(index: usize) -> char {
    ('A'..='Z').chain('a'..='z').nth(index).unwrap_or('?')
}

/// The alignment plane as a faction sees it: which cells are within its tolerance, where it
/// stands, and where it pictures each character (DESIGN.md §10.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlignmentMap {
    pub faction: FactionId,
    pub at: Alignment,
    pub tolerance: Fixed,
    /// Row by row from good 100 down to good −100, each from law −100 to law 100: whether
    /// the cell's centre is within the faction's tolerance.
    pub within: Vec<Vec<bool>>,
    /// The faction's own cell.
    pub cell: MapCell,
    /// Each character, in id order.
    pub characters: Vec<MapMark>,
}

/// A cell of the map: its row from the top (good 100) and its column from the left (law
/// −100).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MapCell {
    pub row: usize,
    pub column: usize,
}

/// A character on a faction's map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapMark {
    pub character: CharacterId,
    /// `A` to `Z`, then `a` to `z`, then `?`, in id order.
    pub letter: char,
    /// Where the faction pictures them.
    pub cell: MapCell,
    /// How far, with where it's measured to and where they truly are.
    pub distance: Distance,
    pub within: bool,
}

/// Every observer, factions first, toward each subject.
#[derive(Debug, Clone, PartialEq)]
pub struct DispositionMatrix {
    pub subjects: Vec<CharacterId>,
    pub rows: Vec<MatrixRow>,
}

/// One observer's dispositions toward each subject, in order; nothing toward themself.
#[derive(Debug, Clone, PartialEq)]
pub struct MatrixRow {
    pub observer: Observer,
    pub cells: Vec<Option<Disposition>>,
}

impl MapCell {
    /// Its centre on the law axis, such as −100 for column 0.
    pub fn law(&self) -> i64 {
        centre(self.column)
    }

    /// Its centre on the good axis, such as 100 for row 0.
    pub fn good(&self) -> i64 {
        -centre(self.row)
    }
}

impl AlignmentMap {
    /// Every mark in each cell that has any: `@` for the faction, then each character's
    /// letter.
    pub fn marks(&self) -> BTreeMap<MapCell, Vec<char>> {
        let mut marks: BTreeMap<MapCell, Vec<char>> = BTreeMap::new();
        marks.entry(self.cell).or_default().push('@');
        for mark in &self.characters {
            marks.entry(mark.cell).or_default().push(mark.letter);
        }
        marks
    }
}

impl World {
    /// The alignment plane as `faction` sees it; `None` for an unknown faction.
    pub fn alignment_map(&self, faction: &FactionId) -> Option<AlignmentMap> {
        let at = self.faction_alignment(faction)?;
        let tolerance = self.faction(faction)?.tolerances.tolerance();
        let within = (0..MAP_CELLS)
            .map(|row| {
                (0..MAP_CELLS)
                    .map(|column| {
                        let cell = MapCell { row, column };
                        let point = Alignment::new(
                            Fixed::from_hundredths(cell.law() * 100),
                            Fixed::from_hundredths(cell.good() * 100),
                        )
                        .expect("every cell is on the plane");
                        self.distance_to_point(faction, point)
                            .is_some_and(|distance| distance <= tolerance)
                    })
                    .collect()
            })
            .collect();
        let observer = Observer::Faction(faction.clone());
        let characters = self
            .characters()
            .enumerate()
            .map(|(index, character)| {
                let distance = self.distance(&observer, &character.id).expect("both exist");
                MapMark {
                    character: character.id.clone(),
                    letter: letter(index),
                    cell: cell_of(distance.subject),
                    within: distance.value <= tolerance,
                    distance,
                }
            })
            .collect();
        Some(AlignmentMap {
            faction: faction.clone(),
            at,
            tolerance,
            within,
            cell: cell_of(at),
            characters,
        })
    }

    /// Every observer, factions then characters in id order, toward each of `subjects`;
    /// `None` if a subject is unknown.
    pub fn disposition_matrix(&self, subjects: &[CharacterId]) -> Option<DispositionMatrix> {
        if !subjects
            .iter()
            .all(|subject| self.characters().any(|c| &c.id == subject))
        {
            return None;
        }
        let observers = self
            .factions()
            .map(|faction| Observer::Faction(faction.id.clone()))
            .chain(
                self.characters()
                    .map(|character| Observer::Character(character.id.clone())),
            );
        let rows = observers
            .map(|observer| {
                let cells = subjects
                    .iter()
                    .map(|subject| {
                        let themself = observer == Observer::Character(subject.clone());
                        if themself {
                            None
                        } else {
                            self.disposition(&observer, subject)
                        }
                    })
                    .collect();
                MatrixRow { observer, cells }
            })
            .collect();
        Some(DispositionMatrix {
            subjects: subjects.to_vec(),
            rows,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::tests::{character, h, world_of};

    fn riverhold() -> World {
        world_of([
            character("Vex", -55_00, -20_00),
            character("Ava", 20_00, 10_00),
            character("Player", 0, 0),
        ])
    }

    fn id(text: &str) -> CharacterId {
        CharacterId::new(text).expect("valid id")
    }

    fn watch() -> FactionId {
        FactionId::new("city_watch").expect("valid id")
    }

    #[test]
    fn a_value_falls_in_the_nearest_cell_halves_away_from_zero() {
        assert_eq!(cell_along(h(-100_00)), 0);
        assert_eq!(cell_along(h(-55_00)), 4);
        assert_eq!(cell_along(h(-45_00)), 5);
        assert_eq!(cell_along(h(-44_99)), 6);
        assert_eq!(cell_along(Fixed::ZERO), 10);
        assert_eq!(cell_along(h(44_99)), 14);
        assert_eq!(cell_along(h(45_00)), 15);
        assert_eq!(cell_along(h(100_00)), 20);
    }

    #[test]
    fn a_cell_knows_its_centre_on_each_axis() {
        let corner = MapCell { row: 0, column: 0 };
        assert_eq!((corner.law(), corner.good()), (-100, 100));
        let middle = MapCell {
            row: 10,
            column: 10,
        };
        assert_eq!((middle.law(), middle.good()), (0, 0));
        let far = MapCell {
            row: 20,
            column: 17,
        };
        assert_eq!((far.law(), far.good()), (70, -100));
    }

    #[test]
    fn characters_are_lettered_in_id_order() {
        assert_eq!(letter(0), 'A');
        assert_eq!(letter(25), 'Z');
        assert_eq!(letter(26), 'a');
        assert_eq!(letter(51), 'z');
        assert_eq!(letter(52), '?');
    }

    #[test]
    fn a_factions_map_marks_its_tolerance_and_everyone_where_it_pictures_them() {
        let world = riverhold();
        let map = world.alignment_map(&watch()).expect("the Watch");
        assert_eq!(
            map.at,
            Alignment::new(h(70_00), h(20_00)).expect("on the plane")
        );
        assert_eq!(map.tolerance, h(40_00));
        assert_eq!(map.cell, MapCell { row: 8, column: 17 });
        // At good 100: law 40 is √(30² + 20²) = 36.06 away, within; law 30, √(40² + 20²) =
        // 44.72, outside. At good −100, law 70 is 30 away, within; law 30, 50, outside.
        assert!(map.within[0][14]);
        assert!(!map.within[0][13]);
        assert!(map.within[20][17]);
        assert!(!map.within[20][13]);
        let marks: Vec<(char, &str, MapCell, Fixed, bool)> = map
            .characters
            .iter()
            .map(|m| {
                (
                    m.letter,
                    m.character.as_str(),
                    m.cell,
                    m.distance.value,
                    m.within,
                )
            })
            .collect();
        assert_eq!(
            marks,
            [
                // √(50² + 2.5²) = 50.06; √(70² + 5²) = 70.18; √(125² + 10²) = 125.40.
                ('A', "ava", MapCell { row: 9, column: 12 }, h(50_06), false),
                (
                    'B',
                    "player",
                    MapCell {
                        row: 10,
                        column: 10
                    },
                    h(70_18),
                    false
                ),
                ('C', "vex", MapCell { row: 12, column: 4 }, h(125_40), false),
            ]
        );
        let inside = world_of([character("Hale", 75_00, 30_00)]);
        let hale = &inside
            .alignment_map(&watch())
            .expect("the Watch")
            .characters[0];
        assert_eq!(hale.distance.value, h(5_59));
        assert!(hale.within);
        assert!(
            world
                .alignment_map(&FactionId::new("nowhere").expect("valid"))
                .is_none()
        );
    }

    #[test]
    fn a_cells_height_counts_for_a_faction_that_weighs_good() {
        // The Temple, at 30 / 80, weighs good fully and law by half, with tolerance 35: at
        // law 30, good 100 is 20 away, within; good 40 is 40 away, outside.
        let world = riverhold();
        let temple = FactionId::new("temple").expect("valid id");
        let map = world.alignment_map(&temple).expect("the Temple");
        assert!(map.within[0][13]);
        assert!(!map.within[6][13]);
    }

    #[test]
    fn marks_sharing_a_cell_are_kept_together() {
        let world = world_of([
            character("Vex", -55_00, -20_00),
            character("Twin", -56_00, -21_00),
            character("Player", 0, 0),
        ]);
        let map = world.alignment_map(&watch()).expect("the Watch");
        let marks = map.marks();
        assert_eq!(marks.get(&map.cell), Some(&vec!['@']));
        assert_eq!(
            marks.get(&MapCell { row: 12, column: 4 }),
            Some(&vec!['B', 'C'])
        );
        assert_eq!(
            marks.get(&MapCell {
                row: 10,
                column: 10
            }),
            Some(&vec!['A'])
        );
        assert_eq!(marks.len(), 3);
    }

    #[test]
    fn the_matrix_has_every_observer_factions_first_and_nothing_toward_themself() {
        let world = riverhold();
        let subjects = [id("ava"), id("vex")];
        let matrix = world.disposition_matrix(&subjects).expect("both known");
        assert_eq!(matrix.subjects, subjects);
        let observers: Vec<String> = matrix.rows.iter().map(|r| r.observer.to_string()).collect();
        assert_eq!(
            observers,
            [
                "city_watch",
                "free_company",
                "lantern_guild",
                "temple",
                "ava",
                "player",
                "vex"
            ]
        );
        for row in &matrix.rows {
            for (subject, cell) in subjects.iter().zip(&row.cells) {
                let expected = if row.observer == Observer::Character(subject.clone()) {
                    None
                } else {
                    world.disposition(&row.observer, subject)
                };
                assert_eq!(cell, &expected);
            }
        }
        assert_eq!(matrix.rows[4].cells[0], None);
        assert!(matrix.rows[4].cells[1].is_some());
        assert!(
            world
                .disposition_matrix(&[id("ava"), id("nobody")])
                .is_none()
        );
        assert!(
            world
                .disposition_matrix(&[])
                .is_some_and(|m| m.rows.len() == 7)
        );
    }
}
