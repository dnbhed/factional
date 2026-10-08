//! The load-time checks on quests and questlines: every reference resolves and every value
//! is in range (D-20, P-32). Whether every stage can be reached (Q3) and whether quests
//! reconcile (Q4) come later.

use std::collections::BTreeMap;
use std::fmt;

use factional_core::{Fixed, suggest};
use factional_reputation::{
    AXIS_LIMIT, CharacterId, Content, FactionId, OutcomeId, Party, RankId, ShiftProblem,
};

use crate::quest::{
    ChoiceEffects, ChoiceId, Leftovers, Next, PartyRef, Progress, Quest, QuestId, QuestlineId,
    Quests, Requirements, StageId,
};

/// What has a giver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Owner {
    Quest(QuestId),
    Questline(QuestlineId),
}

/// A choice's place: its quest, and its stage's and its own places in their lists, from 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceAt {
    pub quest: QuestId,
    pub stage: usize,
    pub choice: usize,
}

/// Where a set of requirements is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gate {
    /// A quest's own `requires`, to start it.
    Quest(QuestId),
    /// A stage's, to reach it; `stage` is its place, from 0.
    Stage { quest: QuestId, stage: usize },
    /// A questline step's, added to each of its quests' gates.
    Step { questline: QuestlineId, step: usize },
}

/// Which entry of a `requires` table a problem is at: an id for a table, a place for a list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequirementKey {
    Standing(PartyRef),
    Member(usize),
    NotMember(usize),
    RankAtLeast(FactionId),
    WithinTolerance(usize),
    Done(usize),
}

/// Something wrong with quests or questlines, found before a world is built (P-32).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestProblem {
    /// A giver that's neither a faction nor a character.
    UnknownGiver {
        owner: Owner,
        giver: PartyRef,
        suggestion: Option<String>,
    },
    NoStages(QuestId),
    /// Two stages of a quest share an id; `stage` is the second.
    DuplicateStage {
        quest: QuestId,
        stage: usize,
        id: StageId,
    },
    /// A stage called `end`, which `next` uses for the end of the quest.
    StageCalledEnd {
        quest: QuestId,
        stage: usize,
    },
    NoChoices {
        quest: QuestId,
        stage: usize,
    },
    /// Two choices of a stage share an id; `at` is the second.
    DuplicateChoice {
        at: ChoiceAt,
        id: ChoiceId,
    },
    UnknownOutcome {
        at: ChoiceAt,
        outcome: OutcomeId,
        suggestion: Option<OutcomeId>,
    },
    /// A choice's inline `standing` names a faction or character that doesn't exist.
    UnknownEffectParty {
        at: ChoiceAt,
        party: Party,
        suggestion: Option<Party>,
    },
    /// A choice's inline standing change outside −100…100.
    EffectOutOfRange {
        at: ChoiceAt,
        party: Party,
        value: Fixed,
    },
    /// A choice's inline relation shift that can't be made.
    EffectRelation {
        at: ChoiceAt,
        problem: ShiftProblem,
    },
    /// `next` names a stage the quest doesn't have.
    UnknownNext {
        at: ChoiceAt,
        next: StageId,
        suggestion: Option<StageId>,
    },
    /// `next` names the choice's own stage or an earlier one, which would make a loop.
    BackwardNext {
        at: ChoiceAt,
        next: StageId,
    },
    /// A `standing` requirement names a faction or character that doesn't exist.
    UnknownParty {
        gate: Gate,
        party: PartyRef,
        suggestion: Option<String>,
    },
    /// A `standing` requirement outside −100…100.
    RequirementOutOfRange {
        gate: Gate,
        party: PartyRef,
        value: Fixed,
    },
    /// A requirement names a faction that doesn't exist.
    UnknownFaction {
        gate: Gate,
        key: RequirementKey,
        faction: FactionId,
        suggestion: Option<FactionId>,
    },
    /// `rank_at_least` names a rank that isn't on the faction's ladder.
    UnknownRank {
        gate: Gate,
        faction: FactionId,
        rank: RankId,
        suggestion: Option<RankId>,
    },
    /// A requirement's list names the same thing twice; `key` is the second.
    Repeated {
        gate: Gate,
        key: RequirementKey,
        entry: String,
    },
    /// `done` names progress that doesn't exist. `missing` is as far as the first part that
    /// doesn't: the quest, its stage or the stage's choice; `suggestion` is for that part.
    UnknownProgress {
        gate: Gate,
        index: usize,
        missing: Progress,
        suggestion: Option<String>,
    },
    NoSteps(QuestlineId),
    EmptyStep {
        questline: QuestlineId,
        step: usize,
    },
    /// A step names a quest that doesn't exist; `index` is its place in the step's list.
    UnknownQuest {
        questline: QuestlineId,
        step: usize,
        index: usize,
        quest: QuestId,
        suggestion: Option<QuestId>,
    },
    /// A quest listed a second time, in any questline; `first` is where it was first.
    QuestRepeated {
        questline: QuestlineId,
        step: usize,
        index: usize,
        quest: QuestId,
        first: (QuestlineId, usize),
    },
    /// A step needs more quests than it has.
    NeedTooMany {
        questline: QuestlineId,
        step: usize,
        need: usize,
        quests: usize,
    },
}

/// Something in quests that's allowed but probably not meant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestWarning {
    /// `leftovers = "close"` on a step that needs all its quests, so none are left over.
    LeftoversNeverLeft { questline: QuestlineId, step: usize },
}

impl Quests {
    /// Every problem with these quests against `content`, in a fixed order: each quest in id
    /// order (its giver, its gate, then each stage), then each questline in id order. Empty
    /// if every reference resolves and every value is in range.
    pub fn problems(&self, content: &Content) -> Vec<QuestProblem> {
        let mut problems = Vec::new();
        for quest in self.quests.values() {
            self.check_quest(quest, content, &mut problems);
        }
        // Where each quest was first listed, so a second listing can say where the first is.
        let mut placed: BTreeMap<&QuestId, (QuestlineId, usize)> = BTreeMap::new();
        for line in self.questlines.values() {
            if let Some(giver) = &line.giver {
                check_giver(
                    Owner::Questline(line.id.clone()),
                    giver,
                    content,
                    &mut problems,
                );
            }
            if line.steps.is_empty() {
                problems.push(QuestProblem::NoSteps(line.id.clone()));
            }
            for (index, step) in line.steps.iter().enumerate() {
                if step.quests.is_empty() {
                    problems.push(QuestProblem::EmptyStep {
                        questline: line.id.clone(),
                        step: index,
                    });
                }
                for (place, quest) in step.quests.iter().enumerate() {
                    if let Some(first) = placed.get(quest) {
                        problems.push(QuestProblem::QuestRepeated {
                            questline: line.id.clone(),
                            step: index,
                            index: place,
                            quest: quest.clone(),
                            first: first.clone(),
                        });
                    } else if self.quests.contains_key(quest) {
                        placed.insert(quest, (line.id.clone(), index));
                    } else {
                        problems.push(QuestProblem::UnknownQuest {
                            questline: line.id.clone(),
                            step: index,
                            index: place,
                            quest: quest.clone(),
                            suggestion: closest(quest.as_str(), self.quests.keys()),
                        });
                    }
                }
                if step.needed() > step.quests.len() {
                    problems.push(QuestProblem::NeedTooMany {
                        questline: line.id.clone(),
                        step: index,
                        need: step.needed(),
                        quests: step.quests.len(),
                    });
                }
                let gate = Gate::Step {
                    questline: line.id.clone(),
                    step: index,
                };
                self.check_requirements(&gate, &step.requires, content, &mut problems);
            }
        }
        problems
    }

    /// Everything probably not meant, in a fixed order: each questline in id order, each
    /// step in order.
    pub fn warnings(&self) -> Vec<QuestWarning> {
        self.questlines
            .values()
            .flat_map(|line| {
                line.steps
                    .iter()
                    .enumerate()
                    .filter(|(_, step)| {
                        step.leftovers == Leftovers::Close && step.needed() >= step.quests.len()
                    })
                    .map(|(step, _)| QuestWarning::LeftoversNeverLeft {
                        questline: line.id.clone(),
                        step,
                    })
            })
            .collect()
    }

    /// A quest's giver, its gate, then each stage: its id, its requirements, then its
    /// choices.
    fn check_quest(&self, quest: &Quest, content: &Content, problems: &mut Vec<QuestProblem>) {
        if let Some(giver) = &quest.giver {
            check_giver(Owner::Quest(quest.id.clone()), giver, content, problems);
        }
        let gate = Gate::Quest(quest.id.clone());
        self.check_requirements(&gate, &quest.requires, content, problems);
        if quest.stages.is_empty() {
            problems.push(QuestProblem::NoStages(quest.id.clone()));
        }
        for (index, stage) in quest.stages.iter().enumerate() {
            if quest.stages[..index]
                .iter()
                .any(|other| other.id == stage.id)
            {
                problems.push(QuestProblem::DuplicateStage {
                    quest: quest.id.clone(),
                    stage: index,
                    id: stage.id.clone(),
                });
            } else if stage.id.as_str() == Next::END {
                problems.push(QuestProblem::StageCalledEnd {
                    quest: quest.id.clone(),
                    stage: index,
                });
            }
            let gate = Gate::Stage {
                quest: quest.id.clone(),
                stage: index,
            };
            self.check_requirements(&gate, &stage.requires, content, problems);
            if stage.choices.is_empty() {
                problems.push(QuestProblem::NoChoices {
                    quest: quest.id.clone(),
                    stage: index,
                });
            }
            for (place, choice) in stage.choices.iter().enumerate() {
                let at = ChoiceAt {
                    quest: quest.id.clone(),
                    stage: index,
                    choice: place,
                };
                if stage.choices[..place]
                    .iter()
                    .any(|other| other.id == choice.id)
                {
                    problems.push(QuestProblem::DuplicateChoice {
                        at: at.clone(),
                        id: choice.id.clone(),
                    });
                }
                check_effects(&at, &choice.effects, content, problems);
                if let Next::Stage(next) = &choice.next {
                    match quest.stages.iter().position(|stage| &stage.id == next) {
                        None => problems.push(QuestProblem::UnknownNext {
                            at,
                            next: next.clone(),
                            suggestion: closest(
                                next.as_str(),
                                quest.stages.iter().map(|stage| &stage.id),
                            ),
                        }),
                        Some(later) if later <= index => {
                            problems.push(QuestProblem::BackwardNext {
                                at,
                                next: next.clone(),
                            });
                        }
                        Some(_) => {}
                    }
                }
            }
        }
    }

    /// Each requirement in the order `Requirements::KEYS` lists them, and each entry in
    /// order.
    fn check_requirements(
        &self,
        gate: &Gate,
        requires: &Requirements,
        content: &Content,
        problems: &mut Vec<QuestProblem>,
    ) {
        for (party, value) in &requires.standing {
            if resolve(party, content).is_none() {
                problems.push(QuestProblem::UnknownParty {
                    gate: gate.clone(),
                    party: party.clone(),
                    suggestion: closest_party(party.as_str(), content),
                });
            } else if !(-AXIS_LIMIT..=AXIS_LIMIT).contains(value) {
                problems.push(QuestProblem::RequirementOutOfRange {
                    gate: gate.clone(),
                    party: party.clone(),
                    value: *value,
                });
            }
        }
        check_factions(
            gate,
            &requires.member,
            RequirementKey::Member,
            content,
            problems,
        );
        check_factions(
            gate,
            &requires.not_member,
            RequirementKey::NotMember,
            content,
            problems,
        );
        for (faction, rank) in &requires.rank_at_least {
            match content.factions.get(faction) {
                None => problems.push(QuestProblem::UnknownFaction {
                    gate: gate.clone(),
                    key: RequirementKey::RankAtLeast(faction.clone()),
                    faction: faction.clone(),
                    suggestion: closest(faction.as_str(), content.factions.keys()),
                }),
                Some(found) if found.rank_position(rank).is_none() => {
                    problems.push(QuestProblem::UnknownRank {
                        gate: gate.clone(),
                        faction: faction.clone(),
                        rank: rank.clone(),
                        suggestion: closest(rank.as_str(), found.ranks.iter().map(|r| &r.id)),
                    });
                }
                Some(_) => {}
            }
        }
        check_factions(
            gate,
            &requires.within_tolerance,
            RequirementKey::WithinTolerance,
            content,
            problems,
        );
        for (index, progress) in requires.done.iter().enumerate() {
            if requires.done[..index].contains(progress) {
                problems.push(QuestProblem::Repeated {
                    gate: gate.clone(),
                    key: RequirementKey::Done(index),
                    entry: progress.to_string(),
                });
            } else if let Some((missing, suggestion)) = self.missing(progress) {
                problems.push(QuestProblem::UnknownProgress {
                    gate: gate.clone(),
                    index,
                    missing,
                    suggestion,
                });
            }
        }
    }

    /// As far as the first part of `progress` that doesn't exist, with a suggestion for that
    /// part; `None` if it all exists.
    fn missing(&self, progress: &Progress) -> Option<(Progress, Option<String>)> {
        let quest_id = progress.quest();
        let Some(quest) = self.quests.get(quest_id) else {
            let suggestion = closest(quest_id.as_str(), self.quests.keys());
            return Some((
                Progress::Quest(quest_id.clone()),
                suggestion.map(|id| id.to_string()),
            ));
        };
        let (stage_id, choice_id) = match progress {
            Progress::Quest(_) => return None,
            Progress::Stage(_, stage) => (stage, None),
            Progress::Choice(_, stage, choice) => (stage, Some(choice)),
        };
        let Some(stage) = quest.stages.iter().find(|stage| &stage.id == stage_id) else {
            let suggestion = closest(stage_id.as_str(), quest.stages.iter().map(|s| &s.id));
            return Some((
                Progress::Stage(quest_id.clone(), stage_id.clone()),
                suggestion.map(|id| id.to_string()),
            ));
        };
        let choice_id = choice_id?;
        if stage.choices.iter().any(|choice| &choice.id == choice_id) {
            return None;
        }
        let suggestion = closest(choice_id.as_str(), stage.choices.iter().map(|c| &c.id));
        Some((progress.clone(), suggestion.map(|id| id.to_string())))
    }
}

/// A giver must be a faction or a character.
fn check_giver(
    owner: Owner,
    giver: &PartyRef,
    content: &Content,
    problems: &mut Vec<QuestProblem>,
) {
    if resolve(giver, content).is_none() {
        problems.push(QuestProblem::UnknownGiver {
            owner,
            giver: giver.clone(),
            suggestion: closest_party(giver.as_str(), content),
        });
    }
}

/// An outcome must exist; inline standing must name parties that exist, within ±100, and
/// inline relation shifts must be ones the reputation module could make.
fn check_effects(
    at: &ChoiceAt,
    effects: &ChoiceEffects,
    content: &Content,
    problems: &mut Vec<QuestProblem>,
) {
    match effects {
        ChoiceEffects::None => {}
        ChoiceEffects::Outcome(outcome) => {
            if !content.outcomes.contains_key(outcome) {
                problems.push(QuestProblem::UnknownOutcome {
                    at: at.clone(),
                    outcome: outcome.clone(),
                    suggestion: closest(outcome.as_str(), content.outcomes.keys()),
                });
            }
        }
        ChoiceEffects::Inline(effects) => {
            for (party, value) in effects.standing.parties() {
                let unknown = match &party {
                    Party::Faction(id) => (!content.factions.contains_key(id))
                        .then(|| closest(id.as_str(), content.factions.keys()).map(Party::Faction)),
                    Party::Character(id) => (!content.characters.contains_key(id)).then(|| {
                        closest(id.as_str(), content.characters.keys()).map(Party::Character)
                    }),
                };
                if let Some(suggestion) = unknown {
                    problems.push(QuestProblem::UnknownEffectParty {
                        at: at.clone(),
                        party,
                        suggestion,
                    });
                } else if !(-AXIS_LIMIT..=AXIS_LIMIT).contains(&value) {
                    problems.push(QuestProblem::EffectOutOfRange {
                        at: at.clone(),
                        party,
                        value,
                    });
                }
            }
            problems.extend(content.shift_problems(&effects.relations).into_iter().map(
                |problem| QuestProblem::EffectRelation {
                    at: at.clone(),
                    problem,
                },
            ));
        }
    }
}

/// Each faction in a list must exist, and be listed once.
fn check_factions(
    gate: &Gate,
    factions: &[FactionId],
    key: fn(usize) -> RequirementKey,
    content: &Content,
    problems: &mut Vec<QuestProblem>,
) {
    for (index, faction) in factions.iter().enumerate() {
        if factions[..index].contains(faction) {
            problems.push(QuestProblem::Repeated {
                gate: gate.clone(),
                key: key(index),
                entry: faction.to_string(),
            });
        } else if !content.factions.contains_key(faction) {
            problems.push(QuestProblem::UnknownFaction {
                gate: gate.clone(),
                key: key(index),
                faction: faction.clone(),
                suggestion: closest(faction.as_str(), content.factions.keys()),
            });
        }
    }
}

/// The faction or character an id names, if either exists.
fn resolve(party: &PartyRef, content: &Content) -> Option<Party> {
    let faction = FactionId::new(party.as_str()).ok()?;
    if content.factions.contains_key(&faction) {
        return Some(Party::Faction(faction));
    }
    let character = CharacterId::new(party.as_str()).ok()?;
    content
        .characters
        .contains_key(&character)
        .then_some(Party::Character(character))
}

/// The faction or character id closest to `word`, factions first.
fn closest_party(word: &str, content: &Content) -> Option<String> {
    let ids = content
        .factions
        .keys()
        .map(FactionId::as_str)
        .chain(content.characters.keys().map(CharacterId::as_str));
    suggest(word, ids).map(str::to_owned)
}

/// The id closest to `word`, if one is close enough to be a likely typo.
fn closest<'a, Id>(word: &str, mut ids: impl Iterator<Item = &'a Id> + Clone) -> Option<Id>
where
    Id: Clone + AsRef<str> + 'a,
{
    let close = suggest(word, ids.clone().map(AsRef::as_ref))?;
    ids.find(|id| id.as_ref() == close).cloned()
}

/// `" (did you mean 'x'?)"`, or nothing.
fn did_you_mean(f: &mut fmt::Formatter<'_>, suggestion: Option<&dyn fmt::Display>) -> fmt::Result {
    match suggestion {
        Some(close) => write!(f, " (did you mean '{close}'?)"),
        None => Ok(()),
    }
}

fn out_of_range(f: &mut fmt::Formatter<'_>, value: Fixed) -> fmt::Result {
    write!(f, "{value} is outside {}..{}", -AXIS_LIMIT, AXIS_LIMIT)
}

impl fmt::Display for QuestProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QuestProblem::UnknownGiver {
                giver, suggestion, ..
            }
            | QuestProblem::UnknownParty {
                party: giver,
                suggestion,
                ..
            } => {
                write!(f, "unknown faction or character '{giver}'")?;
                did_you_mean(f, suggestion.as_ref().map(|s| s as &dyn fmt::Display))
            }
            QuestProblem::NoStages(_) => f.write_str("a quest needs at least one stage"),
            QuestProblem::DuplicateStage { id, .. } => {
                write!(f, "another stage is already called '{id}'")
            }
            QuestProblem::StageCalledEnd { .. } => write!(
                f,
                "a stage can't be called '{}': next = \"{}\" ends the quest",
                Next::END,
                Next::END
            ),
            QuestProblem::NoChoices { .. } => f.write_str("a stage needs at least one choice"),
            QuestProblem::DuplicateChoice { id, .. } => {
                write!(f, "another choice in this stage is already called '{id}'")
            }
            QuestProblem::UnknownOutcome {
                outcome,
                suggestion,
                ..
            } => {
                write!(f, "unknown outcome '{outcome}'")?;
                did_you_mean(f, suggestion.as_ref().map(|s| s as &dyn fmt::Display))
            }
            QuestProblem::UnknownEffectParty {
                party, suggestion, ..
            } => {
                let kind = match party {
                    Party::Faction(_) => "faction",
                    Party::Character(_) => "character",
                };
                write!(f, "unknown {kind} '{party}'")?;
                did_you_mean(f, suggestion.as_ref().map(|s| s as &dyn fmt::Display))
            }
            QuestProblem::EffectOutOfRange { value, .. }
            | QuestProblem::RequirementOutOfRange { value, .. } => out_of_range(f, *value),
            QuestProblem::EffectRelation { problem, .. } => problem.fmt(f),
            QuestProblem::UnknownNext {
                next, suggestion, ..
            } => {
                write!(f, "unknown stage '{next}' in this quest")?;
                did_you_mean(f, suggestion.as_ref().map(|s| s as &dyn fmt::Display))
            }
            QuestProblem::BackwardNext { next, .. } => write!(
                f,
                "choices only lead forward, and '{next}' isn't after this stage"
            ),
            QuestProblem::UnknownFaction {
                faction,
                suggestion,
                ..
            } => {
                write!(f, "unknown faction '{faction}'")?;
                did_you_mean(f, suggestion.as_ref().map(|s| s as &dyn fmt::Display))
            }
            QuestProblem::UnknownRank {
                faction,
                rank,
                suggestion,
                ..
            } => {
                write!(f, "unknown rank '{rank}' for {faction}")?;
                did_you_mean(f, suggestion.as_ref().map(|s| s as &dyn fmt::Display))
            }
            QuestProblem::Repeated { entry, .. } => write!(f, "'{entry}' is listed twice"),
            QuestProblem::UnknownProgress {
                missing,
                suggestion,
                ..
            } => {
                match missing {
                    Progress::Quest(quest) => write!(f, "unknown quest '{quest}'")?,
                    Progress::Stage(quest, stage) => {
                        write!(f, "unknown stage '{stage}' in {quest}")?;
                    }
                    Progress::Choice(quest, stage, choice) => {
                        write!(f, "unknown choice '{choice}' in {quest}.{stage}")?;
                    }
                }
                did_you_mean(f, suggestion.as_ref().map(|s| s as &dyn fmt::Display))
            }
            QuestProblem::NoSteps(_) => f.write_str("a questline needs at least one step"),
            QuestProblem::EmptyStep { .. } => f.write_str("a step needs at least one quest"),
            QuestProblem::UnknownQuest {
                quest, suggestion, ..
            } => {
                write!(f, "unknown quest '{quest}'")?;
                did_you_mean(f, suggestion.as_ref().map(|s| s as &dyn fmt::Display))
            }
            QuestProblem::QuestRepeated {
                quest,
                first: (questline, step),
                ..
            } => write!(
                f,
                "'{quest}' is already at {questline}.steps[{step}]: a quest is in at most one questline, at one step"
            ),
            QuestProblem::NeedTooMany { need, quests, .. } => {
                let quests = match quests {
                    1 => "1 quest".to_owned(),
                    n => format!("{n} quests"),
                };
                write!(f, "need is {need}, but the step has only {quests}")
            }
        }
    }
}

impl fmt::Display for QuestWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QuestWarning::LeftoversNeverLeft { .. } => f.write_str(
                "leftovers = \"close\" has no effect: the step needs all its quests, so none are left over",
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use factional_reputation::{
        Alignment, Balance, Character, CharacterId, Effects, Faction, Outcome, Rank, RelationEnds,
        RelationShift, RelationSide, StandingEffects, Tolerances,
    };

    use super::*;
    use crate::{
        Choice, ChoiceEffects, Leftovers, Next, Quest, Questline, Requirements, Stage, Step,
    };

    const fn h(hundredths: i64) -> Fixed {
        Fixed::from_hundredths(hundredths)
    }

    fn faction_id(id: &str) -> FactionId {
        FactionId::new(id).expect("valid id")
    }

    fn quest_id(id: &str) -> QuestId {
        QuestId::new(id).expect("valid id")
    }

    fn stage_id(id: &str) -> StageId {
        StageId::new(id).expect("valid id")
    }

    fn line_id(id: &str) -> QuestlineId {
        QuestlineId::new(id).expect("valid id")
    }

    fn party(id: &str) -> PartyRef {
        PartyRef::new(id).expect("valid id")
    }

    fn rank(id: &str) -> RankId {
        RankId::new(id).expect("valid id")
    }

    fn outcome_id(id: &str) -> OutcomeId {
        OutcomeId::new(id).expect("valid id")
    }

    /// Two factions, the Watch (recruit, sergeant) and the Guild (cutpurse); one character,
    /// Hale; one outcome, `fined`.
    fn content() -> Content {
        let faction = |id: &str, ranks: &[&str]| Faction {
            id: faction_id(id),
            name: id.to_owned(),
            alignment: Alignment::new(h(0), h(0)).expect("in range"),
            weights: None,
            tolerances: Tolerances::new(h(40_00), None).expect("valid"),
            leave_standing_change: h(0),
            ranks: ranks
                .iter()
                .map(|id| Rank {
                    id: rank(id),
                    requires_standing: None,
                    tolerance: None,
                })
                .collect(),
            rule_tables: BTreeMap::new(),
            drift: None,
            expel_standing_change: Faction::DEFAULT_EXPEL_STANDING_CHANGE,
            secret_members: false,
        };
        let hale = Character {
            id: CharacterId::new("hale").expect("valid id"),
            name: "Hale".to_owned(),
            alignment: Alignment::new(h(0), h(0)).expect("in range"),
            weights: None,
            inertia: None,
            memberships: Vec::new(),
            standing: StandingEffects::default(),
            contacts: Vec::new(),
        };
        Content {
            balance: Balance::default(),
            characters: [(hale.id.clone(), hale)].into(),
            factions: [
                (
                    faction_id("watch"),
                    faction("watch", &["recruit", "sergeant"]),
                ),
                (faction_id("guild"), faction("guild", &["cutpurse"])),
            ]
            .into(),
            actions: BTreeMap::new(),
            relations: Vec::new(),
            outcomes: [(
                outcome_id("fined"),
                Outcome {
                    id: outcome_id("fined"),
                    effects: Effects::default(),
                },
            )]
            .into(),
        }
    }

    fn choice(id: &str, effects: ChoiceEffects, next: Option<&str>) -> Choice {
        Choice {
            id: ChoiceId::new(id).expect("valid id"),
            effects,
            next: next.map_or(Next::End, |stage| Next::Stage(stage_id(stage))),
        }
    }

    fn stage(id: &str, choices: Vec<Choice>) -> Stage {
        Stage {
            id: stage_id(id),
            requires: Requirements::default(),
            choices,
        }
    }

    fn step(quests: &[&str]) -> Step {
        Step {
            quests: quests.iter().map(|id| quest_id(id)).collect(),
            need: None,
            requires: Requirements::default(),
            leftovers: Leftovers::Open,
        }
    }

    /// `oath`, the Watch's: `patrol` (`report`, fined, on to `swear`; `look_away`, the end),
    /// then `swear` (`swear`, the end). `errand`, Hale's: `run` (`go`, the end). `career`,
    /// the Watch's: `oath`, then `errand`.
    fn quests() -> Quests {
        let oath = Quest {
            id: quest_id("oath"),
            name: "The Oath".to_owned(),
            giver: Some(party("watch")),
            requires: Requirements::default(),
            stages: vec![
                stage(
                    "patrol",
                    vec![
                        choice(
                            "report",
                            ChoiceEffects::Outcome(outcome_id("fined")),
                            Some("swear"),
                        ),
                        choice("look_away", ChoiceEffects::None, None),
                    ],
                ),
                stage("swear", vec![choice("swear", ChoiceEffects::None, None)]),
            ],
        };
        let errand = Quest {
            id: quest_id("errand"),
            name: "An Errand".to_owned(),
            giver: Some(party("hale")),
            requires: Requirements::default(),
            stages: vec![stage("run", vec![choice("go", ChoiceEffects::None, None)])],
        };
        let career = Questline {
            id: line_id("career"),
            name: "A Career".to_owned(),
            giver: Some(party("watch")),
            steps: vec![step(&["oath"]), step(&["errand"])],
        };
        Quests {
            quests: [(oath.id.clone(), oath), (errand.id.clone(), errand)].into(),
            questlines: [(career.id.clone(), career)].into(),
        }
    }

    /// The problems once `change` is made to the quests.
    fn problems_after(change: impl FnOnce(&mut Quests)) -> Vec<QuestProblem> {
        let mut quests = quests();
        change(&mut quests);
        quests.problems(&content())
    }

    fn oath(quests: &mut Quests) -> &mut Quest {
        quests.quests.get_mut(&quest_id("oath")).expect("there")
    }

    fn career(quests: &mut Quests) -> &mut Questline {
        quests
            .questlines
            .get_mut(&line_id("career"))
            .expect("there")
    }

    fn at(quest: &str, stage: usize, choice: usize) -> ChoiceAt {
        ChoiceAt {
            quest: quest_id(quest),
            stage,
            choice,
        }
    }

    #[test]
    fn quests_that_name_only_what_exists_have_no_problems() {
        assert_eq!(quests().problems(&content()), []);
        assert_eq!(Quests::default().problems(&content()), []);
    }

    #[test]
    fn a_giver_is_a_faction_a_character_or_no_one() {
        assert_eq!(
            problems_after(|quests| {
                oath(quests).giver = None;
                career(quests).giver = None;
            }),
            []
        );
        assert_eq!(
            problems_after(|quests| {
                oath(quests).giver = Some(party("wach"));
                career(quests).giver = Some(party("hal"));
            }),
            [
                QuestProblem::UnknownGiver {
                    owner: Owner::Quest(quest_id("oath")),
                    giver: party("wach"),
                    suggestion: Some("watch".to_owned()),
                },
                QuestProblem::UnknownGiver {
                    owner: Owner::Questline(line_id("career")),
                    giver: party("hal"),
                    suggestion: Some("hale".to_owned()),
                },
            ]
        );
    }

    #[test]
    fn stages_are_there_with_unique_ids_none_called_end() {
        assert_eq!(
            problems_after(|quests| oath(quests).stages.clear()),
            [QuestProblem::NoStages(quest_id("oath"))]
        );
        assert_eq!(
            problems_after(|quests| {
                let stages = &mut oath(quests).stages;
                stages.push(stage(
                    "patrol",
                    vec![choice("x", ChoiceEffects::None, None)],
                ));
                stages.push(stage("end", vec![choice("x", ChoiceEffects::None, None)]));
            }),
            [
                QuestProblem::DuplicateStage {
                    quest: quest_id("oath"),
                    stage: 2,
                    id: stage_id("patrol"),
                },
                QuestProblem::StageCalledEnd {
                    quest: quest_id("oath"),
                    stage: 3,
                },
            ]
        );
    }

    #[test]
    fn choices_are_there_with_unique_ids_in_their_stage() {
        assert_eq!(
            problems_after(|quests| oath(quests).stages[1].choices.clear()),
            [QuestProblem::NoChoices {
                quest: quest_id("oath"),
                stage: 1,
            }]
        );
        assert_eq!(
            problems_after(|quests| {
                let swear = &mut oath(quests).stages[1].choices;
                swear.push(choice("look_away", ChoiceEffects::None, None));
                swear.push(choice("swear", ChoiceEffects::None, None));
            }),
            [QuestProblem::DuplicateChoice {
                at: at("oath", 1, 2),
                id: ChoiceId::new("swear").expect("valid id"),
            }]
        );
    }

    #[test]
    fn a_choice_names_an_outcome_that_exists() {
        assert_eq!(
            problems_after(|quests| {
                oath(quests).stages[0].choices[0].effects =
                    ChoiceEffects::Outcome(outcome_id("fine"));
            }),
            [QuestProblem::UnknownOutcome {
                at: at("oath", 0, 0),
                outcome: outcome_id("fine"),
                suggestion: Some(outcome_id("fined")),
            }]
        );
    }

    #[test]
    fn inline_effects_name_parties_that_exist_within_range() {
        let mut effects = Effects::default();
        effects.standing.factions = [
            (faction_id("watch"), h(100_00)),
            (faction_id("guild"), h(-100_01)),
            (faction_id("guil"), h(5_00)),
        ]
        .into();
        effects.standing.characters = [
            (CharacterId::new("hale").expect("valid"), h(-100_00)),
            (CharacterId::new("hal").expect("valid"), h(1_00)),
        ]
        .into();
        assert_eq!(
            problems_after(|quests| {
                oath(quests).stages[0].choices[1].effects = ChoiceEffects::Inline(effects);
            }),
            [
                QuestProblem::UnknownEffectParty {
                    at: at("oath", 0, 1),
                    party: Party::Faction(faction_id("guil")),
                    suggestion: Some(Party::Faction(faction_id("guild"))),
                },
                QuestProblem::EffectOutOfRange {
                    at: at("oath", 0, 1),
                    party: Party::Faction(faction_id("guild")),
                    value: h(-100_01),
                },
                QuestProblem::UnknownEffectParty {
                    at: at("oath", 0, 1),
                    party: Party::Character(CharacterId::new("hal").expect("valid")),
                    suggestion: Some(Party::Character(CharacterId::new("hale").expect("valid"))),
                },
            ]
        );
    }

    #[test]
    fn inline_relation_shifts_name_two_factions_once_each_within_range() {
        let shift = |from: &str, to: &str, by: i64| RelationShift {
            ends: RelationEnds::Directed {
                from: faction_id(from),
                to: faction_id(to),
            },
            by: h(by),
        };
        let effects = Effects {
            relations: vec![
                shift("watch", "guild", 200_00),
                shift("guild", "wach", 5_00),
                shift("watch", "guild", -5_00),
            ],
            ..Effects::default()
        };
        assert_eq!(
            problems_after(|quests| {
                oath(quests).stages[1].choices[0].effects = ChoiceEffects::Inline(effects);
            }),
            [
                QuestProblem::EffectRelation {
                    at: at("oath", 1, 0),
                    problem: ShiftProblem::UnknownFaction {
                        index: 1,
                        side: RelationSide::To,
                        faction: faction_id("wach"),
                        suggestion: Some(faction_id("watch")),
                    },
                },
                QuestProblem::EffectRelation {
                    at: at("oath", 1, 0),
                    problem: ShiftProblem::Repeated {
                        index: 2,
                        from: faction_id("watch"),
                        to: faction_id("guild"),
                        first: 0,
                    },
                },
            ]
        );
        assert_eq!(
            QuestProblem::EffectRelation {
                at: at("oath", 1, 0),
                problem: ShiftProblem::SelfRelation { index: 0 },
            }
            .to_string(),
            "a faction can't have a relation with itself"
        );
    }

    #[test]
    fn next_names_a_later_stage_of_the_same_quest() {
        assert_eq!(
            problems_after(|quests| {
                let stages = &mut oath(quests).stages;
                stages[1].choices[0].next = Next::Stage(stage_id("swear"));
                stages[0].choices[1].next = Next::Stage(stage_id("patrol"));
            }),
            [
                QuestProblem::BackwardNext {
                    at: at("oath", 0, 1),
                    next: stage_id("patrol"),
                },
                QuestProblem::BackwardNext {
                    at: at("oath", 1, 0),
                    next: stage_id("swear"),
                },
            ]
        );
        assert_eq!(
            problems_after(|quests| {
                oath(quests).stages[0].choices[0].next = Next::Stage(stage_id("swaer"));
                quests
                    .quests
                    .get_mut(&quest_id("errand"))
                    .expect("there")
                    .stages[0]
                    .choices[0]
                    .next = Next::Stage(stage_id("run_away"));
            }),
            [
                QuestProblem::UnknownNext {
                    at: at("errand", 0, 0),
                    next: stage_id("run_away"),
                    suggestion: None,
                },
                QuestProblem::UnknownNext {
                    at: at("oath", 0, 0),
                    next: stage_id("swaer"),
                    suggestion: Some(stage_id("swear")),
                },
            ]
        );
    }

    #[test]
    fn standing_requirements_name_parties_that_exist_within_range() {
        let requires = Requirements {
            standing: [
                (party("watch"), h(100_00)),
                (party("hale"), h(-100_00)),
                (party("guild"), h(100_01)),
                (party("wtch"), h(0)),
            ]
            .into(),
            ..Requirements::default()
        };
        assert_eq!(
            problems_after(|quests| oath(quests).requires = requires),
            [
                QuestProblem::RequirementOutOfRange {
                    gate: Gate::Quest(quest_id("oath")),
                    party: party("guild"),
                    value: h(100_01),
                },
                QuestProblem::UnknownParty {
                    gate: Gate::Quest(quest_id("oath")),
                    party: party("wtch"),
                    suggestion: Some("watch".to_owned()),
                },
            ]
        );
    }

    #[test]
    fn faction_lists_name_factions_that_exist_once_each() {
        let requires = Requirements {
            member: vec![faction_id("watch"), faction_id("wach"), faction_id("watch")],
            not_member: vec![faction_id("guild"), faction_id("temple")],
            within_tolerance: vec![faction_id("watch"), faction_id("watch")],
            ..Requirements::default()
        };
        let gate = || Gate::Stage {
            quest: quest_id("oath"),
            stage: 1,
        };
        assert_eq!(
            problems_after(|quests| oath(quests).stages[1].requires = requires),
            [
                QuestProblem::UnknownFaction {
                    gate: gate(),
                    key: RequirementKey::Member(1),
                    faction: faction_id("wach"),
                    suggestion: Some(faction_id("watch")),
                },
                QuestProblem::Repeated {
                    gate: gate(),
                    key: RequirementKey::Member(2),
                    entry: "watch".to_owned(),
                },
                QuestProblem::UnknownFaction {
                    gate: gate(),
                    key: RequirementKey::NotMember(1),
                    faction: faction_id("temple"),
                    suggestion: None,
                },
                QuestProblem::Repeated {
                    gate: gate(),
                    key: RequirementKey::WithinTolerance(1),
                    entry: "watch".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn rank_requirements_name_a_rung_of_a_faction_that_exists() {
        let requires = Requirements {
            rank_at_least: [
                (faction_id("watch"), rank("sergaent")),
                (faction_id("guild"), rank("cutpurse")),
                (faction_id("temple"), rank("acolyte")),
            ]
            .into(),
            ..Requirements::default()
        };
        let gate = || Gate::Step {
            questline: line_id("career"),
            step: 1,
        };
        assert_eq!(
            problems_after(|quests| career(quests).steps[1].requires = requires),
            [
                QuestProblem::UnknownFaction {
                    gate: gate(),
                    key: RequirementKey::RankAtLeast(faction_id("temple")),
                    faction: faction_id("temple"),
                    suggestion: None,
                },
                QuestProblem::UnknownRank {
                    gate: gate(),
                    faction: faction_id("watch"),
                    rank: rank("sergaent"),
                    suggestion: Some(rank("sergeant")),
                },
            ]
        );
    }

    #[test]
    fn done_names_progress_that_exists_once_each() {
        let parse = |text: &str| Progress::parse(text).expect("valid");
        let requires = Requirements {
            done: vec![
                parse("oath"),
                parse("oath.swear"),
                parse("oath.patrol.look_away"),
                parse("oth"),
                parse("oath.patrl"),
                parse("oath.patrol.reprt"),
                parse("oath"),
                parse("errand.run.go"),
            ],
            ..Requirements::default()
        };
        let gate = || Gate::Quest(quest_id("errand"));
        assert_eq!(
            problems_after(|quests| {
                quests
                    .quests
                    .get_mut(&quest_id("errand"))
                    .expect("there")
                    .requires = requires;
            }),
            [
                QuestProblem::UnknownProgress {
                    gate: gate(),
                    index: 3,
                    missing: parse("oth"),
                    suggestion: Some("oath".to_owned()),
                },
                QuestProblem::UnknownProgress {
                    gate: gate(),
                    index: 4,
                    missing: parse("oath.patrl"),
                    suggestion: Some("patrol".to_owned()),
                },
                QuestProblem::UnknownProgress {
                    gate: gate(),
                    index: 5,
                    missing: parse("oath.patrol.reprt"),
                    suggestion: Some("report".to_owned()),
                },
                QuestProblem::Repeated {
                    gate: gate(),
                    key: RequirementKey::Done(6),
                    entry: "oath".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn questlines_have_steps_and_steps_have_quests_that_exist() {
        assert_eq!(
            problems_after(|quests| career(quests).steps.clear()),
            [QuestProblem::NoSteps(line_id("career"))]
        );
        assert_eq!(
            problems_after(|quests| {
                let steps = &mut career(quests).steps;
                steps[1].quests.clear();
                steps.push(step(&["errnd"]));
            }),
            [
                QuestProblem::EmptyStep {
                    questline: line_id("career"),
                    step: 1,
                },
                QuestProblem::UnknownQuest {
                    questline: line_id("career"),
                    step: 2,
                    index: 0,
                    quest: quest_id("errnd"),
                    suggestion: Some(quest_id("errand")),
                },
            ]
        );
    }

    #[test]
    fn a_quest_is_in_one_questline_at_one_step() {
        assert_eq!(
            problems_after(|quests| {
                career(quests).steps[1].quests.push(quest_id("oath"));
                let other = Questline {
                    id: line_id("detour"),
                    name: "A Detour".to_owned(),
                    giver: None,
                    steps: vec![step(&["errand", "errand"])],
                };
                quests.questlines.insert(other.id.clone(), other);
            }),
            [
                QuestProblem::QuestRepeated {
                    questline: line_id("career"),
                    step: 1,
                    index: 1,
                    quest: quest_id("oath"),
                    first: (line_id("career"), 0),
                },
                QuestProblem::QuestRepeated {
                    questline: line_id("detour"),
                    step: 0,
                    index: 0,
                    quest: quest_id("errand"),
                    first: (line_id("career"), 1),
                },
                QuestProblem::QuestRepeated {
                    questline: line_id("detour"),
                    step: 0,
                    index: 1,
                    quest: quest_id("errand"),
                    first: (line_id("career"), 1),
                },
            ]
        );
    }

    #[test]
    fn a_step_needs_at_most_all_its_quests() {
        assert_eq!(
            problems_after(|quests| {
                let steps = &mut career(quests).steps;
                steps[0].need = Some(1);
                steps[1].need = Some(0);
            }),
            []
        );
        assert_eq!(
            problems_after(|quests| career(quests).steps[1].need = Some(2)),
            [QuestProblem::NeedTooMany {
                questline: line_id("career"),
                step: 1,
                need: 2,
                quests: 1,
            }]
        );
    }

    #[test]
    fn closing_leftovers_warns_only_when_none_can_be_left() {
        let mut quests = quests();
        let steps = &mut career(&mut quests).steps;
        steps[0].leftovers = Leftovers::Close;
        steps[1].leftovers = Leftovers::Close;
        steps[1].quests.push(quest_id("oath"));
        steps[1].need = Some(1);
        assert_eq!(
            quests.warnings(),
            [QuestWarning::LeftoversNeverLeft {
                questline: line_id("career"),
                step: 0,
            }]
        );
        career(&mut quests).steps[1].need = Some(2);
        assert_eq!(quests.warnings().len(), 2);
        assert_eq!(self::quests().warnings(), []);
    }

    #[test]
    fn problems_say_what_is_wrong_in_a_designers_words() {
        let messages: Vec<String> = [
            QuestProblem::UnknownGiver {
                owner: Owner::Quest(quest_id("oath")),
                giver: party("wach"),
                suggestion: Some("watch".to_owned()),
            },
            QuestProblem::UnknownParty {
                gate: Gate::Quest(quest_id("oath")),
                party: party("zed"),
                suggestion: None,
            },
            QuestProblem::NoStages(quest_id("oath")),
            QuestProblem::DuplicateStage {
                quest: quest_id("oath"),
                stage: 1,
                id: stage_id("patrol"),
            },
            QuestProblem::StageCalledEnd {
                quest: quest_id("oath"),
                stage: 0,
            },
            QuestProblem::NoChoices {
                quest: quest_id("oath"),
                stage: 0,
            },
            QuestProblem::DuplicateChoice {
                at: at("oath", 0, 1),
                id: ChoiceId::new("report").expect("valid"),
            },
            QuestProblem::UnknownOutcome {
                at: at("oath", 0, 0),
                outcome: outcome_id("fine"),
                suggestion: Some(outcome_id("fined")),
            },
            QuestProblem::UnknownEffectParty {
                at: at("oath", 0, 0),
                party: Party::Character(CharacterId::new("hal").expect("valid")),
                suggestion: None,
            },
            QuestProblem::UnknownEffectParty {
                at: at("oath", 0, 0),
                party: Party::Faction(faction_id("guil")),
                suggestion: Some(Party::Faction(faction_id("guild"))),
            },
            QuestProblem::EffectOutOfRange {
                at: at("oath", 0, 0),
                party: Party::Faction(faction_id("guild")),
                value: h(-100_01),
            },
            QuestProblem::RequirementOutOfRange {
                gate: Gate::Quest(quest_id("oath")),
                party: party("guild"),
                value: h(100_01),
            },
            QuestProblem::UnknownNext {
                at: at("oath", 0, 0),
                next: stage_id("swaer"),
                suggestion: Some(stage_id("swear")),
            },
            QuestProblem::BackwardNext {
                at: at("oath", 1, 0),
                next: stage_id("patrol"),
            },
            QuestProblem::UnknownFaction {
                gate: Gate::Quest(quest_id("oath")),
                key: RequirementKey::Member(0),
                faction: faction_id("wach"),
                suggestion: Some(faction_id("watch")),
            },
            QuestProblem::UnknownRank {
                gate: Gate::Quest(quest_id("oath")),
                faction: faction_id("watch"),
                rank: rank("sargeant"),
                suggestion: Some(rank("sergeant")),
            },
            QuestProblem::Repeated {
                gate: Gate::Quest(quest_id("oath")),
                key: RequirementKey::Member(1),
                entry: "watch".to_owned(),
            },
            QuestProblem::UnknownProgress {
                gate: Gate::Quest(quest_id("oath")),
                index: 0,
                missing: Progress::parse("oth").expect("valid"),
                suggestion: Some("oath".to_owned()),
            },
            QuestProblem::UnknownProgress {
                gate: Gate::Quest(quest_id("oath")),
                index: 0,
                missing: Progress::parse("oath.vault").expect("valid"),
                suggestion: None,
            },
            QuestProblem::UnknownProgress {
                gate: Gate::Quest(quest_id("oath")),
                index: 0,
                missing: Progress::parse("oath.patrol.reprt").expect("valid"),
                suggestion: Some("report".to_owned()),
            },
            QuestProblem::NoSteps(line_id("career")),
            QuestProblem::EmptyStep {
                questline: line_id("career"),
                step: 0,
            },
            QuestProblem::UnknownQuest {
                questline: line_id("career"),
                step: 0,
                index: 0,
                quest: quest_id("errnd"),
                suggestion: Some(quest_id("errand")),
            },
            QuestProblem::QuestRepeated {
                questline: line_id("detour"),
                step: 0,
                index: 1,
                quest: quest_id("errand"),
                first: (line_id("career"), 1),
            },
            QuestProblem::NeedTooMany {
                questline: line_id("career"),
                step: 1,
                need: 2,
                quests: 1,
            },
            QuestProblem::NeedTooMany {
                questline: line_id("career"),
                step: 1,
                need: 4,
                quests: 3,
            },
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        assert_eq!(
            messages,
            [
                "unknown faction or character 'wach' (did you mean 'watch'?)",
                "unknown faction or character 'zed'",
                "a quest needs at least one stage",
                "another stage is already called 'patrol'",
                "a stage can't be called 'end': next = \"end\" ends the quest",
                "a stage needs at least one choice",
                "another choice in this stage is already called 'report'",
                "unknown outcome 'fine' (did you mean 'fined'?)",
                "unknown character 'hal'",
                "unknown faction 'guil' (did you mean 'guild'?)",
                "-100.01 is outside -100.00..100.00",
                "100.01 is outside -100.00..100.00",
                "unknown stage 'swaer' in this quest (did you mean 'swear'?)",
                "choices only lead forward, and 'patrol' isn't after this stage",
                "unknown faction 'wach' (did you mean 'watch'?)",
                "unknown rank 'sargeant' for watch (did you mean 'sergeant'?)",
                "'watch' is listed twice",
                "unknown quest 'oth' (did you mean 'oath'?)",
                "unknown stage 'vault' in oath",
                "unknown choice 'reprt' in oath.patrol (did you mean 'report'?)",
                "a questline needs at least one step",
                "a step needs at least one quest",
                "unknown quest 'errnd' (did you mean 'errand'?)",
                "'errand' is already at career.steps[1]: a quest is in at most one questline, at one step",
                "need is 2, but the step has only 1 quest",
                "need is 4, but the step has only 3 quests",
            ]
        );
        assert_eq!(
            QuestWarning::LeftoversNeverLeft {
                questline: line_id("career"),
                step: 0,
            }
            .to_string(),
            "leftovers = \"close\" has no effect: the step needs all its quests, so none are left over"
        );
    }
}
