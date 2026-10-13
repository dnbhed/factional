//! The game builder (G4, D-43 to D-47, P-118, P-125, DESIGN.md §20): content played as a
//! game would play it. Its shell: each character a token on a 2D plane, placed from
//! `builder.toml` beside the content or laid out on a ring; and time, paused, stepped or run.
//! The builder is a game: it uses the plugin and the inspector exactly as a game would.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use bevy::app::{App, Plugin, PreUpdate, Startup, Update};
use bevy::ecs::component::Component;
use bevy::ecs::message::MessageWriter;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Res, ResMut};
use bevy::math::Vec2;
use bevy::time::Time;
use bevy::transform::components::Transform;
use bevy_egui::egui;
use factional_bevy::{Factional, FactionalCommand};
use factional_core::{Tick, suggest};
use factional_reputation::{CharacterId, Command};

/// How far from the middle the ring of characters not placed is, in points.
pub const RING: f32 = 240.0;

/// How much game time a tick is, while time runs.
const TICK_EVERY: Duration = Duration::from_secs(1);

pub mod play;

pub use play::{KeyboardTaken, Nearby, Play, Played, act, act_ui, nearby};

/// The builder, on the content in `content`, whose `builder.toml` it reads, playing the
/// character `player` names.
pub struct BuilderPlugin {
    pub content: PathBuf,
    pub player: String,
}

/// A character's token on the plane.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub character: CharacterId,
}

/// What was wrong with `builder.toml`, a line each, as the loader would say it.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct BuilderProblems(pub Vec<String>);

/// The world's time in the builder: paused, or running a tick a second, and steps asked for.
#[derive(Resource, Debug, Default)]
pub struct Clock {
    running: bool,
    /// Game time toward the next tick, while running.
    since: Duration,
    /// Ticks asked for by Step, to be sent next frame.
    steps: u64,
}

impl Clock {
    /// Whether time is running.
    pub fn running(&self) -> bool {
        self.running
    }

    /// Runs time, a tick a second.
    pub fn run(&mut self) {
        self.running = true;
    }

    /// Pauses time.
    pub fn pause(&mut self) {
        self.running = false;
        self.since = Duration::ZERO;
    }

    /// Advances time a tick, next frame.
    pub fn step(&mut self) {
        self.steps += 1;
    }
}

/// Where each character stands, and what was wrong with `builder.toml`, if anything.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Placed {
    pub positions: Vec<(CharacterId, Vec2)>,
    /// How near is near.
    pub play: Play,
    pub problems: Vec<String>,
}

/// `builder.toml`: where characters stand, by id.
#[derive(serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct BuilderFile {
    #[serde(default)]
    positions: BTreeMap<String, Position>,
    #[serde(default)]
    play: PlayFile,
}

/// `builder.toml`'s `[play]`: how near is near, each left out for its default.
#[derive(serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct PlayFile {
    reach: Option<f32>,
    sight: Option<f32>,
    speed: Option<f32>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Position {
    x: f32,
    y: f32,
}

/// Places `characters`, in their order, from `file`, the text of `builder.toml` if there is
/// one: each where the file puts them, or on a ring around the middle, in order, the first
/// at the top and the rest clockwise, evenly spaced. A position for no character is said, as
/// the loader would say it, and left out; a file that can't be read is said, and everyone
/// laid out.
pub fn place(file: Option<&str>, characters: &[CharacterId]) -> Placed {
    let mut problems = Vec::new();
    let read = match file.map(toml::from_str::<BuilderFile>) {
        None => BuilderFile::default(),
        Some(Ok(read)) => read,
        Some(Err(error)) => {
            let said = error.message().trim_end_matches('.').to_owned();
            problems.push(format!("builder.toml: {said}, so everyone is laid out"));
            BuilderFile::default()
        }
    };
    for named in read.positions.keys() {
        if !characters
            .iter()
            .any(|character| character.as_str() == named)
        {
            let hint = suggest(named, characters.iter().map(CharacterId::as_str))
                .map(|close| format!(" (did you mean '{close}'?)"))
                .unwrap_or_default();
            problems.push(format!(
                "builder.toml: positions.{named} names no character{hint}, so it's left out"
            ));
        }
    }
    let mut play = Play::default();
    for (name, read, kept) in [
        ("reach", read.play.reach, &mut play.reach),
        ("sight", read.play.sight, &mut play.sight),
        ("speed", read.play.speed, &mut play.speed),
    ] {
        match read {
            Some(value) if value > 0.0 => *kept = value,
            Some(_) => problems.push(format!(
                "builder.toml: play.{name} must be above 0, so it's {kept}"
            )),
            None => {}
        }
    }
    let count = characters.len().max(1) as f32;
    let positions = characters
        .iter()
        .enumerate()
        .map(|(index, character)| {
            let at = match read.positions.get(character.as_str()) {
                Some(Position { x, y }) => Vec2::new(*x, *y),
                None => {
                    let turn = std::f32::consts::TAU * index as f32 / count;
                    Vec2::new(turn.sin(), turn.cos()) * RING
                }
            };
            (character.clone(), at)
        })
        .collect();
    Placed {
        positions,
        play,
        problems,
    }
}

impl Plugin for BuilderPlugin {
    /// Reads `builder.toml`, if it's there, to place the world's characters at startup; and
    /// keeps time, paused to begin with.
    fn build(&self, app: &mut App) {
        let file = std::fs::read_to_string(self.content.join("builder.toml")).ok();
        app.insert_resource(Layout(file))
            .insert_resource(PlayAs(self.player.clone()))
            .init_resource::<BuilderProblems>()
            .init_resource::<Clock>()
            .init_resource::<Play>()
            .init_resource::<Nearby>()
            .init_resource::<KeyboardTaken>()
            .add_systems(Startup, spawn_tokens)
            .add_systems(PreUpdate, keep_time)
            .add_systems(Update, (play::walk, play::find_nearby).chain());
    }
}

/// The text of `builder.toml`, as read when the builder was built.
#[derive(Resource)]
struct Layout(Option<String>);

/// Who the builder plays, as `--as` named them.
#[derive(Resource)]
struct PlayAs(String);

/// Each of the world's characters, a token where `builder.toml` puts them, the played one
/// marked; and how near is near. Its problems said.
fn spawn_tokens(
    mut commands: Commands,
    file: Res<Layout>,
    play_as: Res<PlayAs>,
    factional: Option<Res<Factional>>,
    mut problems: ResMut<BuilderProblems>,
    mut play: ResMut<Play>,
) {
    let Some(factional) = factional else {
        return;
    };
    let characters: Vec<CharacterId> = factional
        .world
        .characters()
        .map(|character| character.id.clone())
        .collect();
    let mut placed = place(file.0.as_deref(), &characters);
    if !characters
        .iter()
        .any(|character| character.as_str() == play_as.0)
    {
        let hint = suggest(&play_as.0, characters.iter().map(CharacterId::as_str))
            .map(|close| format!(" (did you mean '{close}'?)"))
            .unwrap_or_default();
        placed.problems.push(format!(
            "--as {} names no character{hint}, so no one is played",
            play_as.0
        ));
    }
    for problem in &placed.problems {
        eprintln!("{problem}");
    }
    problems.0 = placed.problems;
    *play = placed.play;
    for (character, at) in placed.positions {
        let played = character.as_str() == play_as.0;
        let mut token = commands.spawn((
            Token { character },
            Transform::from_translation(at.extend(0.0)),
        ));
        if played {
            token.insert(Played);
        }
    }
}

/// The ticks asked for by Step, and while running, those that have passed in game time; sent
/// as one `AdvanceTime` before the game's systems run.
fn keep_time(time: Res<Time>, mut clock: ResMut<Clock>, mut out: MessageWriter<FactionalCommand>) {
    let mut ticks = std::mem::take(&mut clock.steps);
    if clock.running {
        let since = clock.since + time.delta();
        let every = TICK_EVERY.as_nanos();
        ticks += u64::try_from(since.as_nanos() / every).unwrap_or(u64::MAX);
        clock.since =
            Duration::from_nanos(u64::try_from(since.as_nanos() % every).unwrap_or(u64::MAX));
    }
    if ticks > 0 {
        out.write(FactionalCommand::World(Command::AdvanceTime { ticks }));
    }
}

/// The controls: whether time is paused or running, the tick, Run or Pause, and Step.
pub fn controls_ui(ui: &mut egui::Ui, clock: &mut Clock, now: Tick) {
    ui.horizontal(|ui| {
        ui.strong(if clock.running() { "Running" } else { "Paused" });
        ui.label(format!("Tick {}", now.0));
        if clock.running() {
            if ui.button("Pause").clicked() {
                clock.pause();
            }
        } else if ui.button("Run").clicked() {
            clock.run();
        }
        if ui.button("Step").clicked() {
            clock.step();
        }
    });
}
