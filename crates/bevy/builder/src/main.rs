//! The builder app (G4, P-125): `factional-builder <content dir>` opens the content's world in
//! a window, each character a token where `builder.toml` puts them, the inspector open beside
//! them, and time paused until it's stepped or run.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use factional_bevy::{Factional, FactionalPlugin};
use factional_bevy_inspector::{Inspector, InspectorPlugin};
use factional_builder::{BuilderPlugin, Clock, Token, controls_ui};

fn main() -> AppExit {
    let Some(content) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("usage: factional-builder <content dir>");
        return AppExit::error();
    };
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
        .add_plugins(BuilderPlugin { content })
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(Camera2d);
        })
        .add_systems(Update, draw_tokens)
        .add_systems(EguiPrimaryContextPass, controls)
        .run()
}

/// Each new token drawn: a disc, its character's id under it. Placeholder shapes (D-46).
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
                MeshMaterial2d(materials.add(Color::srgb(0.35, 0.66, 0.9))),
            ))
            .with_child((
                Text2d::new(token.character.as_str()),
                Transform::from_xyz(0.0, -28.0, 1.0),
            ));
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
