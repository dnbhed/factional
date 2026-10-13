//! Regard on the tokens (G6a, P-127): how each character regards the one played, their
//! disposition from their own picture of them (K3), in the words the editor's matrix uses.

use std::collections::BTreeMap;

use bevy::ecs::change_detection::DetectChanges;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Query, Res, ResMut};
use factional_bevy::Factional;
use factional_core::Fixed;
use factional_reputation::{CharacterId, Observer, World};

use crate::{Played, Token};

/// How one character regards the played one: their disposition's score and band, and where
/// the band is among the world's, lowest first, for its tint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Regard {
    pub score: Fixed,
    pub band: String,
    pub index: usize,
    pub bands: usize,
}

impl Regard {
    /// As the editor's matrix says it: `-5.49, neutral`.
    pub fn said(&self) -> String {
        format!("{}, {}", self.score, self.band)
    }
}

/// How each character regards the played one, by id; none with no one played.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct Regards(pub BTreeMap<CharacterId, Regard>);

/// How each of `world`'s characters regards `played`, but `played` themselves.
pub fn regards(world: &World, played: Option<&CharacterId>) -> BTreeMap<CharacterId, Regard> {
    let Some(played) = played else {
        return BTreeMap::new();
    };
    let bands: Vec<&str> = world
        .balance()
        .bands
        .iter()
        .map(|band| band.name.as_str())
        .collect();
    world
        .characters()
        .filter(|character| &character.id != played)
        .filter_map(|character| {
            let observer = Observer::Character(character.id.clone());
            let disposition = world.disposition(&observer, played)?;
            let index = bands.iter().position(|band| *band == disposition.band)?;
            Some((
                character.id.clone(),
                Regard {
                    score: disposition.score,
                    band: disposition.band,
                    index,
                    bands: bands.len(),
                },
            ))
        })
        .collect()
}

/// The regards, worked out again when the world has changed, after the frame's commands.
pub(crate) fn follow_regards(
    factional: Option<Res<Factional>>,
    played: Query<&Token, With<Played>>,
    mut kept: ResMut<Regards>,
) {
    let Some(factional) = factional.filter(|factional| factional.is_changed()) else {
        return;
    };
    let played = played.single().ok().map(|token| &token.character);
    kept.0 = regards(&factional.world, played);
}
