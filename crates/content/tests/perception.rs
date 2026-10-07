//! Everyone judges a character by what they know of them (DESIGN.md §10.3, D-21, P-57), in
//! Riverhold, whose news ripples at 0.50, 0.25 then 0.10, ten ticks a hop. Distances are
//! worked out by hand from §6: Hale (75 / 30, weights 1 / 0.25) is 75.37 from 0 / 0, 77.90
//! from −2.50 / −1.50 and 80.42 from −5 / −3; the Watch (70 / 20, 1 / 0.25) is 72.70 from
//! −2.50 / −1.50. Affinity is `[[0, 50], [60, 0], [200, -50]]` (§8.1).

use std::path::Path;

use factional_content::load_dir;
use factional_core::Fixed;
use factional_reputation::{
    ActionId, Alignment, AlignmentDelta, Change, CharacterId, Command, CommandError, ComponentKind,
    Content, FactionId, JoinBlock, KnowledgeModel, LeaveReason, Observer, OutcomeId, Party, Role,
    Witnesses, World,
};

fn sample() -> Content {
    load_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/sample"))
        .expect("the sample content is valid")
}

fn riverhold() -> World {
    World::new(sample()).expect("the sample content makes a world")
}

fn character(id: &str) -> CharacterId {
    CharacterId::new(id).expect("valid id")
}

fn faction_id(id: &str) -> FactionId {
    FactionId::new(id).expect("valid id")
}

fn as_character(id: &str) -> Observer {
    Observer::Character(character(id))
}

fn as_faction(id: &str) -> Observer {
    Observer::Faction(faction_id(id))
}

const fn h(hundredths: i64) -> Fixed {
    Fixed::from_hundredths(hundredths)
}

fn at(law: i64, good: i64) -> Alignment {
    Alignment::new(h(law), h(good)).expect("on the plane")
}

fn seen_by(witnesses: &[&str]) -> Witnesses {
    Witnesses::These(witnesses.iter().copied().map(character).collect())
}

fn act(actor: &str, action: &str, target: Option<&str>, witnesses: Witnesses) -> Command {
    Command::PerformAction {
        actor: character(actor),
        action: ActionId::new(action).expect("valid id"),
        target: target.map(character),
        scale: Fixed::ONE,
        witnesses,
    }
}

fn outcome(outcome: &str, to: &str, witnesses: Witnesses) -> Command {
    Command::ApplyOutcome {
        outcome: OutcomeId::new(outcome).expect("valid id"),
        character: character(to),
        witnesses,
    }
}

fn run(world: &mut World, command: Command) -> Vec<Change> {
    world
        .execute(command)
        .expect("accepted")
        .into_iter()
        .map(|event| event.payload)
        .collect()
}

fn advance(world: &mut World, ticks: u64) -> Vec<Change> {
    run(world, Command::AdvanceTime { ticks })
}

/// How `observer` pictures `subject`.
fn pictured(world: &World, observer: &Observer, subject: &str) -> Alignment {
    world
        .perceived(observer, &character(subject))
        .expect("both exist")
        .perceived
}

fn distance(world: &World, observer: &Observer, subject: &str) -> Fixed {
    world
        .distance(observer, &character(subject))
        .expect("both exist")
        .value
}

/// Everyone who could observe someone: every faction, then every character.
fn everyone(world: &World) -> Vec<Observer> {
    world
        .factions()
        .map(|faction| Observer::Faction(faction.id.clone()))
        .chain(
            world
                .characters()
                .map(|character| Observer::Character(character.id.clone())),
        )
        .collect()
}

#[test]
fn a_theft_ava_alone_saw_changes_only_how_she_pictures_the_player() {
    let mut world = riverhold();
    let changes = run(
        &mut world,
        act(
            "player",
            "steal",
            Some("captain_hale"),
            seen_by(&["merchant_ava"]),
        ),
    );
    assert!(changes.contains(&Change::ShiftWitnessed {
        character: character("player"),
        shift: AlignmentDelta {
            law: h(-5_00),
            good: h(-3_00),
        },
        seen_by: [Party::Character(character("merchant_ava"))].into(),
    }));
    assert_eq!(
        pictured(&world, &as_character("merchant_ava"), "player"),
        at(-5_00, -3_00)
    );
    assert_eq!(
        pictured(&world, &as_character("captain_hale"), "player"),
        at(0, 0)
    );
    assert_eq!(
        pictured(&world, &as_faction("city_watch"), "player"),
        at(0, 0)
    );
    assert_eq!(
        pictured(&world, &as_character("player"), "player"),
        at(-5_00, -3_00),
        "everyone knows themself"
    );
    let working = world
        .perceived(&as_character("captain_hale"), &character("player"))
        .expect("both exist");
    assert_eq!(working.truth, at(-5_00, -3_00));
    assert_eq!(
        (working.hidden, working.heard),
        (
            AlignmentDelta {
                law: h(-5_00),
                good: h(-3_00),
            },
            AlignmentDelta::default()
        )
    );
}

#[test]
fn hale_judges_the_player_by_his_picture() {
    let mut world = riverhold();
    run(
        &mut world,
        act(
            "player",
            "steal",
            Some("captain_hale"),
            seen_by(&["merchant_ava"]),
        ),
    );
    let hale = as_character("captain_hale");
    let measured = world
        .distance(&hale, &character("player"))
        .expect("both exist");
    assert_eq!(
        (measured.value, measured.subject, measured.truth),
        (h(75_37), at(0, 0), at(-5_00, -3_00))
    );
    let affinity = |world: &World| {
        world
            .disposition(&hale, &character("player"))
            .expect("both exist")
            .components
            .iter()
            .find(|component| component.kind == ComponentKind::Affinity)
            .expect("an affinity")
            .value
    };
    // (75.37 − 60) / 140 × −50 = −5.49; seen by everyone it would be 80.42: −7.29.
    assert_eq!(affinity(&world), h(-5_49));
    let mut seen = riverhold();
    run(
        &mut seen,
        act("player", "steal", Some("captain_hale"), Witnesses::Everyone),
    );
    assert_eq!(affinity(&seen), h(-7_29));
}

#[test]
fn news_moves_pictures_by_the_strength_it_arrives_at() {
    let mut world = riverhold();
    run(
        &mut world,
        act(
            "player",
            "steal",
            Some("captain_hale"),
            seen_by(&["merchant_ava"]),
        ),
    );
    advance(&mut world, 10);
    for observer in [
        as_character("captain_hale"),
        as_character("sister_mira"),
        as_faction("city_watch"),
        as_faction("temple"),
    ] {
        assert_eq!(
            pictured(&world, &observer, "player"),
            at(-2_50, -1_50),
            "{observer:?}"
        );
    }
    assert_eq!(
        distance(&world, &as_character("captain_hale"), "player"),
        h(77_90)
    );
    assert_eq!(
        distance(&world, &as_faction("city_watch"), "player"),
        h(72_70)
    );
    advance(&mut world, 10);
    for observer in [as_character("brother_ash"), as_faction("ashen_circle")] {
        assert_eq!(
            pictured(&world, &observer, "player"),
            at(-1_25, -75),
            "{observer:?}"
        );
    }
    assert_eq!(
        pictured(&world, &as_character("vex"), "player"),
        at(0, 0),
        "the news never reached Vex"
    );
}

#[test]
fn an_act_everyone_sees_moves_every_picture() {
    let mut world = riverhold();
    let changes = run(
        &mut world,
        act("player", "steal", Some("captain_hale"), Witnesses::Everyone),
    );
    assert!(
        !changes
            .iter()
            .any(|change| matches!(change, Change::ShiftWitnessed { .. }))
    );
    for observer in everyone(&world) {
        assert_eq!(
            pictured(&world, &observer, "player"),
            at(-5_00, -3_00),
            "{observer:?}"
        );
    }
}

#[test]
fn under_the_omniscient_model_everyone_pictures_the_truth() {
    let mut content = sample();
    content.balance.knowledge = KnowledgeModel::Omniscient;
    let mut world = World::new(content).expect("valid content");
    let changes = run(
        &mut world,
        act(
            "player",
            "steal",
            Some("captain_hale"),
            seen_by(&["merchant_ava"]),
        ),
    );
    assert!(
        !changes
            .iter()
            .any(|change| matches!(change, Change::ShiftWitnessed { .. }))
    );
    for observer in everyone(&world) {
        assert_eq!(
            pictured(&world, &observer, "player"),
            at(-5_00, -3_00),
            "{observer:?}"
        );
    }
}

#[test]
fn the_guild_judges_a_secret_thief_by_what_it_knows() {
    // Three thefts no one saw: the player is truly −15 / −9, 45.00 from the Guild (law Δ45,
    // good Δ1 × 0.5), within its tolerance of 45. But the Guild still pictures 0 / 0, 60.21
    // away (law Δ60, good Δ10 × 0.5).
    let mut unseen = riverhold();
    for _ in 0..3 {
        run(&mut unseen, act("player", "steal", None, Witnesses::Nobody));
    }
    let assessment = unseen
        .assess_join(&character("player"), &faction_id("lantern_guild"))
        .expect("both exist");
    assert_eq!(
        assessment.blocks,
        [JoinBlock::OutsideTolerance {
            distance: h(60_21),
            tolerance: h(45_00),
        }]
    );
    let mut seen = riverhold();
    for _ in 0..3 {
        run(&mut seen, act("player", "steal", None, Witnesses::Everyone));
    }
    let assessment = seen
        .assess_join(&character("player"), &faction_id("lantern_guild"))
        .expect("both exist");
    assert_eq!(
        (assessment.distance.value, assessment.allowed()),
        (h(45_00), true)
    );
}

#[test]
fn the_circle_keeps_a_member_whose_change_of_heart_it_never_saw() {
    // Brother Ash (25 / −70, steady) rescues Ava again and again: good +6 each time. After
    // five, he's truly 25 / −40, 40.02 from the Ashen Circle (law Δ5 × 0.25, good Δ40), past
    // its member tolerance of 40, and it expels drifters.
    let expelled = |changes: &[Change]| {
        changes.iter().any(|change| {
            matches!(
                change,
                Change::LeftFaction {
                    reason: LeaveReason::Expelled,
                    ..
                }
            )
        })
    };
    let mut seen = riverhold();
    for rescue in 1..=5 {
        let changes = run(
            &mut seen,
            outcome("rescued_merchant", "brother_ash", Witnesses::Everyone),
        );
        assert_eq!(expelled(&changes), rescue == 5, "rescue {rescue}");
    }
    // Unseen, only Ava and the Watch, whom it names, know; the news never reaches another
    // member of the Circle, so it keeps him.
    let mut unseen = riverhold();
    for _ in 1..=5 {
        let changes = run(
            &mut unseen,
            outcome("rescued_merchant", "brother_ash", Witnesses::Nobody),
        );
        assert!(!expelled(&changes));
    }
    assert!(!expelled(&advance(&mut unseen, 100)));
    assert!(
        unseen
            .memberships(&character("brother_ash"))
            .expect("Ash exists")
            .any(|(faction, _)| faction.as_str() == "ashen_circle")
    );
    assert_eq!(
        pictured(&unseen, &as_faction("ashen_circle"), "brother_ash"),
        at(25_00, -70_00)
    );
}

#[test]
fn an_outcome_tells_the_parties_it_names_and_the_news_goes_on() {
    // rescued_merchant: good +6, Ava +30, the Watch +10. Unseen, Ava and the Watch learn of
    // the player's good deed; Hale and Mira hear of it at 0.50 ten ticks later.
    let mut world = riverhold();
    run(
        &mut world,
        outcome("rescued_merchant", "player", Witnesses::Nobody),
    );
    assert_eq!(
        pictured(&world, &as_character("merchant_ava"), "player"),
        at(0, 6_00)
    );
    assert_eq!(
        pictured(&world, &as_faction("city_watch"), "player"),
        at(0, 6_00)
    );
    assert_eq!(
        pictured(&world, &as_character("captain_hale"), "player"),
        at(0, 0)
    );
    advance(&mut world, 10);
    for observer in [
        as_character("captain_hale"),
        as_character("sister_mira"),
        as_faction("temple"),
    ] {
        assert_eq!(
            pictured(&world, &observer, "player"),
            at(0, 3_00),
            "{observer:?}"
        );
    }
}

#[test]
fn an_outcomes_witnesses_must_exist() {
    let mut world = riverhold();
    assert_eq!(
        world.execute(outcome(
            "rescued_merchant",
            "player",
            seen_by(&["merchant_av"])
        )),
        Err(CommandError::UnknownCharacter {
            role: Role::Witness,
            id: character("merchant_av"),
            suggestion: Some(character("merchant_ava")),
        })
    );
}
