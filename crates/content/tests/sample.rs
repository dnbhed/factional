//! Loads the real sample world, `content/sample` (Riverhold, DESIGN.md §13), from disk.

use std::path::{Path, PathBuf};

use factional_content::load_dir;
use factional_core::Fixed;
use factional_reputation::{ActionId, CharacterId, Command, Witnesses, World};

fn sample_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/sample")
}

#[test]
fn loads_riverholds_characters_with_their_labels() {
    let world = World::new(load_dir(&sample_dir()).expect("the sample content is valid"));
    let threshold = world.balance().label_threshold;
    let labels: Vec<(&str, &str)> = world
        .characters()
        .map(|character| (character.id.as_str(), character.alignment.label(threshold)))
        .collect();
    assert_eq!(
        labels,
        [
            ("brother_ash", "Neutral Evil"),
            ("captain_hale", "Lawful Neutral"),
            ("merchant_ava", "True Neutral"),
            ("player", "True Neutral"),
            ("sister_mira", "Lawful Good"),
            ("vex", "Chaotic Neutral"),
        ]
    );
}

#[test]
fn reports_a_directory_it_cannot_read() {
    let missing = sample_dir().join("no_such_world");
    let error = load_dir(&missing).expect_err("the directory doesn't exist");
    let message = error.to_string();
    assert!(
        message.starts_with(&format!(
            "{}: cannot read the directory: ",
            missing.display()
        )),
        "{message}"
    );
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn a_missing_file_means_none_of_that_kind() {
    let content = load_dir(&fixture("balance_only")).expect("valid content");
    assert!(content.characters.is_empty());
    assert_eq!(content.balance.label_threshold.to_string(), "50.00");
}

#[test]
fn reports_a_file_that_exists_but_cannot_be_read() {
    let error = load_dir(&fixture("unreadable")).expect_err("characters.toml is a directory");
    let message = error.to_string();
    assert!(
        message.starts_with("characters.toml: cannot read the file: "),
        "{message}"
    );
}

#[test]
fn loads_riverholds_action_catalogue() {
    let content = load_dir(&sample_dir()).expect("the sample content is valid");
    let actions: Vec<(&str, String, String)> = content
        .actions
        .values()
        .map(|action| {
            (
                action.id.as_str(),
                action.alignment.law.to_string(),
                action.alignment.good.to_string(),
            )
        })
        .collect();
    let expected = [
        ("donate_to_temple", "0.00", "3.00"),
        ("extort", "-2.00", "-6.00"),
        ("help_stranger", "0.00", "4.00"),
        ("murder", "-10.00", "-15.00"),
        ("report_crime", "4.00", "1.00"),
        ("steal", "-5.00", "-3.00"),
    ];
    let expected: Vec<(&str, String, String)> = expected
        .iter()
        .map(|&(id, law, good)| (id, law.to_owned(), good.to_owned()))
        .collect();
    assert_eq!(actions, expected);
}

#[test]
fn the_player_stealing_from_ava_moves_toward_chaotic_evil() {
    let mut world = World::new(load_dir(&sample_dir()).expect("the sample content is valid"));
    let player = CharacterId::new("player").expect("valid id");
    world
        .execute(Command::PerformAction {
            actor: player.clone(),
            action: ActionId::new("steal").expect("valid id"),
            target: Some(CharacterId::new("merchant_ava").expect("valid id")),
            scale: Fixed::ONE,
            witnesses: Witnesses::Everyone,
        })
        .expect("accepted");
    let alignment = world.alignment(&player).expect("the player exists");
    assert_eq!(
        (alignment.law().to_string(), alignment.good().to_string()),
        ("-5.00".to_owned(), "-3.00".to_owned())
    );
}
