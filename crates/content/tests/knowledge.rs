//! Who learns of an act in Riverhold, whose `knowledge.model` is `witnessed` (DESIGN.md
//! §10.1, P-55). Expected values are worked out by hand from §7.1: a change with the Watch
//! spills × 0.10 to the Temple (+60) and × −0.18 to the Guild (−80); one with the Temple
//! spills × 0.10 to the Watch (+60) and × −0.24 to the Ashen Circle (−90).

use std::path::Path;

use factional_content::load_dir;
use factional_core::Fixed;
use factional_reputation::{
    ActionId, Change, CharacterId, Command, Content, FactionId, KnowledgeModel, Learned, Party,
    Reached, Witnesses, World,
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

fn faction(id: &str) -> FactionId {
    FactionId::new(id).expect("valid id")
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

/// Each standing change the command made, as `party: before → after`.
fn standing_changes(world: &mut World, command: Command) -> Vec<String> {
    world
        .execute(command)
        .expect("accepted")
        .into_iter()
        .filter_map(|event| match event.payload {
            Change::StandingChanged {
                party,
                before,
                after,
                ..
            } => Some(format!("{party}: {before} → {after}")),
            _ => None,
        })
        .collect()
}

/// Riverhold with these characters in the Free Company.
fn with_free_company(members: &[&str]) -> World {
    in_free_company(sample(), members)
}

/// A world of this content, with these characters in the Free Company.
fn in_free_company(content: Content, members: &[&str]) -> World {
    let mut world = World::new(content).expect("valid content");
    for member in members {
        world
            .execute(Command::JoinFaction {
                character: character(member),
                faction: faction("free_company"),
            })
            .expect("joins");
    }
    world
}

#[test]
fn riverhold_uses_the_witnessed_model() {
    assert_eq!(sample().balance.knowledge, KnowledgeModel::Witnessed);
}

#[test]
fn a_theft_seen_only_by_a_bystander_changes_no_standing() {
    let mut world = riverhold();
    let events = world
        .execute(act(
            "player",
            "steal",
            Some("captain_hale"),
            seen_by(&["merchant_ava"]),
        ))
        .expect("accepted");
    let kinds: Vec<&str> = events
        .iter()
        .map(|event| match event.payload {
            Change::ActionPerformed { .. } => "performed",
            Change::AlignmentChanged { .. } => "alignment",
            _ => "other",
        })
        .collect();
    assert_eq!(
        kinds,
        ["performed", "alignment"],
        "Hale didn't notice, so neither he nor the Watch knows"
    );
}

#[test]
fn a_target_who_sees_it_learns_and_tells_their_factions() {
    let mut world = riverhold();
    assert_eq!(
        standing_changes(
            &mut world,
            act(
                "player",
                "steal",
                Some("captain_hale"),
                seen_by(&["captain_hale"]),
            ),
        ),
        [
            "city_watch: 0.00 → -10.00",
            "lantern_guild: 0.00 → 1.80",
            "temple: 0.00 → -1.00",
            "captain_hale: 0.00 → -20.00",
        ]
    );
}

#[test]
fn a_witness_tells_their_factions_but_the_target_learns_only_by_seeing_it() {
    let mut world = with_free_company(&["merchant_ava", "vex"]);
    assert_eq!(
        standing_changes(
            &mut world,
            act("player", "steal", Some("merchant_ava"), seen_by(&["vex"])),
        ),
        ["free_company: 0.00 → -10.00"],
        "the Company learns through Vex; Ava didn't notice"
    );
}

#[test]
fn the_actor_tells_no_one() {
    let mut world = with_free_company(&["merchant_ava", "player"]);
    assert_eq!(
        standing_changes(
            &mut world,
            act(
                "player",
                "steal",
                Some("merchant_ava"),
                seen_by(&["player"])
            ),
        ),
        Vec::<String>::new(),
        "the player is in the Company, but doesn't tell it"
    );
    assert_eq!(
        standing_changes(
            &mut world,
            act(
                "player",
                "steal",
                Some("merchant_ava"),
                seen_by(&["merchant_ava"]),
            ),
        ),
        ["free_company: 0.00 → -10.00", "merchant_ava: 0.00 → -20.00"]
    );
}

#[test]
fn a_character_the_act_names_tells_their_factions() {
    // A theft that also pays Vex his cut: he's named, so he learns of it unseen, and the
    // Company learns through him. Ava didn't notice.
    let mut content = sample();
    let steal = content
        .actions
        .get_mut(&ActionId::new("steal").expect("valid id"))
        .expect("Riverhold can steal");
    steal
        .standing
        .named
        .characters
        .insert(character("vex"), Fixed::from_hundredths(5_00));
    let mut world = in_free_company(content, &["merchant_ava", "vex"]);
    assert_eq!(
        standing_changes(
            &mut world,
            act("player", "steal", Some("merchant_ava"), Witnesses::Nobody),
        ),
        ["free_company: 0.00 → -10.00", "vex: 0.00 → 5.00"]
    );
}

#[test]
fn parties_an_act_names_learn_of_it_unseen() {
    let mut world = riverhold();
    assert_eq!(
        standing_changes(
            &mut world,
            act("player", "donate_to_temple", None, Witnesses::Nobody)
        ),
        [
            "ashen_circle: 0.00 → -2.40",
            "city_watch: 0.00 → 1.00",
            "temple: 0.00 → 10.00",
        ]
    );
}

#[test]
fn an_act_everyone_sees_reaches_everyone() {
    let mut world = riverhold();
    assert_eq!(
        standing_changes(
            &mut world,
            act("player", "steal", Some("captain_hale"), Witnesses::Everyone),
        ),
        [
            "city_watch: 0.00 → -10.00",
            "lantern_guild: 0.00 → 1.80",
            "temple: 0.00 → -1.00",
            "captain_hale: 0.00 → -20.00",
        ]
    );
}

#[test]
fn the_omniscient_model_ignores_witnesses() {
    let mut content = sample();
    content.balance.knowledge = KnowledgeModel::Omniscient;
    let mut world = World::new(content).expect("valid content");
    assert_eq!(
        standing_changes(
            &mut world,
            act(
                "player",
                "steal",
                Some("captain_hale"),
                seen_by(&["merchant_ava"]),
            ),
        ),
        [
            "city_watch: 0.00 → -10.00",
            "lantern_guild: 0.00 → 1.80",
            "temple: 0.00 → -1.00",
            "captain_hale: 0.00 → -20.00",
        ]
    );
}

fn reached(party: Party, change: i64, learned: Option<Learned>) -> Reached {
    Reached {
        party,
        change: Fixed::from_hundredths(change),
        learned,
    }
}

fn reach(world: &World, action: &str, target: Option<&str>, witnesses: &Witnesses) -> Vec<Reached> {
    world
        .reach(
            &character("player"),
            &ActionId::new(action).expect("valid id"),
            target.map(character).as_ref(),
            witnesses,
        )
        .expect("everyone exists")
}

#[test]
fn reach_says_who_would_learn_and_how() {
    let world = riverhold();
    let watch = || Party::Faction(faction("city_watch"));
    let hale = || Party::Character(character("captain_hale"));
    assert_eq!(
        reach(
            &world,
            "steal",
            Some("captain_hale"),
            &seen_by(&["merchant_ava"])
        ),
        [
            reached(watch(), -10_00, None),
            reached(hale(), -20_00, None)
        ]
    );
    assert_eq!(
        reach(
            &world,
            "steal",
            Some("captain_hale"),
            &seen_by(&["captain_hale"])
        ),
        [
            reached(
                watch(),
                -10_00,
                Some(Learned::ThroughMember(character("captain_hale")))
            ),
            reached(hale(), -20_00, Some(Learned::Witness)),
        ]
    );
    assert_eq!(
        reach(&world, "steal", Some("captain_hale"), &Witnesses::Everyone),
        [
            reached(watch(), -10_00, Some(Learned::Everyone)),
            reached(hale(), -20_00, Some(Learned::Everyone)),
        ]
    );
    assert_eq!(
        reach(&world, "donate_to_temple", None, &Witnesses::Nobody),
        [reached(
            Party::Faction(faction("temple")),
            10_00,
            Some(Learned::Named)
        )]
    );
    assert_eq!(
        world.reach(
            &character("player"),
            &ActionId::new("steal").expect("valid id"),
            None,
            &seen_by(&["nobody_at_all"]),
        ),
        None,
        "an unknown witness"
    );
}

#[test]
fn reach_says_the_actor_tells_no_one() {
    let world = with_free_company(&["merchant_ava", "player"]);
    assert_eq!(
        reach(&world, "steal", Some("merchant_ava"), &seen_by(&["player"])),
        [
            reached(Party::Faction(faction("free_company")), -10_00, None),
            reached(Party::Character(character("merchant_ava")), -20_00, None),
        ]
    );
}
