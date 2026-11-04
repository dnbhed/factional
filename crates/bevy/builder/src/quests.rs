//! Quests in the builder (G7, P-129): who offers the played character which quests, their
//! givers marked; and a panel to start them and make their choices. Whether a quest can start,
//! and why not, is the engine's to say (`assess_start`).

use std::collections::BTreeSet;

use bevy::ecs::change_detection::DetectChanges;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Query, Res, ResMut};
use bevy_egui::egui;
use factional_bevy::{Factional, FactionalCommand};
use factional_quests::{
    ChoiceId, QuestCommand, QuestId, QuestLog, QuestState, StageId, StartBlock,
};
use factional_reputation::{CharacterId, FactionId, World};

use crate::play::witnesses;
use crate::{Nearby, Played, Token};

/// A quest offered to the played character: whether they can start it now, and if not, each
/// reason, as the engine says it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub quest: QuestId,
    pub name: String,
    pub blocks: Vec<String>,
}

impl Offer {
    /// Whether nothing stops it starting.
    pub fn can_start(&self) -> bool {
        self.blocks.is_empty()
    }
}

/// A quest the played character is doing: the stage they're at, its choices, and each of its
/// requirements that doesn't hold yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnderWay {
    pub quest: QuestId,
    pub name: String,
    pub stage: StageId,
    pub choices: Vec<ChoiceId>,
    pub unmet: Vec<String>,
}

/// What the Quests panel shows: whoever's in reach and what they offer, what the world
/// offers, and what's under way.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuestPanel {
    pub giver: Option<CharacterId>,
    pub offered: Vec<Offer>,
    pub worlds: Vec<Offer>,
    pub under_way: Vec<UnderWay>,
}

/// What was asked of the panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    Start(QuestId),
    Choose(QuestId, ChoiceId),
}

/// Who offers the played character a quest they can start now, by id.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct Givers(pub BTreeSet<CharacterId>);

/// The quests `giver` offers, in id order: each whose giver, its own or else its
/// questline's, is them, or a faction they're in now.
pub fn offered_by(world: &World, log: &QuestLog, giver: &CharacterId) -> Vec<QuestId> {
    let factions: Vec<&FactionId> = world
        .memberships(giver)
        .into_iter()
        .flatten()
        .map(|(faction, _)| faction)
        .collect();
    let found = log.quests();
    found
        .quests
        .keys()
        .filter(|quest| {
            found.giver_of(quest).is_some_and(|party| {
                party.as_str() == giver.as_str()
                    || factions
                        .iter()
                        .any(|faction| party.as_str() == faction.as_str())
            })
        })
        .cloned()
        .collect()
}

/// The world's own quests, given by no one, in id order.
pub fn the_worlds(log: &QuestLog) -> Vec<QuestId> {
    let found = log.quests();
    found
        .quests
        .keys()
        .filter(|quest| found.giver_of(quest).is_none())
        .cloned()
        .collect()
}

/// Who offers `played` a quest they can start now, but `played` themselves.
pub fn givers(
    world: &World,
    log: &QuestLog,
    played: Option<&CharacterId>,
) -> BTreeSet<CharacterId> {
    let Some(played) = played else {
        return BTreeSet::new();
    };
    world
        .characters()
        .map(|character| &character.id)
        .filter(|character| *character != played)
        .filter(|character| {
            offers(world, log, played, &offered_by(world, log, character))
                .iter()
                .any(Offer::can_start)
        })
        .cloned()
        .collect()
}

/// The panel for `played`, with `giver` in reach: what they offer and the world offers, but
/// what `played` has begun; and each quest under way, in id order.
pub fn panel(
    world: &World,
    log: &QuestLog,
    played: &CharacterId,
    giver: Option<&CharacterId>,
) -> QuestPanel {
    let offered = giver.map_or_else(Vec::new, |giver| {
        offers(world, log, played, &offered_by(world, log, giver))
    });
    let quests = log.quests();
    let under_way = log
        .record(played)
        .states
        .iter()
        .filter_map(|(quest, state)| match state {
            QuestState::Active { stage } => {
                let found = &quests.quests[quest];
                let stage = &found.stages[*stage];
                Some(UnderWay {
                    quest: quest.clone(),
                    name: found.name.clone(),
                    stage: stage.id.clone(),
                    choices: stage
                        .choices
                        .iter()
                        .map(|choice| choice.id.clone())
                        .collect(),
                    unmet: log
                        .unmet(world, played, &stage.requires)
                        .iter()
                        .map(|unmet| unmet.describe(played))
                        .collect(),
                })
            }
            QuestState::Finished | QuestState::Closed { .. } => None,
        })
        .collect();
    QuestPanel {
        giver: giver.cloned(),
        offered,
        worlds: offers(world, log, played, &the_worlds(log)),
        under_way,
    }
}

/// Each of `quests` offered to `played`, as the engine assesses it; those `played` has
/// begun, finished or had closed left out.
fn offers(world: &World, log: &QuestLog, played: &CharacterId, quests: &[QuestId]) -> Vec<Offer> {
    quests
        .iter()
        .filter_map(|quest| {
            let assessment = log.assess_start(world, played, quest).ok()?;
            let begun = assessment.blocks.iter().any(|block| {
                matches!(
                    block,
                    StartBlock::AlreadyStarted
                        | StartBlock::AlreadyFinished
                        | StartBlock::Closed { .. }
                )
            });
            (!begun).then(|| Offer {
                quest: quest.clone(),
                name: log.quests().quests[quest].name.clone(),
                blocks: assessment
                    .blocks
                    .iter()
                    .map(|block| block.describe(played))
                    .collect(),
            })
        })
        .collect()
}

/// The panel drawn: what whoever's in reach offers, what the world offers, each that can
/// start a button and the rest folded under "Not yet" with why; then each quest under way at
/// its stage, a button for each choice, disabled with why while the stage's requirements
/// don't hold. What was asked of it, if anything.
pub fn quests_ui(ui: &mut egui::Ui, panel: &QuestPanel) -> Option<Asked> {
    let mut asked = None;
    match &panel.giver {
        None => {
            ui.label("No one in reach");
        }
        Some(giver) if panel.offered.is_empty() => {
            ui.label(format!("{giver} offers no quests"));
        }
        Some(giver) => {
            ui.strong(format!("Offered by {giver}"));
            asked = asked.or(offers_ui(ui, "offered", &panel.offered));
        }
    }
    if !panel.worlds.is_empty() {
        ui.separator();
        ui.strong("The world's");
        asked = asked.or(offers_ui(ui, "worlds", &panel.worlds));
    }
    if !panel.under_way.is_empty() {
        ui.separator();
        ui.strong("Under way");
        for quest in &panel.under_way {
            ui.push_id(quest.quest.as_str(), |ui| {
                ui.label(format!("{}, at {}", quest.name, quest.stage));
                for unmet in &quest.unmet {
                    ui.label(unmet);
                }
                ui.horizontal(|ui| {
                    for choice in &quest.choices {
                        let button = egui::Button::new(choice.as_str());
                        if ui.add_enabled(quest.unmet.is_empty(), button).clicked() {
                            asked = Some(Asked::Choose(quest.quest.clone(), choice.clone()));
                        }
                    }
                });
            });
        }
    }
    asked
}

/// Offers drawn: a Start button for each that can start, the rest folded under "Not yet".
fn offers_ui(ui: &mut egui::Ui, salt: &str, offers: &[Offer]) -> Option<Asked> {
    let mut asked = None;
    for offer in offers.iter().filter(|offer| offer.can_start()) {
        if ui.button(format!("Start {}", offer.name)).clicked() {
            asked = Some(Asked::Start(offer.quest.clone()));
        }
    }
    let waiting: Vec<&Offer> = offers.iter().filter(|offer| !offer.can_start()).collect();
    if !waiting.is_empty() {
        egui::CollapsingHeader::new(format!("Not yet ({})", waiting.len()))
            .id_salt(salt)
            .show(ui, |ui| {
                for offer in waiting {
                    ui.strong(&offer.name);
                    for block in &offer.blocks {
                        ui.label(block);
                    }
                }
            });
    }
    asked
}

/// What was asked, as the played character's quest command; a choice witnessed by those in
/// sight, as acting is.
pub fn ask(played: &CharacterId, asked: Asked, nearby: &Nearby) -> FactionalCommand {
    FactionalCommand::Quest(match asked {
        Asked::Start(quest) => QuestCommand::StartQuest {
            character: played.clone(),
            quest,
        },
        Asked::Choose(quest, choice) => QuestCommand::MakeChoice {
            character: played.clone(),
            quest,
            choice,
            witnesses: witnesses(nearby),
        },
    })
}

/// The givers, worked out again when the world or the quests have changed, after the
/// frame's commands.
pub(crate) fn follow_givers(
    factional: Option<Res<Factional>>,
    played: Query<&Token, With<Played>>,
    mut kept: ResMut<Givers>,
) {
    let Some(factional) = factional.filter(|factional| factional.is_changed()) else {
        return;
    };
    let played = played.single().ok().map(|token| &token.character);
    kept.0 = givers(&factional.world, &factional.quests, played);
}
