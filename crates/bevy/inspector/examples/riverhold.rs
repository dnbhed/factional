//! Riverhold in a window with the inspector open (G1): Space makes the player steal from
//! Merchant Ava, and a tick passes every second. The backquote shows and hides the inspector.
//!
//! `cargo run -p factional-bevy-inspector --example riverhold --features example`, in
//! `crates/bevy`.

use std::path::Path;
use std::time::Duration;

use bevy::prelude::*;
use factional_bevy::{FactionalCommand, FactionalPlugin};
use factional_bevy_inspector::{Inspector, InspectorPlugin};
use factional_core::Fixed;
use factional_reputation::{ActionId, CharacterId, Command, Witnesses};

fn main() -> AppExit {
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../content/sample");
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(FactionalPlugin {
            content,
            tick_every: Some(Duration::from_secs(1)),
        })
        .add_plugins(InspectorPlugin::default())
        .insert_resource(Inspector { open: true })
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(Camera2d);
        })
        .add_systems(Update, steal_on_space)
        .run()
}

/// The player steals from Merchant Ava, in front of everyone.
fn steal_on_space(keys: Res<ButtonInput<KeyCode>>, mut out: MessageWriter<FactionalCommand>) {
    if keys.just_pressed(KeyCode::Space) {
        let id = |text| CharacterId::new(text).expect("a valid id");
        out.write(FactionalCommand::World(Command::PerformAction {
            actor: id("player"),
            action: ActionId::new("steal").expect("a valid id"),
            target: Some(id("merchant_ava")),
            scale: Fixed::ONE,
            witnesses: Witnesses::Everyone,
        }));
    }
}
