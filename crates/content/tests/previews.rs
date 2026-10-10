//! Previews of the sample world (U5): the disposition matrix and the City Watch's alignment
//! map, as the CLI's `matrix` and `map` and the editor's Previews tab show them. Values are
//! worked by hand from DESIGN.md §6 and §8.

use std::path::Path;

use factional_content::load_dir;
use factional_core::Fixed;
use factional_reputation::{CharacterId, FactionId, MapCell, Observer, World};

fn riverhold() -> World {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/sample");
    World::new(load_dir(&dir).expect("the sample loads")).expect("the sample makes a world")
}

fn character(id: &str) -> CharacterId {
    CharacterId::new(id).expect("valid id")
}

fn fixed(text: &str) -> Fixed {
    text.parse().expect("a number")
}

#[test]
fn the_matrix_has_every_observer_factions_first_toward_each_character() {
    let world = riverhold();
    let subjects: Vec<CharacterId> = world.characters().map(|c| c.id.clone()).collect();
    let matrix = world.disposition_matrix(&subjects).expect("all known");
    assert_eq!(matrix.subjects, subjects);
    let observers: Vec<String> = matrix
        .rows
        .iter()
        .map(|row| row.observer.to_string())
        .collect();
    assert_eq!(
        observers,
        [
            "ashen_circle",
            "city_watch",
            "free_company",
            "lantern_guild",
            "temple",
            "brother_ash",
            "captain_hale",
            "merchant_ava",
            "player",
            "sister_mira",
            "vex",
        ]
    );
    let hale = &matrix.rows[6];
    assert_eq!(
        hale.observer,
        Observer::Character(character("captain_hale"))
    );
    // Nothing toward himself.
    assert_eq!(hale.cells[1], None);
    // Toward the player: 75.37 apart, so affinity (75.37 − 60) × −50 / 140 = −5.49; nothing
    // else counts at the start.
    let player = hale.cells[3].as_ref().expect("toward the player");
    assert_eq!(player.distance.value, fixed("75.37"));
    assert_eq!(player.score, fixed("-5.49"));
    assert_eq!(player.band, "neutral");
    assert!(hale.cells.iter().filter(|cell| cell.is_none()).count() == 1);
    assert!(matrix.rows[0].cells.iter().all(Option::is_some));
}

#[test]
fn a_matrix_of_chosen_subjects_has_just_those_and_refuses_unknown_ones() {
    let world = riverhold();
    let matrix = world
        .disposition_matrix(&[character("vex")])
        .expect("vex is known");
    assert_eq!(matrix.subjects, [character("vex")]);
    assert!(matrix.rows.iter().all(|row| row.cells.len() == 1));
    assert_eq!(matrix.rows[10].cells[0], None);
    assert!(world.disposition_matrix(&[character("nobody")]).is_none());
}

#[test]
fn the_watchs_map_marks_its_tolerance_and_where_it_pictures_everyone() {
    let world = riverhold();
    let watch = FactionId::new("city_watch").expect("valid id");
    let map = world.alignment_map(&watch).expect("the Watch");
    assert_eq!(map.tolerance, fixed("40"));
    assert_eq!(map.within.len(), 21);
    assert!(map.within.iter().all(|row| row.len() == 21));
    // The Watch stands at law 70, good 20.
    assert_eq!(map.cell, MapCell { row: 8, column: 17 });
    assert_eq!((map.cell.law(), map.cell.good()), (70, 20));
    // At good 100, law 40 is √(30² + (80 × 0.25)²) = 36.06 away, within; law 30 is 44.72.
    assert!(map.within[0][14]);
    assert!(!map.within[0][13]);
    let letters: String = map.characters.iter().map(|mark| mark.letter).collect();
    assert_eq!(letters, "ABCDEF");
    let mark = |id: &str| {
        map.characters
            .iter()
            .find(|mark| mark.character.as_str() == id)
            .expect("on the map")
    };
    // Hale, at 75 / 30, falls in the cell at law 80, good 30: √(5² + 2.5²) = 5.59 away.
    let hale = mark("captain_hale");
    assert_eq!((hale.cell.law(), hale.cell.good()), (80, 30));
    assert_eq!(hale.distance.value, fixed("5.59"));
    assert!(hale.within);
    // Vex, at −55 / −20, in the cell at law −60, good −20: √(125² + 10²) = 125.40 away.
    let vex = mark("vex");
    assert_eq!(vex.cell, MapCell { row: 12, column: 4 });
    assert_eq!(vex.distance.value, fixed("125.40"));
    assert!(!vex.within);
    assert!(mark("sister_mira").within);
    assert_eq!(mark("sister_mira").distance.value, fixed("38.59"));
    let marks = map.marks();
    assert_eq!(marks.get(&map.cell), Some(&vec!['@']));
    assert_eq!(marks.get(&hale.cell), Some(&vec!['B']));
    assert_eq!(marks.len(), 7);
    let nowhere = FactionId::new("nowhere").expect("valid id");
    assert!(world.alignment_map(&nowhere).is_none());
}
