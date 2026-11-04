//! Quests in the builder (G7): who offers the played character which quests, their givers
//! marked, and quests started and chosen from the panel. Expected values are worked by hand
//! from the sample's quests, questlines and outcomes.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::time::TimeUpdateStrategy;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use factional_bevy::{Factional, FactionalCommand, FactionalPlugin};
use factional_builder::{
    Asked, BuilderPlugin, Givers, Nearby, Offer, QuestPanel, UnderWay, ask, givers, offered_by,
    panel, quests_ui, the_worlds,
};
use factional_core::Fixed;
use factional_quests::{ChoiceId, QuestCommand, QuestId, QuestLog, StageId};
use factional_reputation::{CharacterId, Command, Effects, FactionId, Party, Witnesses, World};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../..");

fn sample() -> PathBuf {
    Path::new(REPO).join("content/sample")
}

fn id(text: &str) -> CharacterId {
    CharacterId::new(text).expect("an id")
}

fn quest(text: &str) -> QuestId {
    QuestId::new(text).expect("an id")
}

fn choice(text: &str) -> ChoiceId {
    ChoiceId::new(text).expect("an id")
}

fn quests(list: &[&str]) -> Vec<QuestId> {
    list.iter().map(|text| quest(text)).collect()
}

fn riverhold() -> (World, QuestLog) {
    let (content, found) = factional_content::load_quests(&sample()).expect("loads");
    (World::new(content).expect("complete"), QuestLog::new(found))
}

/// The builder on the sample, playing `player`.
fn app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(1)))
        .add_plugins(FactionalPlugin {
            content: sample(),
            tick_every: None,
        })
        .add_plugins(BuilderPlugin {
            content: sample(),
            player: "player".to_owned(),
        });
    app.update();
    app
}

fn send(app: &mut App, command: FactionalCommand) {
    app.world_mut().write_message(command);
    app.update();
}

/// The panel for the player, with `giver` in reach, as the app's world stands.
fn panel_now(app: &App, giver: Option<&str>) -> QuestPanel {
    let factional = app.world().resource::<Factional>();
    panel(
        &factional.world,
        &factional.quests,
        &id("player"),
        giver.map(id).as_ref(),
    )
}

/// The player's standing with city_watch shifted by `by`.
fn shift_watch_standing(by: &str) -> FactionalCommand {
    let effects: Effects = toml::from_str(&format!(
        "alignment = {{ law = 0.0, good = 0.0 }}\nstanding = {{ factions = {{ city_watch = {by} }}, characters = {{}} }}"
    ))
    .expect("effects");
    FactionalCommand::World(Command::ApplyEffects {
        source: "test".to_owned(),
        character: id("player"),
        effects,
        witnesses: Witnesses::Nobody,
    })
}

fn near(nearest: &str, seen_by: &[&str]) -> Nearby {
    Nearby {
        nearest: Some(id(nearest)),
        seen_by: seen_by.iter().map(|text| id(text)).collect(),
    }
}

#[test]
fn a_quest_is_offered_by_its_giver_a_faction_through_its_members_and_the_world_everywhere() {
    let (world, log) = riverhold();
    assert_eq!(
        offered_by(&world, &log, &id("captain_hale")),
        quests(&[
            "dock_inspection",
            "harbour_errands",
            "lost_dog",
            "night_patrol",
            "smugglers_cove",
            "watch_captain",
            "watch_oath",
        ]),
        "the Watch's through his membership, the dog his own"
    );
    assert_eq!(
        offered_by(&world, &log, &id("merchant_ava")),
        quests(&["lost_ring"])
    );
    assert_eq!(
        offered_by(&world, &log, &id("brother_ash")),
        quests(&["circle_rite"])
    );
    assert!(offered_by(&world, &log, &id("sister_mira")).is_empty());
    assert!(offered_by(&world, &log, &id("vex")).is_empty());
    assert_eq!(the_worlds(&log), quests(&["the_long_winter"]));
}

#[test]
fn a_faction_offers_through_whoever_is_a_member_now() {
    let (mut world, log) = riverhold();
    world
        .execute(Command::LeaveFaction {
            character: id("captain_hale"),
            faction: FactionId::new("city_watch").expect("an id"),
        })
        .expect("Hale leaves the Watch");
    assert_eq!(
        offered_by(&world, &log, &id("captain_hale")),
        quests(&["lost_dog"]),
        "out of the Watch, only his own"
    );
}

#[test]
fn givers_are_marked_when_they_offer_a_quest_the_played_can_start_now() {
    let (world, log) = riverhold();
    assert_eq!(
        givers(&world, &log, Some(&id("player"))),
        BTreeSet::from([id("captain_hale"), id("merchant_ava")]),
        "Ash's rite needs standing the player hasn't"
    );
    assert_eq!(
        givers(&world, &log, Some(&id("captain_hale"))),
        BTreeSet::from([id("merchant_ava")]),
        "no one offers to themselves"
    );
    assert!(givers(&world, &log, None).is_empty());
}

#[test]
fn near_a_giver_the_panel_offers_what_can_start_and_says_why_the_rest_cant() {
    let (world, log) = riverhold();
    let found = panel(&world, &log, &id("player"), Some(&id("captain_hale")));
    assert_eq!(found.giver, Some(id("captain_hale")));
    let startable: Vec<&QuestId> = found
        .offered
        .iter()
        .filter(|offer| offer.can_start())
        .map(|offer| &offer.quest)
        .collect();
    assert_eq!(startable, [&quest("watch_oath")]);
    assert_eq!(found.offered.len(), 7);
    let dog = found
        .offered
        .iter()
        .find(|offer| offer.quest == quest("lost_dog"))
        .expect("the dog offered");
    assert_eq!(dog.name, "The Captain's Dog");
    assert_eq!(
        dog.blocks,
        ["step 1 of watch_career isn't complete: 0 of 1 done"]
    );
    let ash = panel(&world, &log, &id("player"), Some(&id("brother_ash")));
    assert_eq!(
        ash.offered[0].blocks,
        ["it needs standing 10.00 with ashen_circle, and player has 0.00"]
    );
    let alone = panel(&world, &log, &id("player"), None);
    assert_eq!(alone.giver, None);
    assert!(alone.offered.is_empty());
    for panel in [found, alone] {
        assert_eq!(
            panel.worlds,
            [Offer {
                quest: quest("the_long_winter"),
                name: "The Long Winter".to_owned(),
                blocks: Vec::new(),
            }],
            "the world's own, from anywhere"
        );
        assert!(panel.under_way.is_empty());
    }
}

#[test]
fn starting_and_choosing_send_the_played_characters_quest_commands() {
    assert_eq!(
        ask(
            &id("player"),
            Asked::Start(quest("watch_oath")),
            &near("captain_hale", &["captain_hale", "merchant_ava"])
        ),
        FactionalCommand::Quest(QuestCommand::StartQuest {
            character: id("player"),
            quest: quest("watch_oath"),
        })
    );
    assert_eq!(
        ask(
            &id("player"),
            Asked::Choose(quest("watch_oath"), choice("report")),
            &near("captain_hale", &["captain_hale", "merchant_ava"])
        ),
        FactionalCommand::Quest(QuestCommand::MakeChoice {
            character: id("player"),
            quest: quest("watch_oath"),
            choice: choice("report"),
            witnesses: Witnesses::These(BTreeSet::from([id("captain_hale"), id("merchant_ava")])),
        }),
        "witnessed by those in sight, as acting is"
    );
    assert_eq!(
        ask(
            &id("player"),
            Asked::Choose(quest("lost_ring"), choice("pawn_it")),
            &Nearby::default()
        ),
        FactionalCommand::Quest(QuestCommand::MakeChoice {
            character: id("player"),
            quest: quest("lost_ring"),
            choice: choice("pawn_it"),
            witnesses: Witnesses::Nobody,
        })
    );
}

#[test]
fn the_watchs_oath_started_chosen_and_sworn_in_the_builder() {
    let mut app = app();
    let markers = |app: &App| app.world().resource::<Givers>().0.clone();
    assert_eq!(
        markers(&app),
        BTreeSet::from([id("captain_hale"), id("merchant_ava")])
    );
    let hale = near("captain_hale", &["captain_hale"]);
    send(
        &mut app,
        ask(&id("player"), Asked::Start(quest("watch_oath")), &hale),
    );
    assert_eq!(
        panel_now(&app, Some("captain_hale")).under_way,
        [UnderWay {
            quest: quest("watch_oath"),
            name: "The Watch's Oath".to_owned(),
            stage: StageId::new("patrol").expect("an id"),
            choices: vec![choice("report"), choice("look_away")],
            unmet: Vec::new(),
        }]
    );
    assert!(
        !panel_now(&app, Some("captain_hale"))
            .offered
            .iter()
            .any(|offer| offer.quest == quest("watch_oath")),
        "begun, it's no longer offered"
    );
    assert_eq!(
        markers(&app),
        BTreeSet::from([id("merchant_ava")]),
        "nothing else of Hale's can start until the oath is done"
    );

    send(
        &mut app,
        ask(
            &id("player"),
            Asked::Choose(quest("watch_oath"), choice("report")),
            &hale,
        ),
    );
    let watch = Party::Faction(FactionId::new("city_watch").expect("an id"));
    let standing = |app: &App| {
        app.world()
            .resource::<Factional>()
            .world
            .standing(&id("player"), &watch)
            .expect("a standing")
    };
    // turned_in_vex: +15.00 with the Watch, and -25.00 with the Guild, which the Watch
    // regards at -80, spilling -25.00 x -0.18 = +4.50 (DESIGN.md §7.1).
    assert_eq!(standing(&app), Fixed::from_hundredths(19_50));
    let oath = &panel_now(&app, Some("captain_hale")).under_way[0];
    assert_eq!(oath.stage, StageId::new("oath").expect("an id"));
    assert_eq!(oath.choices, [choice("swear")]);
    assert!(oath.unmet.is_empty(), "19.50 is at least 10.00");

    send(&mut app, shift_watch_standing("-10.0"));
    assert_eq!(
        panel_now(&app, Some("captain_hale")).under_way[0].unmet,
        ["it needs standing 10.00 with city_watch, and player has 9.50"]
    );

    send(&mut app, shift_watch_standing("10.0"));
    send(
        &mut app,
        ask(
            &id("player"),
            Asked::Choose(quest("watch_oath"), choice("swear")),
            &hale,
        ),
    );
    assert!(
        panel_now(&app, Some("captain_hale")).under_way.is_empty(),
        "finished, it leaves the panel"
    );
    assert!(
        !panel_now(&app, Some("captain_hale"))
            .offered
            .iter()
            .any(|offer| offer.quest == quest("watch_oath")),
        "nor is it offered again"
    );
    assert_eq!(
        markers(&app),
        BTreeSet::from([id("captain_hale"), id("merchant_ava")]),
        "the odd jobs open"
    );
}

/// A panel as the oath's patrol would show it beside Hale, with a stage that can't be chosen.
fn a_panel() -> QuestPanel {
    QuestPanel {
        giver: Some(id("captain_hale")),
        offered: vec![
            Offer {
                quest: quest("lost_dog"),
                name: "The Captain's Dog".to_owned(),
                blocks: vec!["step 1 of watch_career isn't complete: 0 of 1 done".to_owned()],
            },
            Offer {
                quest: quest("watch_oath"),
                name: "The Watch's Oath".to_owned(),
                blocks: Vec::new(),
            },
        ],
        worlds: vec![Offer {
            quest: quest("the_long_winter"),
            name: "The Long Winter".to_owned(),
            blocks: Vec::new(),
        }],
        under_way: vec![UnderWay {
            quest: quest("lost_ring"),
            name: "Ava's Lost Ring".to_owned(),
            stage: StageId::new("search").expect("an id"),
            choices: vec![choice("return_it"), choice("pawn_it")],
            unmet: vec!["it needs standing 10.00 with city_watch, and player has 5.00".to_owned()],
        }],
    }
}

#[test]
fn the_panel_starts_what_can_start_and_hides_the_rest_under_not_yet() {
    let mut harness = Harness::new_ui_state(
        |ui, asked: &mut Vec<Asked>| {
            asked.extend(quests_ui(ui, &a_panel()));
        },
        Vec::new(),
    );
    harness.run();
    harness.get_by_label("Offered by captain_hale");
    harness.get_by_label("The world's");
    assert!(
        harness
            .query_by_label("step 1 of watch_career isn't complete: 0 of 1 done")
            .is_none(),
        "folded away"
    );
    harness.get_by_label("Not yet (1)").click();
    harness.run();
    harness.get_by_label("The Captain's Dog");
    harness.get_by_label("step 1 of watch_career isn't complete: 0 of 1 done");
    harness.get_by_label("Start The Watch's Oath").click();
    harness.run();
    harness.get_by_label("Start The Long Winter").click();
    harness.run();
    assert_eq!(
        harness.state(),
        &[
            Asked::Start(quest("watch_oath")),
            Asked::Start(quest("the_long_winter")),
        ]
    );
}

#[test]
fn a_quest_under_way_shows_its_stage_and_a_stage_not_yet_reached_says_why() {
    let mut harness = Harness::new_ui_state(
        |ui, asked: &mut Vec<Asked>| {
            asked.extend(quests_ui(ui, &a_panel()));
        },
        Vec::new(),
    );
    harness.run();
    harness.get_by_label("Under way");
    harness.get_by_label("Ava's Lost Ring, at search");
    harness.get_by_label("it needs standing 10.00 with city_watch, and player has 5.00");
    assert!(
        harness
            .get_by_label("return_it")
            .accesskit_node()
            .is_disabled()
    );
    harness.get_by_label("pawn_it").click();
    harness.run();
    assert!(harness.state().is_empty(), "a disabled choice asks nothing");

    let mut open = a_panel();
    open.under_way[0].unmet.clear();
    let mut harness = Harness::new_ui_state(
        move |ui, asked: &mut Vec<Asked>| {
            asked.extend(quests_ui(ui, &open));
        },
        Vec::new(),
    );
    harness.run();
    harness.get_by_label("pawn_it").click();
    harness.run();
    assert_eq!(
        harness.state(),
        &[Asked::Choose(quest("lost_ring"), choice("pawn_it"))]
    );
}

#[test]
fn with_no_one_in_reach_or_no_one_offering_the_panel_says_so() {
    let mut nobody = a_panel();
    nobody.giver = None;
    nobody.offered.clear();
    let mut harness = Harness::new_ui(move |ui| {
        quests_ui(ui, &nobody);
    });
    harness.run();
    harness.get_by_label("No one in reach");
    let mut mira = a_panel();
    mira.giver = Some(id("sister_mira"));
    mira.offered.clear();
    let mut harness = Harness::new_ui(move |ui| {
        quests_ui(ui, &mira);
    });
    harness.run();
    harness.get_by_label("sister_mira offers no quests");
}
