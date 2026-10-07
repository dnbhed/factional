//! News rippling through Riverhold (DESIGN.md §10.2, D-22, P-56): `knowledge.model` is
//! `ripple`, news arrives at 0.50, 0.25 then 0.10 of its strength, ten ticks a hop, and Ava's
//! contacts are Hale and Mira, Mira's Brother Ash. Expected values are worked out by hand
//! from §7.1: a change with the Watch spills × −0.18 to the Guild (−80) and × 0.10 to the
//! Temple (+60).

use std::collections::BTreeSet;
use std::path::Path;

use factional_content::load_dir;
use factional_core::{Fixed, Tick};
use factional_reputation::{
    ActionId, Change, CharacterId, Command, Content, FactionId, KnowledgeModel, NextHop, Party,
    Ripple, Witnesses, World,
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

fn who(id: &str) -> Party {
    Party::Character(character(id))
}

fn faction(id: &str) -> Party {
    Party::Faction(FactionId::new(id).expect("valid id"))
}

const fn h(hundredths: i64) -> Fixed {
    Fixed::from_hundredths(hundredths)
}

fn steal(actor: &str, target: &str, seen_by: &[&str]) -> Command {
    Command::PerformAction {
        actor: character(actor),
        action: ActionId::new("steal").expect("valid id"),
        target: Some(character(target)),
        scale: Fixed::ONE,
        witnesses: Witnesses::These(seen_by.iter().copied().map(character).collect()),
    }
}

fn advance(world: &mut World, ticks: u64) -> Vec<Change> {
    world
        .execute(Command::AdvanceTime { ticks })
        .expect("accepted")
        .into_iter()
        .map(|event| event.payload)
        .collect()
}

fn hop(hop: u32, at: u64, awareness: i64, parties: &[&str]) -> NextHop {
    NextHop {
        hop,
        at: Tick(at),
        awareness: h(awareness),
        parties: parties.iter().copied().map(character).collect(),
    }
}

/// The `NewsSent` among an act's events, if any.
fn sent(events: &[factional_reputation::Event]) -> Option<Change> {
    events
        .iter()
        .map(|event| event.payload.clone())
        .find(|change| matches!(change, Change::NewsSent { .. }))
}

fn parties(parties: &[Party]) -> BTreeSet<Party> {
    parties.iter().cloned().collect()
}

/// Each standing change among `changes`, as `party: before → after`.
fn standings(changes: &[Change]) -> Vec<String> {
    changes
        .iter()
        .filter_map(|change| match change {
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

#[test]
fn riverhold_ripples_news_ten_ticks_a_hop() {
    let content = sample();
    assert_eq!(content.balance.knowledge, KnowledgeModel::Ripple);
    assert_eq!(
        content.balance.ripple,
        Ripple {
            strength: vec![h(50), h(25), h(10)],
            hop_ticks: 10,
        }
    );
    let contacts = |id: &str| content.characters[&character(id)].contacts.clone();
    assert_eq!(
        contacts("merchant_ava"),
        [character("captain_hale"), character("sister_mira")]
    );
    assert_eq!(contacts("sister_mira"), [character("brother_ash")]);
}

#[test]
fn strength_runs_down_the_list_then_stops() {
    let ripple = sample().balance.ripple;
    let strengths: Vec<Option<Fixed>> = (0..5).map(|hop| ripple.awareness(hop)).collect();
    assert_eq!(
        strengths,
        [
            Some(Fixed::ONE),
            Some(h(50)),
            Some(h(25)),
            Some(h(10)),
            None
        ]
    );
}

/// DESIGN.md §10.2's worked example.
#[test]
fn news_of_an_unseen_theft_reaches_hale_and_the_watch_ten_ticks_later() {
    let mut world = riverhold();
    let events = world
        .execute(steal("player", "captain_hale", &["merchant_ava"]))
        .expect("accepted");
    // Tick 0: Ava saw it, but steal names no standing for her; Hale didn't.
    assert_eq!(
        sent(&events),
        Some(Change::NewsSent {
            news: 1,
            actor: character("player"),
            heard: parties(&[who("merchant_ava"), who("player")]),
            due: vec![
                (faction("city_watch"), h(-10_00)),
                (who("captain_hale"), h(-20_00))
            ],
            next: hop(1, 10, 50, &["captain_hale", "sister_mira"]),
        })
    );
    assert_eq!(standings(&advance(&mut world, 5)), Vec::<String>::new());
    // Tick 10: Hale and Mira hear at 0.50, and through them the Watch and the Temple.
    let tick_10 = advance(&mut world, 5);
    assert_eq!(
        tick_10[1],
        Change::NewsArrived {
            news: 1,
            arrived: hop(1, 10, 50, &["captain_hale", "sister_mira"]),
            learned: parties(&[
                faction("city_watch"),
                faction("temple"),
                who("captain_hale"),
                who("sister_mira"),
            ]),
            next: Some(hop(2, 20, 25, &["brother_ash"])),
        }
    );
    assert_eq!(
        standings(&tick_10),
        [
            "city_watch: 0.00 → -5.00",
            "lantern_guild: 0.00 → 0.90",
            "temple: 0.00 → -0.50",
            "captain_hale: 0.00 → -10.00",
        ]
    );
    // Tick 20: Ash hears at 0.25, and through him the Circle; everyone else has heard.
    let tick_20 = advance(&mut world, 10);
    assert_eq!(
        tick_20,
        [
            Change::TimeAdvanced {
                from: Tick(10),
                to: Tick(20),
            },
            Change::NewsArrived {
                news: 1,
                arrived: hop(2, 20, 25, &["brother_ash"]),
                learned: parties(&[faction("ashen_circle"), who("brother_ash")]),
                next: None,
            },
        ]
    );
    assert_eq!(world.news().count(), 0, "the news has stopped");
}

#[test]
fn one_long_advance_carries_news_every_hop_it_reaches() {
    let mut world = riverhold();
    world
        .execute(steal("player", "captain_hale", &["merchant_ava"]))
        .expect("accepted");
    let arrivals: Vec<u32> = advance(&mut world, 25)
        .iter()
        .filter_map(|change| match change {
            Change::NewsArrived { arrived, .. } => Some(arrived.hop),
            _ => None,
        })
        .collect();
    assert_eq!(arrivals, [1, 2]);
    assert_eq!(world.news().count(), 0);
}

#[test]
fn news_in_flight_says_who_has_heard_and_where_it_goes_next() {
    let mut world = riverhold();
    world
        .execute(steal("player", "captain_hale", &["merchant_ava"]))
        .expect("accepted");
    let news: Vec<(u64, Vec<Party>, NextHop)> = world
        .news()
        .map(|(id, news)| (id, news.heard.iter().cloned().collect(), news.next.clone()))
        .collect();
    assert_eq!(
        news,
        [(
            1,
            vec![who("merchant_ava"), who("player")],
            hop(1, 10, 50, &["captain_hale", "sister_mira"])
        )]
    );
}

#[test]
fn those_who_learned_firsthand_arent_told_again() {
    let mut world = riverhold();
    let events = world
        .execute(steal("player", "captain_hale", &["captain_hale"]))
        .expect("accepted");
    assert_eq!(
        sent(&events),
        Some(Change::NewsSent {
            news: 1,
            actor: character("player"),
            heard: parties(&[faction("city_watch"), who("captain_hale"), who("player")]),
            due: Vec::new(),
            next: hop(1, 10, 50, &["merchant_ava"]),
        })
    );
    let tick_10 = advance(&mut world, 10);
    assert_eq!(standings(&tick_10), Vec::<String>::new());
    assert_eq!(
        tick_10[1],
        Change::NewsArrived {
            news: 1,
            arrived: hop(1, 10, 50, &["merchant_ava"]),
            learned: parties(&[who("merchant_ava")]),
            next: Some(hop(2, 20, 25, &["sister_mira"])),
        }
    );
}

#[test]
fn news_never_reaches_the_actors_factions_through_the_actor() {
    // Hale robs Vex, seen by Ava: she tells Mira, but not Hale, who did it, so the Watch
    // never hears.
    let mut world = riverhold();
    let events = world
        .execute(steal("captain_hale", "vex", &["merchant_ava"]))
        .expect("accepted");
    let Some(Change::NewsSent { next, .. }) = sent(&events) else {
        panic!("news is sent: {events:?}");
    };
    assert_eq!(next, hop(1, 10, 50, &["sister_mira"]));
    let learned: BTreeSet<Party> = advance(&mut world, 100)
        .into_iter()
        .filter_map(|change| match change {
            Change::NewsArrived { learned, .. } => Some(learned),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(
        learned,
        parties(&[
            faction("ashen_circle"),
            faction("temple"),
            who("brother_ash"),
            who("sister_mira"),
        ])
    );
}

#[test]
fn an_act_everyone_sees_sends_no_news() {
    let mut world = riverhold();
    let events = world
        .execute(Command::PerformAction {
            actor: character("player"),
            action: ActionId::new("steal").expect("valid id"),
            target: Some(character("captain_hale")),
            scale: Fixed::ONE,
            witnesses: Witnesses::Everyone,
        })
        .expect("accepted");
    assert!(
        !events
            .iter()
            .any(|event| matches!(event.payload, Change::NewsSent { .. })),
        "{events:?}"
    );
    assert_eq!(world.news().count(), 0);
}

#[test]
fn replay_and_restore_bring_back_news_in_flight() {
    let mut world = riverhold();
    world
        .execute(steal("player", "captain_hale", &["merchant_ava"]))
        .expect("accepted");
    advance(&mut world, 10);
    let in_flight: Vec<_> = world.news().map(|(id, news)| (id, news.clone())).collect();
    assert_eq!(in_flight.len(), 1);
    let replayed = World::replay(sample(), world.events()).expect("replays");
    let replayed: Vec<_> = replayed
        .news()
        .map(|(id, news)| (id, news.clone()))
        .collect();
    assert_eq!(replayed, in_flight);
    let restored =
        World::restore(sample(), &world.saved_journal(), world.events()).expect("restores");
    let restored: Vec<_> = restored
        .news()
        .map(|(id, news)| (id, news.clone()))
        .collect();
    assert_eq!(restored, in_flight);
}
