//! The builder app (G4, G5, P-125, P-126): `factional-builder <content dir> [--as <id>]`
//! opens the content's world in a window, each character a token where `builder.toml` puts
//! them, the inspector open beside them, and time paused until it's stepped or run. The
//! played character, `player` unless `--as` names another, walks by the arrow keys or WASD
//! and acts on the nearest from the Act window.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy_egui::input::EguiWantsInput;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use factional_bevy::{Factional, FactionalCommand, FactionalPlugin};
use factional_bevy_inspector::{Inspector, InspectorPlugin};
use factional_builder::{
    BuilderPlugin, Clock, KeyboardTaken, Nearby, Played, Token, act, act_ui, controls_ui,
};
use factional_reputation::ActionId;

/// The colours of tokens: anyone, the one played, and the nearest to them.
const ANYONE: Color = Color::srgb(0.35, 0.66, 0.9);
const PLAYED: Color = Color::srgb(0.45, 0.8, 0.45);
const NEAREST: Color = Color::srgb(0.94, 0.64, 0.37);

fn main() -> AppExit {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (content, player) = match &args[..] {
        [content] => (content, "player"),
        [content, flag, player] if flag == "--as" => (content, player.as_str()),
        _ => {
            eprintln!("usage: factional-builder <content dir> [--as <character>]");
            return AppExit::error();
        }
    };
    let content = PathBuf::from(content);
    let window = Window {
        title: format!("Factional builder: {}", content.display()),
        ..default()
    };
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(window),
            ..default()
        }))
        .add_plugins(FactionalPlugin {
            content: content.clone(),
            tick_every: None,
        })
        .add_plugins(InspectorPlugin::default())
        .insert_resource(Inspector { open: true })
        .add_plugins(BuilderPlugin {
            content,
            player: player.to_owned(),
        })
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(Camera2d);
        })
        .add_systems(PreUpdate, keyboard_taken)
        .add_systems(Update, (draw_tokens, tint_tokens).chain())
        .add_systems(EguiPrimaryContextPass, (controls, act_window))
        .run()
}

/// The keys don't walk while egui has the keyboard, as when typing in the console.
fn keyboard_taken(wants: Res<EguiWantsInput>, mut taken: ResMut<KeyboardTaken>) {
    taken.0 = wants.wants_any_keyboard_input();
}

/// Each new token drawn: a disc of its own colour, its character's id under it. Placeholder
/// shapes (D-46).
fn draw_tokens(
    mut commands: Commands,
    added: Query<(Entity, &Token), Added<Token>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for (entity, token) in &added {
        commands
            .entity(entity)
            .insert((
                Mesh2d(meshes.add(Circle::new(14.0))),
                MeshMaterial2d(materials.add(ANYONE)),
            ))
            .with_child((
                Text2d::new(token.character.as_str()),
                Transform::from_xyz(0.0, -28.0, 1.0),
            ));
    }
}

/// The played token and the nearest to it in their colours, everyone else in theirs.
fn tint_tokens(
    nearby: Res<Nearby>,
    tokens: Query<(&Token, &MeshMaterial2d<ColorMaterial>, Has<Played>)>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for (token, material, played) in &tokens {
        let colour = match (played, nearby.nearest.as_ref() == Some(&token.character)) {
            (true, _) => PLAYED,
            (false, true) => NEAREST,
            (false, false) => ANYONE,
        };
        if let Some(mut material) = materials.get_mut(&material.0) {
            material.color = colour;
        }
    }
}

/// The controls, at the top left.
fn controls(
    mut contexts: EguiContexts,
    mut clock: ResMut<Clock>,
    factional: Option<Res<Factional>>,
) -> Result {
    let Some(factional) = factional else {
        return Ok(());
    };
    egui::Window::new("Builder")
        .anchor(egui::Align2::LEFT_TOP, [8.0, 8.0])
        .title_bar(false)
        .resizable(false)
        .show(contexts.ctx_mut()?, |ui| {
            controls_ui(ui, &mut clock, factional.world.now());
        });
    Ok(())
}

/// The Act window, at the bottom left: the content's actions, done by the played character on
/// the nearest when chosen.
fn act_window(
    mut contexts: EguiContexts,
    nearby: Res<Nearby>,
    played: Query<&Token, With<Played>>,
    factional: Option<Res<Factional>>,
    mut out: MessageWriter<FactionalCommand>,
) -> Result {
    let (Some(factional), Ok(played)) = (factional, played.single()) else {
        return Ok(());
    };
    let actions: Vec<ActionId> = factional
        .world
        .actions()
        .map(|action| action.id.clone())
        .collect();
    egui::Window::new("Act")
        .anchor(egui::Align2::LEFT_BOTTOM, [8.0, -8.0])
        .resizable(false)
        .show(contexts.ctx_mut()?, |ui| {
            if let Some(action) = act_ui(ui, &actions, &nearby) {
                out.write(act(&played.character, action, &nearby));
            }
        });
    Ok(())
}
