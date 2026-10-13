//! The builder app (G4, G5, P-125, P-126): `factional-builder <content dir> [--as <id>]`
//! opens the content's world in a window, each character a token where `builder.toml` puts
//! them, the inspector open beside them, and time paused until it's stepped or run. The
//! played character, `player` unless `--as` names another, walks by the arrow keys or WASD
//! and acts on the nearest from the Act window; each other token shows how they regard them
//! (G6a, P-127).

use std::path::PathBuf;

use bevy::prelude::*;
use bevy_egui::input::EguiWantsInput;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use factional_bevy::{Factional, FactionalCommand, FactionalPlugin};
use factional_bevy_inspector::{Inspector, InspectorPlugin};
use factional_builder::{
    BuilderPlugin, Clock, KeyboardTaken, Nearby, Played, Regards, Token, act, act_ui, controls_ui,
};
use factional_egui::relations::tint;
use factional_reputation::ActionId;

/// The colours of tokens: the one played, and anyone else before their band's tint.
const PLAYED: Color = Color::srgb(0.45, 0.8, 0.45);
const NEUTRAL: Color = Color::srgb(0.55, 0.57, 0.6);
const RING: Color = Color::WHITE;

/// A token's label, and the ring shown while it's nearest.
#[derive(Component)]
struct Parts {
    label: Entity,
    ring: Entity,
}

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

/// Each new token drawn: a disc of its own colour, its label under it, and a ring behind it
/// for when it's nearest. Placeholder shapes (D-46).
fn draw_tokens(
    mut commands: Commands,
    added: Query<(Entity, &Token), Added<Token>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for (entity, token) in &added {
        let label = commands
            .spawn((
                Text2d::new(token.character.as_str()),
                Transform::from_xyz(0.0, -34.0, 1.0),
            ))
            .id();
        let ring = commands
            .spawn((
                Mesh2d(meshes.add(Circle::new(19.0))),
                MeshMaterial2d(materials.add(RING)),
                Transform::from_xyz(0.0, 0.0, -0.1),
                Visibility::Hidden,
            ))
            .id();
        commands
            .entity(entity)
            .insert((
                Mesh2d(meshes.add(Circle::new(14.0))),
                MeshMaterial2d(materials.add(NEUTRAL)),
                Parts { label, ring },
            ))
            .add_children(&[label, ring]);
    }
}

/// The played token green; anyone else by how they regard it, tinted by band as the
/// relations matrix is, and labelled with it; the nearest ringed.
fn tint_tokens(
    nearby: Res<Nearby>,
    regards: Res<Regards>,
    tokens: Query<(&Token, &MeshMaterial2d<ColorMaterial>, &Parts, Has<Played>)>,
    mut labels: Query<&mut Text2d>,
    mut rings: Query<&mut Visibility>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for (token, material, parts, played) in &tokens {
        let regard = regards.0.get(&token.character);
        let colour = match (played, regard) {
            (true, _) => PLAYED,
            (false, Some(regard)) => band_colour(regard.index, regard.bands),
            (false, None) => NEUTRAL,
        };
        if let Some(mut material) = materials.get_mut(&material.0) {
            material.color = colour;
        }
        if let Ok(mut label) = labels.get_mut(parts.label) {
            let said = match regard {
                Some(regard) => format!("{}\n{}", token.character, regard.said()),
                None => token.character.to_string(),
            };
            if label.0 != said {
                label.0 = said;
            }
        }
        if let Ok(mut ring) = rings.get_mut(parts.ring) {
            let nearest = nearby.nearest.as_ref() == Some(&token.character);
            ring.set_if_neq(if nearest {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
    }
}

/// A band's colour on a disc: neutral grey, toward the matrix's tint as strongly as it's
/// tinted there.
fn band_colour(index: usize, bands: usize) -> Color {
    match tint(index, bands) {
        None => NEUTRAL,
        Some(tint) => {
            let [r, g, b, a] = tint.to_srgba_unmultiplied();
            let strength = (f32::from(a) / 160.0).min(1.0);
            NEUTRAL.mix(&Color::srgb_u8(r, g, b), strength)
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
