//! The plugin (E1, P-75): the world loaded at startup, commands in and events out as
//! messages, time at the game's rate, and content that doesn't load stopping the app.

use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::MinimalPlugins;
use bevy::app::{App, AppExit};
use bevy::ecs::message::Messages;
use bevy::time::TimeUpdateStrategy;
use factional_bevy::{
    Factional, FactionalCommand, FactionalEvent, FactionalPlugin, FactionalProblems,
};
use factional_core::{Fixed, Tick};
use factional_quests::{QuestCommand, QuestEvent, QuestId};
use factional_reputation::{ActionId, Change, CharacterId, Command, Witnesses};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn content(dir: &str) -> PathBuf {
    Path::new(REPO).join(dir)
}

fn app(dir: &str, tick_every: Option<Duration>) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(FactionalPlugin {
            content: content(dir),
            tick_every,
        });
    app
}

fn id(text: &str) -> CharacterId {
    CharacterId::new(text).expect("a valid id")
}

/// Sends `command`, runs a frame, and gives what came out.
fn send(app: &mut App, command: FactionalCommand) -> Vec<FactionalEvent> {
    app.world_mut().write_message(command);
    app.update();
    let messages = app.world().resource::<Messages<FactionalEvent>>();
    messages.iter_current_update_messages().cloned().collect()
}

fn steal() -> Command {
    Command::PerformAction {
        actor: id("player"),
        action: ActionId::new("steal").expect("a valid id"),
        target: Some(id("merchant_ava")),
        scale: Fixed::ONE,
        witnesses: Witnesses::Everyone,
    }
}

#[test]
fn the_world_is_loaded_at_startup_as_a_resource() {
    let mut app = app("content/sample", None);
    app.update();
    let factional = app.world().resource::<Factional>();
    assert_eq!(factional.world.characters().count(), 6);
    assert!(!factional.quests.quests().quests.is_empty());
}

#[test]
fn a_command_sent_is_executed_and_its_events_come_out() {
    let mut app = app("content/sample", None);
    app.update();
    let out = send(&mut app, FactionalCommand::World(steal()));
    let FactionalEvent::World(first) = &out[0] else {
        panic!("the world's events: {out:?}");
    };
    assert!(
        matches!(first.payload, Change::ActionPerformed { .. }),
        "{first:?}"
    );
    let world = &app.world().resource::<Factional>().world;
    assert_eq!(world.journal().len(), 1);
}

#[test]
fn a_refused_command_says_why_and_changes_nothing() {
    let mut app = app("content/sample", None);
    app.update();
    let Command::PerformAction {
        action,
        target,
        scale,
        witnesses,
        ..
    } = steal()
    else {
        unreachable!();
    };
    let nobody = Command::PerformAction {
        actor: id("nobody"),
        action,
        target,
        scale,
        witnesses,
    };
    let out = send(&mut app, FactionalCommand::World(nobody.clone()));
    assert!(
        matches!(&out[..], [FactionalEvent::WorldRefused(command, _)] if *command == nobody),
        "{out:?}"
    );
}

#[test]
fn a_quest_command_plays_through_the_quest_log() {
    let mut app = app("content/sample", None);
    app.update();
    let start = QuestCommand::StartQuest {
        character: id("player"),
        quest: QuestId::new("watch_oath").expect("a valid id"),
    };
    let out = send(&mut app, FactionalCommand::Quest(start));
    assert!(
        out.iter().any(|event| matches!(
            event,
            FactionalEvent::Quest(QuestEvent::QuestStarted { quest, .. }) if quest.as_str() == "watch_oath"
        )),
        "{out:?}"
    );
}

#[test]
fn time_passes_at_the_rate_the_game_sets() {
    let mut app = app("content/sample", Some(Duration::from_secs(1)));
    // Virtual time moves at most 250 ms a frame, so the frames are shorter than that.
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        200,
    )));
    for _ in 0..16 {
        app.update();
        // A frame in which no tick passed sends no time: nothing is refused.
        let messages = app.world().resource::<Messages<FactionalEvent>>();
        let refused = messages
            .iter_current_update_messages()
            .filter(|event| matches!(event, FactionalEvent::WorldRefused(..)))
            .count();
        assert_eq!(refused, 0);
    }
    let elapsed = app
        .world()
        .resource::<bevy::time::Time>()
        .elapsed()
        .as_secs();
    assert!(elapsed >= 2, "{elapsed}");
    let world = &app.world().resource::<Factional>().world;
    assert_eq!(world.now(), Tick(elapsed));
}

#[test]
fn without_a_rate_time_is_the_games_to_send() {
    let mut app = app("content/sample", None);
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        500,
    )));
    for _ in 0..8 {
        app.update();
    }
    assert_eq!(app.world().resource::<Factional>().world.now(), Tick(0));
    send(
        &mut app,
        FactionalCommand::World(Command::AdvanceTime { ticks: 3 }),
    );
    assert_eq!(app.world().resource::<Factional>().world.now(), Tick(3));
}

#[test]
fn content_that_doesnt_load_stops_the_app_saying_why() {
    let mut app = app("crates/cli/tests/fixtures/worlds/broken", None);
    app.update();
    assert_eq!(app.should_exit(), Some(AppExit::error()));
    let problems = app.world().resource::<FactionalProblems>();
    assert!(
        problems
            .0
            .contains(&"characters.toml: hale: missing 'alignment'".to_owned()),
        "{problems:?}"
    );
    assert!(app.world().get_resource::<Factional>().is_none());
}

#[test]
fn a_frame_with_no_commands_leaves_the_world_unchanged() {
    let mut app = app("content/sample", None);
    app.update();
    let changed = |app: &App| {
        app.world()
            .get_resource_change_ticks::<Factional>()
            .expect("the world")
            .changed
    };
    let before = changed(&app);
    app.update();
    app.update();
    assert_eq!(
        changed(&app),
        before,
        "nothing was sent, so nothing changed"
    );
    send(&mut app, FactionalCommand::World(steal()));
    assert_ne!(changed(&app), before, "a command sent changes it");
}
