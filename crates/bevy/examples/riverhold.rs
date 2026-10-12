//! A game of a few frames on Riverhold (E1): the player steals from Ava and starts the Watch's
//! Oath, a tick passes every frame, and each event the world sends out is printed. Headless,
//! so it runs anywhere: `cargo run --example riverhold` in `crates/bevy`.

use std::path::Path;
use std::time::Duration;

use bevy::MinimalPlugins;
use bevy::app::{App, AppExit, ScheduleRunnerPlugin, Startup, Update};
use bevy::diagnostic::FrameCount;
use bevy::ecs::message::{MessageReader, MessageWriter};
use bevy::ecs::system::Res;
use bevy::prelude::PluginGroup;
use factional_bevy::{FactionalCommand, FactionalEvent, FactionalPlugin};
use factional_core::Fixed;
use factional_quests::{QuestCommand, QuestId};
use factional_reputation::{ActionId, CharacterId, Command, Witnesses};

fn main() {
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/sample");
    App::new()
        .add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(100))))
        .add_plugins(FactionalPlugin {
            content,
            tick_every: Some(Duration::from_millis(100)),
        })
        .add_systems(Startup, play)
        .add_systems(Update, (show, stop))
        .run();
}

/// The player's first moves, sent as messages.
fn play(mut commands: MessageWriter<FactionalCommand>) {
    let id = |text: &str| CharacterId::new(text).expect("a valid id");
    commands.write(FactionalCommand::World(Command::PerformAction {
        actor: id("player"),
        action: ActionId::new("steal").expect("a valid id"),
        target: Some(id("merchant_ava")),
        scale: Fixed::ONE,
        witnesses: Witnesses::Everyone,
    }));
    commands.write(FactionalCommand::Quest(QuestCommand::StartQuest {
        character: id("player"),
        quest: QuestId::new("watch_oath").expect("a valid id"),
    }));
}

/// Each event as the world sent it.
fn show(mut events: MessageReader<FactionalEvent>) {
    for event in events.read() {
        println!("{event:?}");
    }
}

/// Five frames are enough to see it work.
fn stop(frames: Res<FrameCount>, mut exit: MessageWriter<AppExit>) {
    if frames.0 >= 5 {
        exit.write(AppExit::Success);
    }
}
