//! The builder app (G4, G5, P-125, P-126): `factional-builder <content dir> [--as <id>]`
//! opens the content's world in a window, each character a token where `builder.toml` puts
//! them, the inspector open beside them, and time paused until it's stepped or run. The
//! played character, `player` unless `--as` names another, walks by the arrow keys or WASD
//! and acts on the nearest from the Act window; each other token shows how they regard them
//! (G6a, P-127), and news coming to them, landing with a pulse (G6b, P-128). A token offering
//! a quest that can start now is marked, and quests are started and chosen from the Quests
//! window (G7, P-129). "Edit in the editor" opens the editor on the same directory, at the
//! character in view (G8, P-130). "Arrange" turns on dragging the tokens, each drag
//! undoable, and saves where they stand to `builder.toml` (G9, P-131). The map
//! `builder.toml` names lies under them, its terrain autotiled on a dual grid, panned with the
//! right or middle button and zoomed with the wheel; Home brings the view back to the played
//! character (W7a, P-150). Its settlements, routes and points of interest are drawn on it,
//! named, but those dismissed; the one under the pointer is said as `factional world
//! suggestions` says it (W7b, P-151).

use std::path::PathBuf;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::sprite_render::{AlphaMode2d, TileData, TilemapChunk, TilemapChunkTileData};
use bevy_egui::input::EguiWantsInput;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use factional_bevy::{Factional, FactionalCommand, FactionalPlugin};
use factional_bevy_inspector::{Inspector, InspectorPlugin};
use factional_builder::{
    Arranging, BuilderPlugin, Clock, ContentDir, Givers, Ground, Hovered, KeyboardTaken, Nearby,
    PULSE, Played, Pointer, Regards, SHAPE_SIDE, Shape, Spreading, Suggested, TOKEN_RADIUS, Token,
    View, act, act_ui, arrange_ui, ask, colour, controls_ui, edit_args, edit_ui, panel, patches,
    quests_ui, tileset,
};
use factional_egui::open::{Lookup, Other, open};
use factional_egui::relations::tint;
use factional_reputation::ActionId;
use factional_worldgen::RouteKind;
use factional_worldgen::{CHUNK, Terrain};

/// The colours of tokens: the one played, and anyone else before their band's tint.
const PLAYED: Color = Color::srgb(0.45, 0.8, 0.45);
const NEUTRAL: Color = Color::srgb(0.55, 0.57, 0.6);
const RING: Color = Color::WHITE;
const NEWS: Color = Color::srgb(0.98, 0.83, 0.35);
const QUEST: Color = Color::srgb(1.0, 0.9, 0.2);

/// A token's label, its quest marker, the ring shown while it's nearest, and the arc of news
/// coming to it, with the arc's mesh, regrown as the news comes.
#[derive(Component)]
struct Parts {
    label: Entity,
    marker: Entity,
    ring: Entity,
    arc: Entity,
    arc_mesh: Handle<Mesh>,
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
        .add_systems(PreUpdate, (keyboard_taken, pointer))
        .add_systems(Update, (draw_tokens, show_tokens).chain())
        .add_systems(Update, (draw_ground, draw_suggestions, aim_camera))
        .add_systems(
            EguiPrimaryContextPass,
            (controls, act_window, quests_window, hover_tip),
        )
        .run()
}

/// How far under the tokens the map's first layer lies, and how far apart its layers are.
const GROUND_DEPTH: f32 = -10.0;
const LAYER_GAP: f32 = 0.01;

/// The map, once it's read: each chunk's layers a `TilemapChunk` of the 16 shapes tinted by
/// its terrain's colour, a tile a unit, scaled to the tile's points, under the tokens.
fn draw_ground(
    mut commands: Commands,
    ground: Option<Res<Ground>>,
    mut images: ResMut<Assets<Image>>,
) -> Result {
    let Some(ground) = ground.filter(|ground| ground.is_added()) else {
        return Ok(());
    };
    let mut shapes = Image::new(
        Extent3d {
            width: SHAPE_SIDE,
            height: SHAPE_SIDE * 16,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        tileset(SHAPE_SIDE),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    shapes.reinterpret_stacked_2d_as_array(16)?;
    let shapes = images.add(shapes);
    let size = UVec2::splat(CHUNK);
    for patch in patches(&ground) {
        let [r, g, b] = colour(patch.terrain);
        let tiles = patch
            .shapes
            .iter()
            .map(|shape| {
                shape.map(|shape| TileData {
                    tileset_index: u16::from(shape),
                    color: Color::srgb_u8(r, g, b),
                    ..default()
                })
            })
            .collect();
        let layer = Terrain::ALL
            .iter()
            .position(|terrain| *terrain == patch.terrain)
            .unwrap_or(0) as f32;
        commands.spawn((
            TilemapChunk {
                chunk_size: size,
                tile_display_size: UVec2::ONE,
                tileset: shapes.clone(),
                alpha_mode: AlphaMode2d::Blend,
            },
            TilemapChunkTileData::from_y_down_tiles(size, tiles),
            Transform::from_translation(patch.centre.extend(GROUND_DEPTH + layer * LAYER_GAP))
                .with_scale(Vec3::new(ground.tile, ground.tile, 1.0)),
        ));
    }
    Ok(())
}

/// How deep the routes, the markers and their labels lie: over the map, under the tokens.
const ROUTE_DEPTH: f32 = -5.0;
const MARKER_DEPTH: f32 = -4.0;
const LABEL_DEPTH: f32 = -3.0;

/// The placeholder colours of suggestions.
const SETTLEMENT: Color = Color::srgb(0.62, 0.22, 0.18);
const CAPITAL_RING: Color = Color::srgb(0.98, 0.85, 0.3);
const POINT: Color = Color::srgb(0.5, 0.25, 0.6);
const ROAD: Color = Color::srgb(0.5, 0.36, 0.22);
const TRAIL: Color = Color::srgb(0.7, 0.58, 0.42);
const SEA_ROUTE: Color = Color::srgb(0.15, 0.35, 0.85);

/// The map's suggestions, once they're worked out: each route a line of stretches with round
/// joints, each settlement a square, a capital's ringed, each point of interest a diamond; each
/// labelled. Placeholder shapes (D-46).
fn draw_suggestions(
    mut commands: Commands,
    suggested: Option<Res<Suggested>>,
    ground: Option<Res<Ground>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let (Some(suggested), Some(ground)) = (suggested, ground) else {
        return;
    };
    if !suggested.is_added() {
        return;
    }
    let mut shape = |mesh: Mesh, colour: Color, at: Vec3, turn: f32| {
        commands.spawn((
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(materials.add(colour)),
            Transform::from_translation(at).with_rotation(Quat::from_rotation_z(turn)),
        ));
    };
    for mark in &suggested.marks {
        match &mark.shape {
            Shape::Line {
                points,
                width,
                kind,
            } => {
                let colour = match kind {
                    RouteKind::Road => ROAD,
                    RouteKind::Trail => TRAIL,
                    RouteKind::SeaRoute => SEA_ROUTE,
                };
                for stretch in points.windows(2) {
                    let along = stretch[1] - stretch[0];
                    let middle = (stretch[0] + stretch[1]) / 2.0;
                    shape(
                        Rectangle::new(along.length(), *width).into(),
                        colour,
                        middle.extend(ROUTE_DEPTH),
                        along.to_angle(),
                    );
                }
                for point in points {
                    shape(
                        Circle::new(width / 2.0).into(),
                        colour,
                        point.extend(ROUTE_DEPTH),
                        0.0,
                    );
                }
            }
            Shape::Square { at, side, ringed } => {
                if *ringed {
                    shape(
                        Annulus::new(side * 0.75, side * 0.9).into(),
                        CAPITAL_RING,
                        at.extend(MARKER_DEPTH - 0.01),
                        0.0,
                    );
                }
                shape(
                    Rectangle::new(*side, *side).into(),
                    SETTLEMENT,
                    at.extend(MARKER_DEPTH),
                    0.0,
                );
            }
            Shape::Diamond { at, side } => shape(
                Rhombus::new(*side, *side).into(),
                POINT,
                at.extend(MARKER_DEPTH),
                0.0,
            ),
        }
    }
    for mark in &suggested.marks {
        commands.spawn((
            Text2d::new(mark.label.clone()),
            TextFont::from_font_size(ground.tile * 0.4),
            Transform::from_translation(mark.label_at.extend(LABEL_DEPTH)),
            bevy::sprite::Anchor::TOP_CENTER,
        ));
    }
}

/// What's said of the suggestion under the pointer, beside it.
fn hover_tip(mut contexts: EguiContexts, hovered: Res<Hovered>) -> Result {
    let Some((_, line)) = &hovered.0 else {
        return Ok(());
    };
    let ctx = contexts.ctx_mut()?;
    let Some(pointer) = ctx.pointer_hover_pos() else {
        return Ok(());
    };
    egui::Area::new(egui::Id::new("suggestion"))
        .order(egui::Order::Tooltip)
        .fixed_pos(pointer + egui::vec2(16.0, 16.0))
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width(360.0);
                ui.label(line);
            });
        });
    Ok(())
}

/// The camera where the view is, as many points a pixel as it says.
fn aim_camera(view: Res<View>, mut camera: Query<(&mut Transform, &mut Projection), With<Camera>>) {
    if !view.is_changed() {
        return;
    }
    for (mut at, mut projection) in &mut camera {
        at.translation = view.at.extend(at.translation.z);
        if let Projection::Orthographic(orthographic) = &mut *projection {
            orthographic.scale = view.scale;
        }
    }
}

/// The keys don't walk while egui has the keyboard, as when typing in the console.
fn keyboard_taken(wants: Res<EguiWantsInput>, mut taken: ResMut<KeyboardTaken>) {
    taken.0 = wants.wants_any_keyboard_input();
}

/// Where the cursor is on the plane, and whether the left button is down; nothing while egui
/// has the pointer, as over a window.
fn pointer(
    wants: Res<EguiWantsInput>,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    mut out: ResMut<Pointer>,
) {
    let on_plane = || {
        let cursor = windows.single().ok()?.cursor_position()?;
        let (camera, at) = cameras.single().ok()?;
        camera.viewport_to_world_2d(at, cursor).ok()
    };
    let at = (!wants.wants_any_pointer_input()).then(on_plane).flatten();
    out.set_if_neq(Pointer {
        at,
        down: buttons.pressed(MouseButton::Left),
    });
}

/// Each new token drawn: a disc of its own colour, its label under it, a quest marker over it
/// for when it offers one, and a ring behind it for when it's nearest. Placeholder shapes
/// (D-46).
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
        let marker = commands
            .spawn((
                Text2d::new("!"),
                TextColor(QUEST),
                Transform::from_xyz(0.0, 30.0, 1.0),
                Visibility::Hidden,
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
        let arc_mesh = meshes.add(CircularSector::new(24.0, 0.0));
        let arc = commands
            .spawn((
                Mesh2d(arc_mesh.clone()),
                MeshMaterial2d(materials.add(NEWS)),
                Transform::from_xyz(0.0, 0.0, -0.2),
                Visibility::Hidden,
            ))
            .id();
        commands
            .entity(entity)
            .insert((
                Mesh2d(meshes.add(Circle::new(TOKEN_RADIUS))),
                MeshMaterial2d(materials.add(NEUTRAL)),
                Parts {
                    label,
                    marker,
                    ring,
                    arc,
                    arc_mesh,
                },
            ))
            .add_children(&[label, marker, ring, arc]);
    }
}

/// The played token green; anyone else by how they regard it, tinted by band as the
/// relations matrix is, and labelled with it; a giver of a quest that can start marked; the
/// nearest ringed; news coming to a token an
/// arc growing as it comes, and said in its label; a token news has just reached swelling and
/// settling.
#[allow(clippy::too_many_arguments)]
fn show_tokens(
    nearby: Res<Nearby>,
    regards: Res<Regards>,
    givers: Res<Givers>,
    spreading: Res<Spreading>,
    clock: Res<Clock>,
    factional: Option<Res<Factional>>,
    mut tokens: Query<TokenParts>,
    mut labels: Query<&mut Text2d>,
    mut shown: Query<&mut Visibility>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(factional) = factional else {
        return;
    };
    let now = factional.world.now();
    for (token, material, parts, mut at, played) in &mut tokens {
        let regard = regards.0.get(&token.character);
        let coming = spreading.hearing.get(&token.character);
        let colour = match (played, regard) {
            (true, _) => PLAYED,
            (false, Some(regard)) => band_colour(regard.index, regard.bands),
            (false, None) => NEUTRAL,
        };
        if let Some(mut material) = materials.get_mut(&material.0) {
            material.color = colour;
        }
        if let Ok(mut label) = labels.get_mut(parts.label) {
            let mut said = token.character.to_string();
            if let Some(regard) = regard {
                said = format!("{said}\n{}", regard.said());
            }
            if let Some(coming) = coming {
                said = format!("{said}\n{}", coming.said(now));
            }
            if label.0 != said {
                label.0 = said;
            }
        }
        if let Ok(mut marker) = shown.get_mut(parts.marker) {
            marker.set_if_neq(visible(givers.0.contains(&token.character)));
        }
        if let Ok(mut ring) = shown.get_mut(parts.ring) {
            let nearest = nearby.nearest.as_ref() == Some(&token.character);
            ring.set_if_neq(visible(nearest));
        }
        if let Ok(mut arc) = shown.get_mut(parts.arc) {
            arc.set_if_neq(visible(coming.is_some()));
        }
        if let (Some(coming), Some(mut mesh)) = (coming, meshes.get_mut(&parts.arc_mesh)) {
            let half_turn = std::f32::consts::PI * coming.progress(now, clock.part());
            *mesh = CircularSector::new(24.0, half_turn).into();
        }
        // Swelling to 1.4 and settling, over the pulse.
        let pulse = spreading.pulsing.get(&token.character).map_or(0.0, |left| {
            let gone = 1.0 - left.as_secs_f32() / PULSE.as_secs_f32();
            (std::f32::consts::PI * gone).sin()
        });
        at.scale = Vec3::splat(1.0 + 0.4 * pulse);
    }
}

/// A token, its disc's material, its parts, where it is, and whether it's played.
type TokenParts<'a> = (
    &'a Token,
    &'a MeshMaterial2d<ColorMaterial>,
    &'a Parts,
    &'a mut Transform,
    Has<Played>,
);

/// Shown, or hidden.
fn visible(shown: bool) -> Visibility {
    match shown {
        true => Visibility::Inherited,
        false => Visibility::Hidden,
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

/// Why the editor didn't open, the last time it was asked to.
#[derive(Default)]
struct EditNote(Option<String>);

/// The controls, at the top left: time; Arrange, with Undo and Save layout while arranging;
/// and "Edit in the editor", the editor opened on the same directory at the nearest
/// character, else the one played.
#[allow(clippy::too_many_arguments)]
fn controls(
    mut contexts: EguiContexts,
    mut clock: ResMut<Clock>,
    mut note: Local<EditNote>,
    mut arranging: ResMut<Arranging>,
    dir: Res<ContentDir>,
    nearby: Res<Nearby>,
    played: Query<&Token, With<Played>>,
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
            arrange_ui(ui, &mut arranging);
            if edit_ui(ui, note.0.as_deref()) {
                let played = played.single().ok().map(|token| &token.character);
                let args = edit_args(&dir.0, &nearby, played);
                note.0 = open(Other::Editor, &Lookup::here(Other::Editor), &args).err();
            }
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

/// The Quests window, at the bottom right: what whoever's nearest offers, what the world
/// offers, and the played character's quests under way, started and chosen from it.
fn quests_window(
    mut contexts: EguiContexts,
    nearby: Res<Nearby>,
    played: Query<&Token, With<Played>>,
    factional: Option<Res<Factional>>,
    mut out: MessageWriter<FactionalCommand>,
) -> Result {
    let (Some(factional), Ok(played)) = (factional, played.single()) else {
        return Ok(());
    };
    let shown = panel(
        &factional.world,
        &factional.quests,
        &played.character,
        nearby.nearest.as_ref(),
    );
    egui::Window::new("Quests")
        .anchor(egui::Align2::RIGHT_BOTTOM, [-8.0, -8.0])
        .resizable(false)
        .show(contexts.ctx_mut()?, |ui| {
            if let Some(asked) = quests_ui(ui, &shown) {
                out.write(ask(&played.character, asked, &nearby));
            }
        });
    Ok(())
}
