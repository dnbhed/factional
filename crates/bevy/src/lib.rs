//! The Bevy plugin (E1, D-33, P-75, DESIGN.md §19): a game's world and its quests as a
//! resource, loaded from a content directory at startup; commands in and events out as
//! messages, in the order sent, once a frame; and time, by `AdvanceTime`, at a rate the game
//! sets. Below it nothing knows Bevy exists.

use std::path::PathBuf;
use std::time::Duration;

use bevy::app::{App, AppExit, Plugin, PostUpdate, PreUpdate, Startup};
use bevy::ecs::message::{Message, MessageReader, MessageWriter};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Res, ResMut};
use bevy::time::Time;
use factional_quests::{Played, QuestCommand, QuestError, QuestEvent, QuestLog};
use factional_reputation::{Command, CommandError, Event, World};

/// The plugin: the content directory to load at startup, and how often a tick of game time
/// passes, if the plugin is to advance it.
pub struct FactionalPlugin {
    pub content: PathBuf,
    /// `None` leaves time to the game, which sends `AdvanceTime` itself.
    pub tick_every: Option<Duration>,
}

/// The world and the quest log, loaded at startup; queries are read straight from it.
#[derive(Resource)]
pub struct Factional {
    pub world: World,
    pub quests: QuestLog,
}

/// Every problem with content that didn't load, one a line, as `load` gives them; the app
/// stops.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct FactionalProblems(pub Vec<String>);

/// A command for the world or the quest log, sent by a game system.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub enum FactionalCommand {
    World(Command),
    Quest(QuestCommand),
}

/// What a command did, in order: each event, or that it was refused, and why.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub enum FactionalEvent {
    World(Event),
    Quest(QuestEvent),
    WorldRefused(Command, CommandError),
    QuestRefused(QuestCommand, QuestError),
}

impl From<Played> for FactionalEvent {
    /// What a quest command played, as the event a game is sent: to the quest log, or through
    /// its effects, to the world.
    fn from(played: Played) -> FactionalEvent {
        match played {
            Played::Quest(event) => FactionalEvent::Quest(event),
            Played::World(event) => FactionalEvent::World(event),
        }
    }
}

/// How often a tick passes, and how long since the last.
#[derive(Resource)]
struct Ticking {
    every: Duration,
    since: Duration,
}

impl Plugin for FactionalPlugin {
    /// Loads the content, as `load` would (D-20): with it, the world and quests as a
    /// resource, commands executed after the game's systems each frame, and time; without,
    /// every problem, and the app stopping.
    fn build(&self, app: &mut App) {
        app.add_message::<FactionalCommand>()
            .add_message::<FactionalEvent>();
        match load(&self.content) {
            Ok(factional) => {
                app.insert_resource(factional)
                    .add_systems(PostUpdate, execute);
                // No time passes at a rate of nothing.
                if let Some(every) = self.tick_every.filter(|every| !every.is_zero()) {
                    app.insert_resource(Ticking {
                        every,
                        since: Duration::ZERO,
                    })
                    .add_systems(PreUpdate, tick);
                }
            }
            Err(problems) => {
                for problem in &problems {
                    eprintln!("{problem}");
                }
                app.insert_resource(FactionalProblems(problems))
                    .add_systems(Startup, |mut exit: MessageWriter<AppExit>| {
                        exit.write(AppExit::error());
                    });
            }
        }
    }
}

/// The world and quests in `content`, or every problem with them, as `load` gives them.
fn load(content: &std::path::Path) -> Result<Factional, Vec<String>> {
    let said = |problems: Vec<String>| problems;
    let (content, quests) = factional_content::load_quests(content)
        .map_err(|error| said(error.diagnostics.iter().map(ToString::to_string).collect()))?;
    let world = World::new(content)
        .map_err(|problems| said(problems.iter().map(ToString::to_string).collect()))?;
    Ok(Factional {
        world,
        quests: QuestLog::new(quests),
    })
}

/// Each command sent this frame, in order: executed, and each event, or its refusal, sent
/// out.
fn execute(
    mut factional: ResMut<Factional>,
    mut commands: MessageReader<FactionalCommand>,
    mut out: MessageWriter<FactionalEvent>,
) {
    // Untouched on a frame with nothing sent, so a game watching it isn't told it changed.
    if commands.is_empty() {
        return;
    }
    let Factional { world, quests } = &mut *factional;
    for command in commands.read() {
        match command.clone() {
            FactionalCommand::World(command) => match world.execute(command.clone()) {
                Ok(events) => {
                    out.write_batch(events.into_iter().map(FactionalEvent::World));
                }
                Err(refused) => {
                    out.write(FactionalEvent::WorldRefused(command, refused));
                }
            },
            FactionalCommand::Quest(command) => match quests.execute(world, command.clone()) {
                Ok(played) => {
                    out.write_batch(played.into_iter().map(FactionalEvent::from));
                }
                Err(refused) => {
                    out.write(FactionalEvent::QuestRefused(command, refused));
                }
            },
        }
    }
}

/// The ticks that have passed in the game's time since the last frame, sent as one
/// `AdvanceTime` before the game's systems run.
fn tick(
    time: Res<Time>,
    mut ticking: ResMut<Ticking>,
    mut commands: MessageWriter<FactionalCommand>,
) {
    let every = ticking.every.as_nanos();
    let since = ticking.since.as_nanos() + time.delta().as_nanos();
    // What's left over counts toward the next tick.
    ticking.since = Duration::from_nanos(u64::try_from(since % every).unwrap_or(u64::MAX));
    let ticks = u64::try_from(since / every).unwrap_or(u64::MAX);
    if ticks > 0 {
        commands.write(FactionalCommand::World(Command::AdvanceTime { ticks }));
    }
}
