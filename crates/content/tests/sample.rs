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
    // Affinity 45.34, standing 40.00, kinship 50.00 × 0.50 (Mira is in the Temple): 110.34,
    // clamped.
    assert_eq!(
        disposition(&world, &faction("temple"), "sister_mira"),
        "100.00 (friendly)"
    );
    // Affinity −32.15, kinship −90.00 × 0.50 (the Temple regards the Circle at −90).
    assert_eq!(
        disposition(&world, &faction("temple"), "brother_ash"),
        "-77.15 (unfriendly)"
    );
    // Affinity −23.36, kinship −80.00 × 0.50.
    assert_eq!(
        disposition(&world, &faction("city_watch"), "vex"),
        "-63.36 (unfriendly)"
    );
}

#[test]
fn hale_thinks_ill_of_a_fined_thief() {
    // DESIGN.md §8.2: two thefts from a merchant, then a fine from the Watch.
    let mut world = riverhold();
    act(&mut world, "steal", Some("merchant_ava"));
    act(&mut world, "steal", Some("merchant_ava"));
    world
        .execute(Command::ApplyOutcome {
            outcome: OutcomeId::new("fined_by_watch").expect("valid id"),
            character: character("player"),
            witnesses: Witnesses::Everyone,
        })
        .expect("accepted");
    let hale = Observer::Character(character("captain_hale"));
    assert_eq!(disposition(&world, &hale, "player"), "-29.10 (unfriendly)");
}

#[test]
fn hale_warms_to_a_priestess_of_an_allied_faith() {
    let world = riverhold();
    let hale = Observer::Character(character("captain_hale"));
    assert_eq!(
        disposition(&world, &hale, "sister_mira"),
        "44.75 (friendly)"
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
    // The Guild would let a cutpurse go, but the Watch's last defectors rule refuses: the
    // player has no standing with it, and is nearer the Guild (40.01) than the Watch.
    assert_eq!(
        refusal.to_string(),
        "player can't join city_watch: 90.35 from The City Watch, tolerance is 40.00; \
         player belongs to The Lantern Guild, in conflict with The City Watch (-80.00): \
         refused by The City Watch's defectors rule 3, \"You serve our enemies.\""
    );
}

#[test]
fn the_ashen_circle_lets_no_one_go() {
    let world = riverhold();
    let assessment = world
        .assess_join(&character("brother_ash"), &faction_id("temple"))
        .expect("both exist");
    // 150.02 from the Temple: gaps 5 and 150, weighted 2.5 and 150. 10.08 from the Circle:
    // gaps 5 and 10, weighted 1.25 and 10, so not closer to the Temple either.
    assert_eq!(
        assessment.reasons(),
        [
            "150.02 from Temple of the Dawn, tolerance is 35.00",
            "brother_ash belongs to The Ashen Circle, in conflict with Temple of the Dawn (-90.00): \
             refused by The Ashen Circle's deserters rule 1, \"No one leaves the Circle.\"",
            "brother_ash belongs to The Ashen Circle, in conflict with Temple of the Dawn (-90.00): \
             refused by Temple of the Dawn's defectors rule 3, \"You serve our enemies.\"",
        ]
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
            witnesses: Witnesses::Everyone,
        })
        .expect("accepted");
    // −20.00 from the fine, on top of +1.80 spilled from robbing a Guild member (P-46).
    assert_eq!(
        standing(&world, "player", Party::Faction(faction_id("city_watch"))),
        "-18.20"
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
    // +10.00, on top of −2.00 spilled from the fine: the Temple regards the Watch at +60.
    assert_eq!(
        standing(&world, "player", Party::Faction(faction_id("temple"))),
        "8.00"
    );
}

#[test]
fn riverholds_factions_share_in_standing_changes() {
    let mut world = riverhold();
    let with = |world: &World, faction: &str| {
        standing(world, "player", Party::Faction(faction_id(faction)))
    };
    // Robbing Vex: −10.00 with the Guild. The Watch regards it at −80: −0.18, so +1.80.
    act(&mut world, "steal", Some("vex"));
    assert_eq!(
        [
            "lantern_guild",
            "city_watch",
            "free_company",
            "temple",
            "ashen_circle"
        ]
        .map(|faction| with(&world, faction)),
        ["-10.00", "1.80", "0.00", "0.00", "0.00"]
    );
    // A donation: +10.00 with the Temple; the Watch (+60) shares 0.10, the Circle (−90) −0.24.
    act(&mut world, "donate_to_temple", None);
    assert_eq!(
        ["temple", "city_watch", "ashen_circle"].map(|faction| with(&world, faction)),
        ["10.00", "2.80", "-2.40"]
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

fn rank_of(world: &World, who: &str, faction: &str) -> String {
    world
        .memberships(&character(who))
        .expect("the character exists")
        .find(|(member_of, _)| member_of.as_str() == faction)
        .map(|(_, membership)| membership.rank.to_string())
        .expect("a member")
}

fn promote(who: &str, faction: &str) -> Command {
    Command::Promote {
        character: character(who),
        faction: faction_id(faction),
    }
}

#[test]
fn riverholds_people_start_at_their_ranks() {
    let world = riverhold();
    assert_eq!(rank_of(&world, "captain_hale", "city_watch"), "captain");
    assert_eq!(rank_of(&world, "sister_mira", "temple"), "ordained");
    assert_eq!(rank_of(&world, "vex", "lantern_guild"), "fence");
    assert_eq!(rank_of(&world, "brother_ash", "ashen_circle"), "initiate");
    let content = load_dir(&sample_dir()).expect("the sample content is valid");
    assert!(factional_content::warnings(&content).is_empty());
}

#[test]
fn promotion_in_riverhold_needs_the_next_ranks_requirements() {
    let mut world = riverhold();
    assert_eq!(
        world
            .execute(promote("vex", "lantern_guild"))
            .expect_err("not yet")
            .to_string(),
        "vex can't be promoted in lantern_guild: shadow needs standing 60.00, vex has 30.00"
    );
    world
        .execute(Command::ApplyOutcome {
            outcome: OutcomeId::new("fenced_the_crown_jewels").expect("valid id"),
            character: character("vex"),
            witnesses: Witnesses::Everyone,
        })
        .expect("accepted");
    world
        .execute(promote("vex", "lantern_guild"))
        .expect("60.00 now");
    assert_eq!(rank_of(&world, "vex", "lantern_guild"), "shadow");
    assert_eq!(
        world
            .execute(promote("captain_hale", "city_watch"))
            .expect_err("the top")
            .to_string(),
        "captain_hale is already a captain, the highest rank of city_watch"
    );
    assert_eq!(
        world
            .execute(promote("sister_mira", "temple"))
            .expect_err("standing")
            .to_string(),
        "sister_mira can't be promoted in temple: high_priest needs standing 80.00, sister_mira has 40.00"
    );
}

/// The good shift from `actor` murdering `target`, as the engine works it out.
fn murder_good(world: &World, actor: &str, target: &str) -> String {
    let shift = world
        .action_shift(
            &character(actor),
            &ActionId::new("murder").expect("valid id"),
            Some(&character(target)),
            Fixed::ONE,
        )
        .expect("all exist");
    shift.axes[1].shift.to_string()
}

#[test]
fn riverholds_murders_weigh_who_was_killed() {
    let world = riverhold();
    // DESIGN.md §5.4: the wicked, a saint, and a sworn enemy killed by a hardened captain.
    assert_eq!(murder_good(&world, "player", "brother_ash"), "-6.60");
    assert_eq!(murder_good(&world, "player", "sister_mira"), "-21.38");
    assert_eq!(murder_good(&world, "captain_hale", "vex"), "-6.64");
}
