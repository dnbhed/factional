//! News drawn as it goes (G6b, P-128): each token about to hear news, how far it's come, and
//! a pulse as it lands. From what the engine records of news in flight, whom it will reach
//! and when; not who passes it on, which it doesn't record (the user's choice).

use std::collections::BTreeMap;
use std::time::Duration;

use bevy::ecs::change_detection::DetectChanges;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Res, ResMut};
use bevy::time::Time;
use factional_bevy::Factional;
use factional_core::Tick;
use factional_reputation::{Change, CharacterId, Party, World};

/// How long a token pulses when news lands, in game time.
pub const PULSE: Duration = Duration::from_secs(1);

/// News on its way to a character: the hop's start and its arrival.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hearing {
    pub from: Tick,
    pub at: Tick,
}

impl Hearing {
    /// How far it's come, 0 to 1: the ticks gone of the hop's, with `part` of a tick under
    /// way.
    pub fn progress(&self, now: Tick, part: f32) -> f32 {
        let hop = self.at.0.saturating_sub(self.from.0).max(1) as f32;
        let gone = now.0.saturating_sub(self.from.0) as f32 + part;
        (gone / hop).clamp(0.0, 1.0)
    }

    /// `news in 10 ticks`.
    pub fn said(&self, now: Tick) -> String {
        match self.at.0.saturating_sub(now.0) {
            1 => "news in 1 tick".to_owned(),
            ticks => format!("news in {ticks} ticks"),
        }
    }
}

/// The news on its way to each character, the soonest of any, by id.
pub fn hearing(world: &World) -> BTreeMap<CharacterId, Hearing> {
    let hop_ticks = world.balance().ripple.hop_ticks;
    let mut found: BTreeMap<CharacterId, Hearing> = BTreeMap::new();
    for (_, news) in world.news() {
        let next = &news.next;
        let coming = Hearing {
            from: Tick(next.at.0.saturating_sub(hop_ticks)),
            at: next.at,
        };
        for character in &next.parties {
            found
                .entry(character.clone())
                .and_modify(|soonest| {
                    *soonest = std::cmp::min_by_key(*soonest, coming, |hearing| hearing.at)
                })
                .or_insert(coming);
        }
    }
    found
}

/// News on the plane: whom it's coming to, and who's just heard it, pulsing for the time left.
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct Spreading {
    pub hearing: BTreeMap<CharacterId, Hearing>,
    pub pulsing: BTreeMap<CharacterId, Duration>,
}

/// The last of the world's events looked at, so each arrival pulses once.
#[derive(Resource, Debug, Default)]
pub(crate) struct Seen(usize);

/// After the frame's commands: whom news is coming to, and who it's just landed for, pulsing
/// for `PULSE`, the pulses running down in game time.
pub(crate) fn follow_news(
    time: Res<Time>,
    factional: Option<Res<Factional>>,
    mut seen: ResMut<Seen>,
    mut spreading: ResMut<Spreading>,
) {
    let delta = time.delta();
    spreading.pulsing.retain(|_, left| {
        *left = left.saturating_sub(delta);
        !left.is_zero()
    });
    let Some(factional) = factional.filter(|factional| factional.is_changed()) else {
        return;
    };
    let world = &factional.world;
    for event in world.events().get(seen.0..).unwrap_or_default() {
        if let Change::NewsArrived { learned, .. } = &event.payload {
            for party in learned {
                if let Party::Character(character) = party {
                    spreading.pulsing.insert(character.clone(), PULSE);
                }
            }
        }
    }
    seen.0 = world.events().len();
    spreading.hearing = hearing(world);
}
