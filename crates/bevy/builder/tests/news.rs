//! News drawn as it goes (G6b): each token about to hear it, how far it's come, and a pulse
//! as it lands. DESIGN.md §10.2's example: the player picks Captain Hale's pocket at tick 0,
//! seen only by Merchant Ava; hops take 10 ticks.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::time::{Time, TimeUpdateStrategy, Virtual};
use factional_bevy::{FactionalCommand, FactionalPlugin};
use factional_builder::{BuilderPlugin, Clock, Hearing, PULSE, Spreading, hearing};
use factional_core::{Fixed, Tick};
use factional_reputation::{ActionId, CharacterId, Command, Witnesses, World};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../..");

fn sample() -> PathBuf {
    Path::new(REPO).join("content/sample")
}

fn id(text: &str) -> CharacterId {
    CharacterId::new(text).expect("an id")
}

fn riverhold() -> World {
    let (content, _) = factional_content::load_quests(&sample()).expect("loads");
    World::new(content).expect("complete")
}

fn pickpocket() -> Command {
    Command::PerformAction {
        actor: id("player"),
        action: ActionId::new("steal").expect("an id"),
        target: Some(id("captain_hale")),
        scale: Fixed::ONE,
        witnesses: Witnesses::These(BTreeSet::from([id("merchant_ava")])),
    }
}

fn hop(from: u64, at: u64) -> Hearing {
    Hearing {
        from: Tick(from),
        at: Tick(at),
    }
}

#[test]
fn news_unseen_is_on_its_way_to_whom_it_will_reach_and_when() {
    let mut world = riverhold();
    assert!(hearing(&world).is_empty());
    world.execute(pickpocket()).expect("steals");
    let found = hearing(&world);
    assert_eq!(
        found.into_iter().collect::<Vec<_>>(),
        [
            (id("captain_hale"), hop(0, 10)),
            (id("sister_mira"), hop(0, 10))
        ]
    );
    assert_eq!(hop(0, 10).said(Tick(0)), "news in 10 ticks");
    assert_eq!(hop(0, 10).said(Tick(9)), "news in 1 tick");
    world
        .execute(Command::AdvanceTime { ticks: 10 })
        .expect("time passes");
    assert_eq!(
        hearing(&world).into_iter().collect::<Vec<_>>(),
        [(id("brother_ash"), hop(10, 20))]
    );
    world
        .execute(Command::AdvanceTime { ticks: 10 })
        .expect("time passes");
    assert!(hearing(&world).is_empty(), "nothing more on its way");
}

#[test]
fn how_far_it_has_come_is_the_ticks_gone_and_the_part_of_one_under_way() {
    let coming = hop(0, 10);
    assert_eq!(coming.progress(Tick(0), 0.0), 0.0);
    assert_eq!(coming.progress(Tick(5), 0.0), 0.5);
    assert!((coming.progress(Tick(5), 0.5) - 0.55).abs() < 0.0001);
    assert_eq!(coming.progress(Tick(10), 0.0), 1.0);
    assert_eq!(coming.progress(Tick(12), 0.9), 1.0, "never past the end");
}

#[test]
fn of_two_pieces_of_news_on_their_way_the_soonest_is_shown() {
    let mut world = riverhold();
    world.execute(pickpocket()).expect("steals");
    world
        .execute(Command::AdvanceTime { ticks: 5 })
        .expect("time passes");
    world.execute(pickpocket()).expect("steals again");
    assert_eq!(hearing(&world)[&id("captain_hale")], hop(0, 10));
}

/// The builder on the sample, `frame` of game time a frame.
fn app(frame: Duration) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(frame))
        .add_plugins(FactionalPlugin {
            content: sample(),
            tick_every: None,
        })
        .add_plugins(BuilderPlugin {
            content: sample(),
            player: "player".to_owned(),
        });
    app.world_mut()
        .resource_mut::<Time<Virtual>>()
        .set_max_delta(Duration::from_secs(10));
    app.update();
    app
}

fn send(app: &mut App, command: Command) {
    app.world_mut()
        .write_message(FactionalCommand::World(command));
    app.update();
}

#[test]
fn as_news_lands_its_hearers_pulse_for_a_second_and_the_next_hop_is_on_its_way() {
    let mut app = app(Duration::from_millis(500));
    send(&mut app, pickpocket());
    let spreading = app.world().resource::<Spreading>().clone();
    assert_eq!(
        spreading.hearing.keys().cloned().collect::<Vec<_>>(),
        [id("captain_hale"), id("sister_mira")]
    );
    assert!(spreading.pulsing.is_empty(), "Ava saw it; no news landed");
    send(&mut app, Command::AdvanceTime { ticks: 10 });
    let spreading = app.world().resource::<Spreading>().clone();
    assert_eq!(
        spreading.pulsing.keys().cloned().collect::<Vec<_>>(),
        [id("captain_hale"), id("sister_mira")]
    );
    assert_eq!(spreading.pulsing[&id("captain_hale")], PULSE);
    assert_eq!(
        spreading.hearing.keys().cloned().collect::<Vec<_>>(),
        [id("brother_ash")]
    );
    app.update();
    let spreading = app.world().resource::<Spreading>().clone();
    assert_eq!(
        spreading.pulsing[&id("captain_hale")],
        Duration::from_millis(500)
    );
    app.update();
    assert!(
        app.world().resource::<Spreading>().pulsing.is_empty(),
        "a second on, it's settled"
    );
}

#[test]
fn the_part_of_a_tick_under_way_is_the_clocks_while_time_runs() {
    let mut app = app(Duration::from_millis(500));
    assert_eq!(app.world().resource::<Clock>().part(), 0.0);
    app.world_mut().resource_mut::<Clock>().run();
    app.update();
    assert_eq!(app.world().resource::<Clock>().part(), 0.5);
    app.world_mut().resource_mut::<Clock>().pause();
    assert_eq!(app.world().resource::<Clock>().part(), 0.0);
}
