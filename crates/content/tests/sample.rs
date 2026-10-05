//! Loads the real sample world, `content/sample` (Riverhold, DESIGN.md §13), from disk.

use std::path::{Path, PathBuf};

use factional_content::load_dir;
use factional_core::Fixed;
use factional_reputation::{
    ActionId, CharacterId, Command, FactionId, Metric, Observer, OutcomeId, Party, WeightsFrom,
    Witnesses, World,
};

fn sample_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/sample")
}

fn riverhold() -> World {
    World::new(load_dir(&sample_dir()).expect("the sample content is valid"))
        .expect("the sample content makes a world")
}

fn character(id: &str) -> CharacterId {
    CharacterId::new(id).expect("valid id")
}

fn faction(id: &str) -> Observer {
    Observer::Faction(FactionId::new(id).expect("valid id"))
}

/// A distance as the CLI shows it.
fn distance(world: &World, observer: &Observer, subject: &str) -> String {
    world
        .distance(observer, &character(subject))
        .expect("both exist")
        .value
        .to_string()
}

#[test]
fn loads_riverholds_factions() {
    let world = riverhold();
    let factions: Vec<(&str, &str)> = world
        .factions()
        .map(|faction| (faction.id.as_str(), faction.name.as_str()))
        .collect();
    assert_eq!(
        factions,
        [
            ("ashen_circle", "The Ashen Circle"),
            ("city_watch", "The City Watch"),
            ("free_company", "The Free Company"),
            ("lantern_guild", "The Lantern Guild"),
            ("temple", "Temple of the Dawn"),
        ]
    );
    assert_eq!(world.balance().metric, Metric::Euclidean);
}

#[test]
fn riverholds_factions_measure_distance_with_their_weights() {
    let world = riverhold();
    assert_eq!(distance(&world, &faction("city_watch"), "player"), "70.18");
    assert_eq!(distance(&world, &faction("temple"), "sister_mira"), "5.59");
}

#[test]
fn a_thief_comes_closer_to_the_lantern_guild() {
    let mut world = riverhold();
    let steal = || Command::PerformAction {
        actor: character("player"),
        action: ActionId::new("steal").expect("valid id"),
        target: None,
        scale: Fixed::ONE,
        witnesses: Witnesses::Everyone,
    };
    for _ in 0..2 {
        world.execute(steal()).expect("accepted");
    }
    assert_eq!(
        distance(&world, &faction("lantern_guild"), "player"),
        "50.04"
    );
    for _ in 0..2 {
        world.execute(steal()).expect("accepted");
    }
    assert_eq!(
        distance(&world, &faction("lantern_guild"), "player"),
        "40.01"
    );
}

#[test]
fn characters_use_their_own_weights_or_the_default() {
    let world = riverhold();
    let hale = Observer::Character(character("captain_hale"));
    let measured = world
        .distance(&hale, &character("player"))
        .expect("both exist");
    assert_eq!(
        (measured.value.to_string(), measured.weights_from),
        ("75.37".to_owned(), WeightsFrom::Own)
    );
    let ava = Observer::Character(character("merchant_ava"));
    let measured = world
        .distance(&ava, &character("player"))
        .expect("both exist");
    assert_eq!(
        (measured.value.to_string(), measured.weights_from),
        ("22.36".to_owned(), WeightsFrom::Default)
    );
}

#[test]
fn loads_riverholds_characters_with_their_labels() {
    let world = riverhold();
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
    let mut world = riverhold();
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

/// A disposition as the CLI shows it: `-3.64 (neutral)`.
fn disposition(world: &World, observer: &Observer, subject: &str) -> String {
    let regard = world
        .disposition(observer, &character(subject))
        .expect("both exist");
    format!("{} ({})", regard.score, regard.band)
}

#[test]
fn riverholds_factions_regard_people_by_how_close_they_are() {
    let world = riverhold();
    assert_eq!(
        disposition(&world, &faction("city_watch"), "player"),
        "-3.64 (neutral)"
    );
    assert_eq!(
        disposition(&world, &faction("temple"), "sister_mira"),
        "45.34 (friendly)"
    );
    assert_eq!(
        disposition(&world, &faction("temple"), "brother_ash"),
        "-32.15 (unfriendly)"
    );
    assert_eq!(
        disposition(&world, &faction("city_watch"), "vex"),
        "-23.36 (neutral)"
    );
}

#[test]
fn riverholds_people_start_in_their_factions_without_warnings() {
    let content = load_dir(&sample_dir()).expect("the sample content is valid");
    assert!(factional_content::warnings(&content).is_empty());
    let world = World::new(content).expect("the sample content makes a world");
    let members = |faction: &str| -> Vec<String> {
        world
            .members(&FactionId::new(faction).expect("valid id"))
            .expect("the faction exists")
            .into_iter()
            .map(ToString::to_string)
            .collect()
    };
    assert_eq!(members("city_watch"), ["captain_hale"]);
    assert_eq!(members("temple"), ["sister_mira"]);
    assert_eq!(members("lantern_guild"), ["vex"]);
    assert_eq!(members("ashen_circle"), ["brother_ash"]);
    assert!(members("free_company").is_empty());
}

#[test]
fn the_player_must_turn_thief_to_join_the_lantern_guild() {
    let mut world = riverhold();
    let guild = FactionId::new("lantern_guild").expect("valid id");
    let join = || Command::JoinFaction {
        character: character("player"),
        faction: guild.clone(),
    };
    let steal = || Command::PerformAction {
        actor: character("player"),
        action: ActionId::new("steal").expect("valid id"),
        target: None,
        scale: Fixed::ONE,
        witnesses: Witnesses::Everyone,
    };
    let refusal = world.execute(join()).expect_err("too far at the start");
    assert_eq!(
        refusal.to_string(),
        "player can't join lantern_guild: 60.21 from The Lantern Guild, tolerance is 45.00"
    );
    for _ in 0..2 {
        world.execute(steal()).expect("accepted");
    }
    assert_eq!(
        world
            .execute(join())
            .expect_err("still too far")
            .to_string(),
        "player can't join lantern_guild: 50.04 from The Lantern Guild, tolerance is 45.00"
    );
    for _ in 0..2 {
        world.execute(steal()).expect("accepted");
    }
    world.execute(join()).expect("40.01 is within 45.00");
    assert_eq!(
        world.members(&guild).expect("the faction exists"),
        [&character("player"), &character("vex")]
    );
}

fn faction_id(id: &str) -> FactionId {
    FactionId::new(id).expect("valid id")
}

#[test]
fn riverholds_factions_regard_each_other_as_written() {
    let world = riverhold();
    let regard = |from: &str, to: &str| {
        let regard = world
            .relation(&faction_id(from), &faction_id(to))
            .expect("both exist");
        format!("{} ({})", regard.value, regard.band)
    };
    assert_eq!(regard("city_watch", "lantern_guild"), "-80.00 (enemy)");
    assert_eq!(regard("city_watch", "free_company"), "-30.00 (rival)");
    assert_eq!(regard("free_company", "city_watch"), "-10.00 (neutral)");
    assert_eq!(regard("city_watch", "ashen_circle"), "-40.00 (rival)");
    let conflict = |a: &str, b: &str| {
        world
            .in_conflict(&faction_id(a), &faction_id(b))
            .expect("both exist")
    };
    assert!(conflict("city_watch", "lantern_guild"));
    assert!(conflict("temple", "ashen_circle"));
    assert!(!conflict("city_watch", "free_company"));
    assert!(!conflict("city_watch", "ashen_circle"));
}

#[test]
fn a_guild_thief_is_turned_away_by_the_watch_on_both_counts() {
    let mut world = riverhold();
    for _ in 0..4 {
        world
            .execute(Command::PerformAction {
                actor: character("player"),
                action: ActionId::new("steal").expect("valid id"),
                target: None,
                scale: Fixed::ONE,
                witnesses: Witnesses::Everyone,
            })
            .expect("accepted");
    }
    world
        .execute(Command::JoinFaction {
            character: character("player"),
            faction: faction_id("lantern_guild"),
        })
        .expect("40.01 is within 45.00");
    let refusal = world
        .execute(Command::JoinFaction {
            character: character("player"),
            faction: faction_id("city_watch"),
        })
        .expect_err("too far, and an enemy");
    assert_eq!(
        refusal.to_string(),
        "player can't join city_watch: 90.35 from The City Watch, tolerance is 40.00; \
         player belongs to The Lantern Guild, in conflict with The City Watch (-80.00)"
    );
}

fn standing(world: &World, subject: &str, party: Party) -> String {
    world
        .standing(&character(subject), &party)
        .expect("both exist")
        .to_string()
}

fn act(world: &mut World, action: &str, target: Option<&str>) {
    world
        .execute(Command::PerformAction {
            actor: character("player"),
            action: ActionId::new(action).expect("valid id"),
            target: target.map(character),
            scale: Fixed::ONE,
            witnesses: Witnesses::Everyone,
        })
        .expect("accepted");
}

#[test]
fn riverholds_acts_and_outcomes_change_standing() {
    let mut world = riverhold();
    act(&mut world, "steal", Some("merchant_ava"));
    assert_eq!(
        standing(
            &world,
            "player",
            Party::Character(character("merchant_ava"))
        ),
        "-20.00"
    );
    act(&mut world, "steal", Some("vex"));
    assert_eq!(
        standing(&world, "player", Party::Character(character("vex"))),
        "-20.00"
    );
    assert_eq!(
        standing(
            &world,
            "player",
            Party::Faction(faction_id("lantern_guild"))
        ),
        "-10.00"
    );
    world
        .execute(Command::ApplyOutcome {
            outcome: OutcomeId::new("fined_by_watch").expect("valid id"),
            character: character("player"),
        })
        .expect("accepted");
    assert_eq!(
        standing(&world, "player", Party::Faction(faction_id("city_watch"))),
        "-20.00"
    );
    assert_eq!(
        standing(
            &world,
            "player",
            Party::Character(character("captain_hale"))
        ),
        "-10.00"
    );
    act(&mut world, "donate_to_temple", None);
    assert_eq!(
        standing(&world, "player", Party::Faction(faction_id("temple"))),
        "10.00"
    );
}

#[test]
fn riverholds_people_start_with_standing_in_their_factions() {
    let world = riverhold();
    assert_eq!(
        standing(
            &world,
            "captain_hale",
            Party::Faction(faction_id("city_watch"))
        ),
        "75.00"
    );
    assert_eq!(
        standing(&world, "sister_mira", Party::Faction(faction_id("temple"))),
        "40.00"
    );
    assert_eq!(
        standing(&world, "vex", Party::Faction(faction_id("lantern_guild"))),
        "30.00"
    );
}
