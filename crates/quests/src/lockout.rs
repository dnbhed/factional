//! The lockout check (Q4, DESIGN.md §17.3, D-26, D-27, P-65): every choice that can make
//! another quest's gate or stage false for good declares it in `locks`. Loading never plays
//! the game out: it works out, from content alone, what each choice can do at worst, and what
//! the character can always undo, then checks each choice against each gate and stage,
//! requirement by requirement, never combinations.

use std::collections::{BTreeMap, BTreeSet};

use factional_core::{Fixed, Ratio};
use factional_reputation::{
    AXIS_LIMIT, AlignmentDelta, Axis, Consequence, Content, DriftPolicy, Effects, FactionId, Party,
    RankId, Toward,
};

use crate::check::{ChoiceAt, QuestProblem, QuestWarning, resolve};
use crate::quest::{Choice, ChoiceEffects, Lock, Progress, Quest, QuestId, Quests, Requirements};

/// Why a choice can make a requirement false for good.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockReason {
    /// It can lower standing with `party`, which no action raises, below `at_least`.
    Standing { party: Party, at_least: Fixed },
    /// It can start a war between `faction` and `other`, which ends one of the memberships
    /// of anyone in both.
    War {
        faction: FactionId,
        other: FactionId,
        needed: Needed,
    },
    /// It moves alignment `toward`, and `faction` demotes or expels members who drift out of
    /// its tolerance at once.
    Drift {
        faction: FactionId,
        toward: Toward,
        consequence: Consequence,
        needed: Needed,
    },
    /// It moves alignment `toward`, which no action moves back, so `faction`'s probation
    /// can run out.
    Probation {
        faction: FactionId,
        toward: Toward,
        needed: Needed,
    },
    /// It moves alignment `toward`, which no action moves back, so it can take the character
    /// outside `faction`'s member tolerance.
    Tolerance { faction: FactionId, toward: Toward },
}

/// What a requirement needs of a faction that a lost membership takes away.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Needed {
    Membership,
    Rank(RankId),
}

impl Quests {
    /// Every undeclared lockout: each choice in order (quests in id order, then stages and
    /// choices in order), against each other quest in id order, its gate and then its
    /// stages. Assumes every reference resolves.
    pub(crate) fn lockout_problems(&self, content: &Content) -> Vec<QuestProblem> {
        let facts = Facts::of(content, self);
        let mut problems = Vec::new();
        for (at, choice) in self.choices() {
            let Some(bounds) = facts.bounds(&at, choice) else {
                continue;
            };
            for target in self.targets(&at) {
                if choice.locks.contains(&target) {
                    continue;
                }
                if let Some(reason) = facts.lockout(&bounds, &self.requirements(&target)) {
                    problems.push(QuestProblem::Lockout {
                        at: at.clone(),
                        target,
                        reason,
                    });
                }
            }
        }
        problems
    }

    /// Every declared lock that can't happen, in choice order, then in the order declared.
    /// Assumes every reference resolves.
    pub(crate) fn stale_locks(&self, content: &Content) -> Vec<QuestWarning> {
        let facts = Facts::of(content, self);
        let mut warnings = Vec::new();
        for (at, choice) in self.choices() {
            let bounds = facts.bounds(&at, choice);
            let targets = self.targets(&at);
            for (index, lock) in choice.locks.iter().enumerate() {
                let possible = bounds.as_ref().is_some_and(|bounds| {
                    targets.contains(lock)
                        && facts.lockout(bounds, &self.requirements(lock)).is_some()
                });
                if !possible {
                    warnings.push(QuestWarning::StaleLock {
                        at: at.clone(),
                        index,
                        lock: lock.clone(),
                    });
                }
            }
        }
        warnings
    }

    /// Every choice with its place, in order.
    fn choices(&self) -> Vec<(ChoiceAt, &Choice)> {
        self.quests
            .values()
            .flat_map(|quest| {
                quest.stages.iter().enumerate().flat_map(move |(stage, s)| {
                    s.choices.iter().enumerate().map(move |(place, choice)| {
                        let at = ChoiceAt {
                            quest: quest.id.clone(),
                            stage,
                            choice: place,
                        };
                        (at, choice)
                    })
                })
            })
            .collect()
    }

    /// What the choice at `at` could lock out: every other quest's gate and stages, except
    /// quests certainly finished before the choice can be made.
    fn targets(&self, at: &ChoiceAt) -> Vec<Lock> {
        let finished = self.finished_before(at);
        self.quests
            .values()
            .filter(|quest| quest.id != at.quest && !finished.contains(&quest.id))
            .flat_map(|quest| {
                let stages = quest
                    .stages
                    .iter()
                    .map(|stage| Lock::Stage(quest.id.clone(), stage.id.clone()));
                std::iter::once(Lock::Gate(quest.id.clone())).chain(stages)
            })
            .collect()
    }

    /// The requirements a lock target needs: a quest's gate is its own and its step's; a
    /// stage's are its own.
    fn requirements(&self, target: &Lock) -> Vec<&Requirements> {
        let quest = &self.quests[target.quest()];
        match target {
            Lock::Gate(_) => {
                let step = self
                    .place_of(&quest.id)
                    .map(|(line, step)| &self.questlines[line].steps[step].requires);
                std::iter::once(&quest.requires).chain(step).collect()
            }
            Lock::Stage(_, stage) => quest
                .stages
                .iter()
                .filter(|s| &s.id == stage)
                .map(|s| &s.requires)
                .collect(),
        }
    }

    /// The quests certainly finished before the choice at `at` can be made: those its stage
    /// or its quest's gate needs done, every quest of an earlier step of its questline that
    /// needs all of them, and, in turn, those finished before each of these started.
    fn finished_before(&self, at: &ChoiceAt) -> BTreeSet<QuestId> {
        let quest = &self.quests[&at.quest];
        let mut finished = BTreeSet::new();
        let mut waiting = self.finished_before_start(quest);
        waiting.extend(finished_in(&quest.stages[at.stage].requires));
        while let Some(done) = waiting.pop() {
            if finished.insert(done.clone())
                && let Some(earlier) = self.quests.get(&done)
            {
                waiting.extend(self.finished_before_start(earlier));
            }
        }
        finished
    }

    /// The quests a quest's gate certainly needs finished before it starts.
    fn finished_before_start(&self, quest: &Quest) -> Vec<QuestId> {
        let mut finished = finished_in(&quest.requires);
        if let Some((line, step)) = self.place_of(&quest.id) {
            let steps = &self.questlines[line].steps;
            finished.extend(finished_in(&steps[step].requires));
            for earlier in &steps[..step] {
                if earlier.needed() == earlier.quests.len() {
                    finished.extend(earlier.quests.iter().cloned());
                }
            }
        }
        finished
    }
}

/// The quests a set of requirements needs finished.
fn finished_in(requires: &Requirements) -> Vec<QuestId> {
    requires
        .done
        .iter()
        .filter_map(|progress| match progress {
            Progress::Quest(quest) => Some(quest.clone()),
            Progress::Stage(..) | Progress::Choice(..) => None,
        })
        .collect()
}

/// What's worked out once per world (P-65): what the character can always undo, and how far
/// each relation can go.
struct Facts<'c> {
    content: &'c Content,
    /// The parties whose standing some action raises, directly. A spill can't be counted on:
    /// it comes from the change as applied, and once standing with the source reaches 100
    /// nothing more spills.
    raisable: BTreeSet<Party>,
    /// The ways some action moves an axis, with every inertia profile's curve for that way
    /// above 0 everywhere, so repeating it always moves the character.
    movable: BTreeSet<Toward>,
    /// Each direction between two factions, with what can move it.
    relations: BTreeMap<(FactionId, FactionId), Span>,
}

/// What a relation in one direction starts at, and what in the content can move it.
#[derive(Default)]
struct Span {
    start: Fixed,
    /// Whether an outcome lowers or raises it: outcomes can be applied any number of times,
    /// so they can take it all the way.
    outcome_lowers: bool,
    outcome_raises: bool,
    /// Each choice's own shift of it, made at most once.
    choices: Vec<(ChoiceAt, Fixed)>,
}

impl Span {
    /// The lowest and highest the relation can reach, leaving out the choice at `without`.
    fn range(&self, without: Option<&ChoiceAt>) -> (Fixed, Fixed) {
        let others = self
            .choices
            .iter()
            .filter(|(at, _)| Some(at) != without)
            .map(|(_, by)| i128::from(by.hundredths()));
        let (down, up): (i128, i128) =
            others.fold((0, 0), |(down, up), by| (down + by.min(0), up + by.max(0)));
        let limit = i128::from(AXIS_LIMIT.hundredths());
        let start = i128::from(self.start.hundredths());
        let reach = |total: i128| {
            let clamped = total.clamp(-limit, limit);
            Fixed::from_hundredths(i64::try_from(clamped).expect("within ±100"))
        };
        let low = if self.outcome_lowers {
            -AXIS_LIMIT
        } else {
            reach(start + down)
        };
        let high = if self.outcome_raises {
            AXIS_LIMIT
        } else {
            reach(start + up)
        };
        (low, high)
    }
}

/// The worst one choice can do (P-65).
struct Bounds {
    /// The parties whose standing it can lower, directly or by one hop of spillover.
    lowers: BTreeSet<Party>,
    /// The ways it moves each axis.
    moves: Vec<Toward>,
    /// The pairs of factions it can set at war, as `(from, to)` of the direction that falls.
    wars: Vec<(FactionId, FactionId)>,
}

impl<'c> Facts<'c> {
    fn of(content: &'c Content, quests: &Quests) -> Facts<'c> {
        let mut raisable = BTreeSet::new();
        let mut movable = BTreeSet::new();
        let profiles = &content.balance.inertia.profiles;
        for action in content.actions.values() {
            for (party, change) in action.standing.named.parties() {
                if change > Fixed::ZERO {
                    raisable.insert(party);
                }
            }
            if action
                .standing
                .target
                .is_some_and(|change| change > Fixed::ZERO)
            {
                raisable.extend(content.characters.keys().cloned().map(Party::Character));
            }
            for axis in [Axis::Law, Axis::Good] {
                let Some(toward) = Toward::of(axis, on(action.alignment, axis)) else {
                    continue;
                };
                let never_stops = profiles.values().all(|profile| {
                    profile
                        .curves
                        .get(&toward)
                        .is_none_or(|curve| curve.lowest() > Fixed::ZERO)
                });
                if never_stops {
                    movable.insert(toward);
                }
            }
        }
        let starts = content.relation_values();
        let mut relations: BTreeMap<(FactionId, FactionId), Span> = BTreeMap::new();
        for from in content.factions.keys() {
            for to in content.factions.keys().filter(|to| *to != from) {
                let direction = (from.clone(), to.clone());
                let start = starts.get(&direction).copied().unwrap_or_default();
                relations.insert(
                    direction,
                    Span {
                        start,
                        ..Span::default()
                    },
                );
            }
        }
        for outcome in content.outcomes.values() {
            for shift in &outcome.effects.relations {
                for direction in shift.ends.directions() {
                    if let Some(span) = relations.get_mut(&direction) {
                        span.outcome_lowers |= shift.by < Fixed::ZERO;
                        span.outcome_raises |= shift.by > Fixed::ZERO;
                    }
                }
            }
        }
        for (at, choice) in quests.choices() {
            let ChoiceEffects::Inline(effects) = &choice.effects else {
                continue;
            };
            for shift in &effects.relations {
                for direction in shift.ends.directions() {
                    if let Some(span) = relations.get_mut(&direction) {
                        span.choices.push((at.clone(), shift.by));
                    }
                }
            }
        }
        Facts {
            content,
            raisable,
            movable,
            relations,
        }
    }

    /// The worst the choice at `at` can do; `None` if it has no effects.
    fn bounds(&self, at: &ChoiceAt, choice: &Choice) -> Option<Bounds> {
        let effects: &Effects = match &choice.effects {
            ChoiceEffects::None => return None,
            ChoiceEffects::Outcome(outcome) => &self.content.outcomes.get(outcome)?.effects,
            ChoiceEffects::Inline(effects) => effects,
        };
        let mut lowers = BTreeSet::new();
        for (party, change) in effects.standing.parties() {
            if change < Fixed::ZERO {
                lowers.insert(party);
            }
        }
        // One hop of spillover from each faction's change, over every relation the receiving
        // faction can hold toward it (DESIGN.md §7.1): the curve is straight between its
        // points, so its extremes over a range are at the range's ends or its points.
        let spillover = &self.content.balance.spillover;
        for (from, change) in &effects.standing.factions {
            for to in self.content.factions.keys().filter(|to| *to != from) {
                let (low, high) = self.relations[&(to.clone(), from.clone())].range(None);
                let inside = spillover
                    .points()
                    .into_iter()
                    .map(|(x, _)| x)
                    .filter(|x| (low..=high).contains(x));
                let can_fall = [low, high].into_iter().chain(inside).any(|relation| {
                    Ratio::from_fixed(*change)
                        .checked_mul(spillover.exact_at(relation))
                        .and_then(Ratio::round)
                        .is_some_and(|amount| amount < Fixed::ZERO)
                });
                if can_fall {
                    lowers.insert(Party::Faction(to.clone()));
                }
            }
        }
        let moves = [Axis::Law, Axis::Good]
            .into_iter()
            .filter_map(|axis| Toward::of(axis, on(effects.alignment, axis)))
            .collect();
        // Two factions are in conflict when either regards the other at or below the
        // threshold (DESIGN.md §9.4), so a war opens when a direction falls there while
        // both were above it.
        let threshold = self.content.balance.conflict_threshold;
        let mut wars = Vec::new();
        for shift in effects.relations.iter().filter(|s| s.by < Fixed::ZERO) {
            for (from, to) in shift.ends.directions() {
                let range = |from: &FactionId, to: &FactionId| {
                    self.relations
                        .get(&(from.clone(), to.clone()))
                        .map(|span| span.range(Some(at)))
                };
                let (Some((low, high)), Some((_, back))) = (range(&from, &to), range(&to, &from))
                else {
                    continue;
                };
                // The threshold is within ±100, so the stop at -100 never matters here.
                if high > threshold && back > threshold && low + shift.by <= threshold {
                    wars.push((from, to));
                }
            }
        }
        Some(Bounds {
            lowers,
            moves,
            wars,
        })
    }

    /// The first requirement, in the order `Requirements::KEYS` lists them, that a choice
    /// with these bounds can make false for good.
    fn lockout(&self, bounds: &Bounds, requirements: &[&Requirements]) -> Option<LockReason> {
        requirements
            .iter()
            .find_map(|requires| self.first_lockout(bounds, requires))
    }

    fn first_lockout(&self, bounds: &Bounds, requires: &Requirements) -> Option<LockReason> {
        for (party, at_least) in &requires.standing {
            let Some(party) = resolve(party, self.content) else {
                continue;
            };
            // Standing never goes below -100, so needing at least -100 can't fail.
            if *at_least > -AXIS_LIMIT
                && bounds.lowers.contains(&party)
                && !self.raisable.contains(&party)
            {
                return Some(LockReason::Standing {
                    party,
                    at_least: *at_least,
                });
            }
        }
        let needs_membership = requires
            .member
            .iter()
            .map(|faction| (faction, Needed::Membership));
        let needs_rank = requires
            .rank_at_least
            .iter()
            .map(|(faction, rank)| (faction, Needed::Rank(rank.clone())));
        for (faction, needed) in needs_membership.chain(needs_rank) {
            if let Some(reason) = self.ends(bounds, faction, needed) {
                return Some(reason);
            }
        }
        for faction in &requires.within_tolerance {
            let toward = self
                .weighed(bounds, faction)
                .find(|toward| !self.movable.contains(&toward.opposite()));
            if let Some(toward) = toward {
                return Some(LockReason::Tolerance {
                    faction: faction.clone(),
                    toward,
                });
            }
        }
        None
    }

    /// How a choice with these bounds can end a membership of `faction`, and with it any
    /// rank: a war with another faction, or drift.
    fn ends(&self, bounds: &Bounds, faction: &FactionId, needed: Needed) -> Option<LockReason> {
        let war = bounds.wars.iter().find_map(|(from, to)| {
            if from == faction {
                Some(to)
            } else if to == faction {
                Some(from)
            } else {
                None
            }
        });
        if let Some(other) = war {
            return Some(LockReason::War {
                faction: faction.clone(),
                other: other.clone(),
                needed,
            });
        }
        let policy = self.content.factions[faction]
            .drift
            .unwrap_or(self.content.balance.default_drift);
        let mut moved = self.weighed(bounds, faction);
        let consequence = match policy {
            DriftPolicy::Ignore | DriftPolicy::Flag => return None,
            DriftPolicy::Demote => Consequence::Demote,
            DriftPolicy::Expel => Consequence::Expel,
            // Probation gives the character time to move back, if some action can.
            DriftPolicy::Probation { .. } => {
                return moved
                    .find(|toward| !self.movable.contains(&toward.opposite()))
                    .map(|toward| LockReason::Probation {
                        faction: faction.clone(),
                        toward,
                        needed,
                    });
            }
        };
        moved.next().map(|toward| LockReason::Drift {
            faction: faction.clone(),
            toward,
            consequence,
            needed,
        })
    }

    /// The ways a choice moves alignment along the axes `faction` weighs.
    fn weighed<'b>(
        &self,
        bounds: &'b Bounds,
        faction: &FactionId,
    ) -> impl Iterator<Item = Toward> + 'b {
        let weights = self.content.factions[faction]
            .weights
            .unwrap_or(self.content.balance.default_weights);
        bounds
            .moves
            .iter()
            .copied()
            .filter(move |toward| weights.on(toward.axis()) > Fixed::ZERO)
    }
}

/// A delta's part on one axis.
fn on(delta: AlignmentDelta, axis: Axis) -> Fixed {
    match axis {
        Axis::Law => delta.law,
        Axis::Good => delta.good,
    }
}

#[cfg(test)]
mod tests {
    use factional_core::Curve;
    use factional_reputation::{
        Action, ActionId, ActionStanding, Alignment, Balance, Character, CharacterId, Faction,
        InertiaProfile, Outcome, OutcomeId, ProfileId, Rank, Relation, RelationEnds, RelationShift,
        StandingEffects, Tolerances, Weights,
    };

    use super::*;
    use crate::quest::{
        ChoiceId, Leftovers, Next, PartyRef, Questline, QuestlineId, Stage, StageId, Step,
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

    fn watch() -> FactionId {
        faction_id("watch")
    }

    fn guild() -> FactionId {
        faction_id("guild")
    }

    /// The Watch and the Guild, each with one rank, `recruit`, no weights or drift of their
    /// own; Hale; balance defaults; no actions, relations or outcomes.
    fn content() -> Content {
        let faction = |id: &str| Faction {
            id: faction_id(id),
            name: id.to_owned(),
            alignment: Alignment::new(h(0), h(0)).expect("in range"),
            weights: None,
            tolerances: Tolerances::new(h(40_00), None).expect("valid"),
            leave_standing_change: h(0),
            ranks: vec![Rank {
                id: RankId::new("recruit").expect("valid id"),
                requires_standing: None,
                tolerance: None,
            }],
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
            factions: [(watch(), faction("watch")), (guild(), faction("guild"))].into(),
            actions: BTreeMap::new(),
            relations: Vec::new(),
            outcomes: BTreeMap::new(),
        }
    }

    /// An action, `id`, moving alignment by `delta` and changing standing with named parties.
    fn action(content: &mut Content, id: &str, delta: (i64, i64), named: StandingEffects) {
        let action = Action {
            id: ActionId::new(id).expect("valid id"),
            alignment: AlignmentDelta {
                law: h(delta.0),
                good: h(delta.1),
            },
            standing: ActionStanding {
                target: None,
                target_factions: None,
                named,
            },
            by_target: BTreeMap::new(),
        };
        content.actions.insert(action.id.clone(), action);
    }

    fn relation(content: &mut Content, from: &str, to: &str, value: i64) {
        content.relations.push(Relation {
            ends: RelationEnds::Directed {
                from: faction_id(from),
                to: faction_id(to),
            },
            value: h(value),
        });
    }

    fn standing(factions: &[(&str, i64)], characters: &[(&str, i64)]) -> StandingEffects {
        StandingEffects {
            factions: factions
                .iter()
                .map(|(id, value)| (faction_id(id), h(*value)))
                .collect(),
            characters: characters
                .iter()
                .map(|(id, value)| (CharacterId::new(id).expect("valid id"), h(*value)))
                .collect(),
        }
    }

    fn effects(
        delta: (i64, i64),
        named: StandingEffects,
        relations: Vec<RelationShift>,
    ) -> Effects {
        Effects {
            alignment: AlignmentDelta {
                law: h(delta.0),
                good: h(delta.1),
            },
            standing: named,
            relations,
        }
    }

    fn between(a: &str, b: &str, by: i64) -> RelationShift {
        RelationShift {
            ends: RelationEnds::Between(faction_id(a), faction_id(b)),
            by: h(by),
        }
    }

    fn one_stage(id: &str, requires: Requirements, effects: ChoiceEffects) -> Quest {
        Quest {
            id: quest_id(id),
            name: id.to_owned(),
            giver: None,
            requires: Requirements::default(),
            stages: vec![Stage {
                id: StageId::new("s").expect("valid id"),
                requires,
                choices: vec![Choice {
                    id: ChoiceId::new("c").expect("valid id"),
                    effects,
                    next: Next::End,
                    locks: Vec::new(),
                }],
            }],
        }
    }

    /// `act`, whose one choice has `effects`, and `wait`, whose gate needs `requires`.
    fn quests(effects: ChoiceEffects, requires: Requirements) -> Quests {
        let act = one_stage("act", Requirements::default(), effects);
        let mut wait = one_stage("wait", Requirements::default(), ChoiceEffects::None);
        wait.requires = requires;
        Quests {
            quests: [(act.id.clone(), act), (wait.id.clone(), wait)].into(),
            questlines: BTreeMap::new(),
        }
    }

    fn inline(effects: Effects) -> ChoiceEffects {
        ChoiceEffects::Inline(effects)
    }

    fn needs_standing(party: &str, at_least: i64) -> Requirements {
        Requirements {
            standing: [(PartyRef::new(party).expect("valid id"), h(at_least))].into(),
            ..Requirements::default()
        }
    }

    fn needs_member(faction: &str) -> Requirements {
        Requirements {
            member: vec![faction_id(faction)],
            ..Requirements::default()
        }
    }

    /// The reasons for each lockout found.
    fn reasons(content: &Content, quests: &Quests) -> Vec<LockReason> {
        quests
            .lockout_problems(content)
            .into_iter()
            .map(|problem| match problem {
                QuestProblem::Lockout { reason, .. } => reason,
                other => panic!("not a lockout: {other:?}"),
            })
            .collect()
    }

    fn lowered(party: Party, at_least: i64) -> Vec<LockReason> {
        vec![LockReason::Standing {
            party,
            at_least: h(at_least),
        }]
    }

    // Standing

    #[test]
    fn a_choice_without_effects_locks_nothing_and_its_locks_are_stale() {
        let mut quests = quests(ChoiceEffects::None, needs_standing("guild", 10_00));
        assert_eq!(reasons(&content(), &quests), []);
        let act = quests.quests.get_mut(&quest_id("act")).expect("there");
        act.stages[0].choices[0].locks = vec![Lock::Gate(quest_id("wait"))];
        assert_eq!(
            quests.stale_locks(&content()),
            [QuestWarning::StaleLock {
                at: ChoiceAt {
                    quest: quest_id("act"),
                    stage: 0,
                    choice: 0,
                },
                index: 0,
                lock: Lock::Gate(quest_id("wait")),
            }]
        );
    }

    #[test]
    fn lowering_standing_no_action_raises_locks_out_a_gate_needing_it() {
        let lower = inline(effects((0, 0), standing(&[("guild", -5_00)], &[]), vec![]));
        let quests = quests(lower, needs_standing("guild", 10_00));
        assert_eq!(
            reasons(&content(), &quests),
            lowered(Party::Faction(guild()), 10_00)
        );
    }

    #[test]
    fn an_outcomes_effects_are_the_choices() {
        let mut content = content();
        let fined = OutcomeId::new("fined").expect("valid id");
        content.outcomes.insert(
            fined.clone(),
            Outcome {
                id: fined.clone(),
                effects: effects((0, 0), standing(&[("guild", -5_00)], &[]), vec![]),
            },
        );
        let quests = quests(
            ChoiceEffects::Outcome(fined),
            needs_standing("guild", 10_00),
        );
        assert_eq!(
            reasons(&content, &quests),
            lowered(Party::Faction(guild()), 10_00)
        );
    }

    #[test]
    fn a_change_of_nothing_lowers_nothing() {
        let none = inline(effects((0, 0), standing(&[("guild", 0)], &[]), vec![]));
        assert_eq!(
            reasons(&content(), &quests(none, needs_standing("guild", 10_00))),
            []
        );
    }

    #[test]
    fn a_lock_on_a_quest_the_choice_can_reach_but_not_lock_out_is_stale() {
        let raise = inline(effects((0, 0), standing(&[("guild", 5_00)], &[]), vec![]));
        let mut quests = quests(raise, needs_standing("guild", 10_00));
        let act = quests.quests.get_mut(&quest_id("act")).expect("there");
        act.stages[0].choices[0].locks = vec![Lock::Gate(quest_id("wait"))];
        assert_eq!(quests.stale_locks(&content()).len(), 1);
    }

    #[test]
    fn a_lock_on_a_quest_finished_first_is_stale() {
        let lower = inline(effects((0, 0), standing(&[("guild", -5_00)], &[]), vec![]));
        let mut quests = quests(lower, needs_standing("guild", 10_00));
        let act = quests.quests.get_mut(&quest_id("act")).expect("there");
        act.stages[0].choices[0].locks = vec![Lock::Gate(quest_id("wait"))];
        assert_eq!(quests.stale_locks(&content()), []);
        let act = quests.quests.get_mut(&quest_id("act")).expect("there");
        act.requires = Requirements {
            done: vec![Progress::Quest(quest_id("wait"))],
            ..Requirements::default()
        };
        assert_eq!(quests.stale_locks(&content()).len(), 1);
    }

    #[test]
    fn an_outcome_shifting_a_relation_by_nothing_leaves_its_reach_alone() {
        let mut content = content();
        let still = OutcomeId::new("still").expect("valid id");
        content.outcomes.insert(
            still.clone(),
            Outcome {
                id: still,
                effects: effects(
                    (0, 0),
                    StandingEffects::default(),
                    vec![between("watch", "guild", 0)],
                ),
            },
        );
        let facts = Facts::of(&content, &Quests::default());
        assert_eq!(
            facts.relations[&(watch(), guild())].range(None),
            (h(0), h(0))
        );
    }

    #[test]
    fn needing_standing_of_minus_100_can_never_fail() {
        let lower = inline(effects((0, 0), standing(&[("guild", -5_00)], &[]), vec![]));
        assert_eq!(
            reasons(
                &content(),
                &quests(lower.clone(), needs_standing("guild", -100_00))
            ),
            []
        );
        assert_eq!(
            reasons(&content(), &quests(lower, needs_standing("guild", -99_99))),
            lowered(Party::Faction(guild()), -99_99)
        );
    }

    #[test]
    fn standing_an_action_raises_directly_is_never_locked_out() {
        let lower = inline(effects((0, 0), standing(&[("guild", -5_00)], &[]), vec![]));
        let quests = quests(lower, needs_standing("guild", 10_00));
        let mut content = content();
        action(
            &mut content,
            "insult",
            (0, 0),
            standing(&[("guild", -1_00)], &[]),
        );
        action(
            &mut content,
            "shrug",
            (0, 0),
            standing(&[("guild", 0)], &[]),
        );
        assert_eq!(
            reasons(&content, &quests),
            lowered(Party::Faction(guild()), 10_00),
            "lowering or leaving it alone isn't raising it"
        );
        action(
            &mut content,
            "flatter",
            (0, 0),
            standing(&[("guild", 1)], &[]),
        );
        assert_eq!(reasons(&content, &quests), []);
    }

    #[test]
    fn a_characters_standing_is_raised_by_any_action_done_to_them() {
        let lower = inline(effects((0, 0), standing(&[], &[("hale", -5_00)]), vec![]));
        let quests = quests(lower, needs_standing("hale", 10_00));
        let mut content = content();
        let hale = Party::Character(CharacterId::new("hale").expect("valid id"));
        assert_eq!(reasons(&content, &quests), lowered(hale, 10_00));
        action(&mut content, "help", (0, 0), StandingEffects::default());
        let help = content
            .actions
            .get_mut(&ActionId::new("help").expect("valid id"))
            .expect("there");
        help.standing.target = Some(h(0));
        assert_eq!(
            reasons(&content, &quests).len(),
            1,
            "a change of 0 raises nothing"
        );
        let help = content
            .actions
            .get_mut(&ActionId::new("help").expect("valid id"))
            .expect("there");
        help.standing.target = Some(h(10_00));
        assert_eq!(reasons(&content, &quests), []);
    }

    // Spillover (DESIGN.md §7.1): by default, -0.30 at -100, 0 from -50 to 50, 0.50 at 100.

    /// A choice giving the Watch `change`, against a gate needing standing 10 with the Guild.
    fn spill(content: &Content, change: i64) -> Vec<LockReason> {
        let raise = inline(effects((0, 0), standing(&[("watch", change)], &[]), vec![]));
        reasons(content, &quests(raise, needs_standing("guild", 10_00)))
    }

    #[test]
    fn standing_spills_by_how_the_receiver_regards_the_faction() {
        let mut content = content();
        assert_eq!(spill(&content, 10_00), [], "at 0, nothing spills");
        relation(&mut content, "guild", "watch", -100_00);
        // 10 × -0.30 = -3.00.
        assert_eq!(
            spill(&content, 10_00),
            lowered(Party::Faction(guild()), 10_00)
        );
        // -10 × -0.30 = 3.00, a rise.
        assert_eq!(spill(&content, -10_00), []);
        relation(&mut content, "watch", "guild", 100_00);
        assert_eq!(
            spill(&content, -10_00),
            [],
            "only the receiver's regard counts"
        );
    }

    #[test]
    fn a_loss_spills_through_friendship() {
        let mut content = content();
        relation(&mut content, "guild", "watch", 100_00);
        // -10 × 0.50 = -5.00.
        assert_eq!(
            spill(&content, -10_00),
            lowered(Party::Faction(guild()), 10_00)
        );
        assert_eq!(spill(&content, 10_00), []);
    }

    #[test]
    fn a_spill_that_rounds_to_nothing_lowers_nothing() {
        let mut content = content();
        relation(&mut content, "guild", "watch", -100_00);
        // 0.01 × -0.30 = -0.003, which rounds to 0.00; 0.02 × -0.30 = -0.006, to -0.01.
        assert_eq!(spill(&content, 1), []);
        assert_eq!(spill(&content, 2), lowered(Party::Faction(guild()), 10_00));
    }

    #[test]
    fn spillover_is_bounded_over_every_relation_an_outcome_can_reach() {
        let mut content = content();
        let sour = OutcomeId::new("sour").expect("valid id");
        content.outcomes.insert(
            sour.clone(),
            Outcome {
                id: sour,
                effects: effects(
                    (0, 0),
                    StandingEffects::default(),
                    vec![RelationShift {
                        ends: RelationEnds::Directed {
                            from: guild(),
                            to: watch(),
                        },
                        by: h(-1),
                    }],
                ),
            },
        );
        // Starting at 0, but an outcome can take it to -100, where 10 × -0.30 = -3.00.
        assert_eq!(
            spill(&content, 10_00),
            lowered(Party::Faction(guild()), 10_00)
        );
    }

    #[test]
    fn spillover_is_bounded_at_every_point_of_the_curve_within_reach() {
        let mut content = content();
        // 0.50 at both ends, but -0.50 at 0.
        content.balance.spillover = Curve::from_points(vec![
            (h(-100_00), h(50)),
            (h(0), h(-50)),
            (h(100_00), h(50)),
        ])
        .expect("a curve");
        relation(&mut content, "guild", "watch", -100_00);
        assert_eq!(spill(&content, 10_00), [], "at -100, 10 × 0.50 is a rise");
        let rise = OutcomeId::new("rise").expect("valid id");
        content.outcomes.insert(
            rise.clone(),
            Outcome {
                id: rise,
                effects: effects(
                    (0, 0),
                    StandingEffects::default(),
                    vec![RelationShift {
                        ends: RelationEnds::Directed {
                            from: guild(),
                            to: watch(),
                        },
                        by: h(1),
                    }],
                ),
            },
        );
        // From -100 to 100, through 0, where 10 × -0.50 = -5.00.
        assert_eq!(
            spill(&content, 10_00),
            lowered(Party::Faction(guild()), 10_00)
        );
    }

    // Wars (DESIGN.md §9.4): the default conflict threshold is -50.

    /// The reasons a choice shifting the Watch and the Guild by `by` locks out a gate
    /// needing membership of the Watch, with `other` also shifting them, in another quest.
    fn war(content: &Content, by: i64, other: Option<i64>) -> Vec<LockReason> {
        let shift = inline(effects(
            (0, 0),
            StandingEffects::default(),
            vec![between("watch", "guild", by)],
        ));
        let mut quests = quests(shift, needs_member("watch"));
        if let Some(by) = other {
            let also = one_stage(
                "also",
                Requirements::default(),
                inline(effects(
                    (0, 0),
                    StandingEffects::default(),
                    vec![between("watch", "guild", by)],
                )),
            );
            quests.quests.insert(also.id.clone(), also);
        }
        quests
            .lockout_problems(content)
            .into_iter()
            .filter_map(|problem| match problem {
                QuestProblem::Lockout { at, reason, .. } if at.quest == quest_id("act") => {
                    Some(reason)
                }
                _ => None,
            })
            .collect()
    }

    fn at_war() -> Vec<LockReason> {
        vec![LockReason::War {
            faction: watch(),
            other: guild(),
            needed: Needed::Membership,
        }]
    }

    #[test]
    fn a_shift_to_the_conflict_threshold_starts_a_war_that_can_end_a_membership() {
        assert_eq!(war(&content(), -50_00, None), at_war());
        assert_eq!(war(&content(), -49_99, None), []);
        assert_eq!(war(&content(), 50_00, None), []);
    }

    #[test]
    fn a_shift_of_nothing_starts_no_war() {
        assert_eq!(war(&content(), 0, Some(-60_00)), []);
        assert_eq!(war(&content(), -1, Some(-60_00)), at_war());
    }

    #[test]
    fn other_choices_shifts_count_toward_a_war_but_never_the_choices_own() {
        assert_eq!(war(&content(), -30_00, Some(-30_00)), at_war());
        assert_eq!(war(&content(), -30_00, Some(30_00)), []);
        assert_eq!(
            war(&content(), -30_00, None),
            [],
            "its own -30 isn't counted twice"
        );
    }

    #[test]
    fn factions_already_in_conflict_cant_start_a_war() {
        let mut content = content();
        relation(&mut content, "watch", "guild", -60_00);
        relation(&mut content, "guild", "watch", -60_00);
        assert_eq!(war(&content, -10_00, None), []);
        // Either direction at the threshold or below is a conflict already.
        content.relations.pop();
        assert_eq!(war(&content, -60_00, None), []);
        content.relations.pop();
        relation(&mut content, "watch", "guild", -50_00);
        assert_eq!(war(&content, -60_00, None), []);
        content.relations.pop();
        relation(&mut content, "watch", "guild", -49_99);
        assert_eq!(war(&content, -60_00, None), at_war());
    }

    #[test]
    fn a_war_ends_a_rank_with_the_membership() {
        let shift = inline(effects(
            (0, 0),
            StandingEffects::default(),
            vec![between("guild", "watch", -60_00)],
        ));
        let requires = Requirements {
            rank_at_least: [(watch(), RankId::new("recruit").expect("valid id"))].into(),
            ..Requirements::default()
        };
        assert_eq!(
            reasons(&content(), &quests(shift, requires)),
            [LockReason::War {
                faction: watch(),
                other: guild(),
                needed: Needed::Rank(RankId::new("recruit").expect("valid id")),
            }]
        );
    }

    #[test]
    fn a_war_between_other_factions_leaves_a_membership_alone() {
        let mut content = content();
        let temple = Faction {
            id: faction_id("temple"),
            ..content.factions[&watch()].clone()
        };
        content.factions.insert(temple.id.clone(), temple);
        let shift = inline(effects(
            (0, 0),
            StandingEffects::default(),
            vec![between("guild", "temple", -60_00)],
        ));
        assert_eq!(reasons(&content, &quests(shift, needs_member("watch"))), []);
    }

    // Drift (DESIGN.md §9.3)

    fn drift(content: &Content, delta: (i64, i64)) -> Vec<LockReason> {
        let shift = inline(effects(delta, StandingEffects::default(), vec![]));
        reasons(content, &quests(shift, needs_member("watch")))
    }

    fn set_drift(content: &mut Content, policy: DriftPolicy) {
        content.factions.get_mut(&watch()).expect("there").drift = Some(policy);
    }

    #[test]
    fn drift_that_only_flags_or_is_ignored_ends_nothing() {
        let mut content = content();
        assert_eq!(drift(&content, (5_00, 0)), [], "the default is to flag");
        set_drift(&mut content, DriftPolicy::Ignore);
        assert_eq!(drift(&content, (5_00, 0)), []);
    }

    #[test]
    fn drift_that_demotes_or_expels_at_once_can_end_a_membership() {
        let mut content = content();
        content.balance.default_drift = DriftPolicy::Expel;
        let ended = |toward, consequence| {
            vec![LockReason::Drift {
                faction: watch(),
                toward,
                consequence,
                needed: Needed::Membership,
            }]
        };
        assert_eq!(
            drift(&content, (5_00, 0)),
            ended(Toward::Lawful, Consequence::Expel)
        );
        set_drift(&mut content, DriftPolicy::Demote);
        assert_eq!(
            drift(&content, (0, -5_00)),
            ended(Toward::Evil, Consequence::Demote)
        );
        assert_eq!(drift(&content, (0, 0)), [], "no move, no drift");
    }

    #[test]
    fn drift_counts_only_along_axes_the_faction_weighs() {
        let mut content = content();
        set_drift(&mut content, DriftPolicy::Expel);
        content.factions.get_mut(&watch()).expect("there").weights =
            Some(Weights::new(h(0), h(1_00)).expect("valid"));
        assert_eq!(drift(&content, (5_00, 0)), []);
        assert_eq!(drift(&content, (5_00, 1)).len(), 1);
        content.factions.get_mut(&watch()).expect("there").weights = None;
        content.balance.default_weights = Weights::new(h(1_00), h(0)).expect("valid");
        assert_eq!(
            drift(&content, (0, 5_00)),
            [],
            "nor the world's default weights"
        );
    }

    #[test]
    fn probation_ends_a_membership_only_if_no_action_moves_back() {
        let mut content = content();
        set_drift(
            &mut content,
            DriftPolicy::Probation {
                grace_ticks: 10,
                then: Consequence::Expel,
            },
        );
        let on_probation = vec![LockReason::Probation {
            faction: watch(),
            toward: Toward::Chaotic,
            needed: Needed::Membership,
        }];
        assert_eq!(drift(&content, (-5_00, 0)), on_probation);
        action(&mut content, "lie", (-1_00, 0), StandingEffects::default());
        assert_eq!(
            drift(&content, (-5_00, 0)),
            on_probation,
            "lying moves the same way"
        );
        action(&mut content, "obey", (1, 0), StandingEffects::default());
        assert_eq!(drift(&content, (-5_00, 0)), []);
    }

    #[test]
    fn a_way_back_counts_only_if_no_inertia_profile_ever_stops_it() {
        let mut content = content();
        set_drift(
            &mut content,
            DriftPolicy::Probation {
                grace_ticks: 10,
                then: Consequence::Demote,
            },
        );
        action(&mut content, "obey", (1_00, 0), StandingEffects::default());
        let curve = |low: i64| {
            Curve::from_points(vec![(h(-100_00), h(low)), (h(100_00), h(1_00))]).expect("a curve")
        };
        let profile = |toward, low| InertiaProfile {
            curves: [(toward, curve(low))].into(),
        };
        let stubborn = ProfileId::new("stubborn").expect("valid id");
        content
            .balance
            .inertia
            .profiles
            .insert(stubborn.clone(), profile(Toward::Chaotic, 0));
        assert_eq!(drift(&content, (-5_00, 0)), [], "only the way back matters");
        content
            .balance
            .inertia
            .profiles
            .insert(stubborn.clone(), profile(Toward::Lawful, 1));
        assert_eq!(drift(&content, (-5_00, 0)), [], "slowed but never stopped");
        content
            .balance
            .inertia
            .profiles
            .insert(stubborn, profile(Toward::Lawful, 0));
        assert_eq!(drift(&content, (-5_00, 0)).len(), 1);
    }

    // Tolerance

    #[test]
    fn moving_where_no_action_moves_back_can_leave_a_factions_tolerance() {
        let mut content = content();
        let shift = inline(effects((0, -5_00), StandingEffects::default(), vec![]));
        let requires = Requirements {
            within_tolerance: vec![guild()],
            ..Requirements::default()
        };
        let quests = quests(shift, requires);
        assert_eq!(
            reasons(&content, &quests),
            [LockReason::Tolerance {
                faction: guild(),
                toward: Toward::Evil,
            }]
        );
        content.factions.get_mut(&guild()).expect("there").weights =
            Some(Weights::new(h(1_00), h(0)).expect("valid"));
        assert_eq!(
            reasons(&content, &quests),
            [],
            "the Guild doesn't weigh good"
        );
        content.factions.get_mut(&guild()).expect("there").weights = None;
        action(&mut content, "pray", (0, 1_00), StandingEffects::default());
        assert_eq!(reasons(&content, &quests), []);
    }

    // What's checked against what

    #[test]
    fn a_gate_includes_its_steps_requirements_and_stages_are_checked_too() {
        let lower = inline(effects((0, 0), standing(&[("guild", -5_00)], &[]), vec![]));
        let mut quests = quests(lower, Requirements::default());
        let line = Questline {
            id: QuestlineId::new("line").expect("valid id"),
            name: "Line".to_owned(),
            giver: None,
            steps: vec![Step {
                quests: vec![quest_id("wait")],
                need: None,
                requires: needs_standing("guild", 10_00),
                leftovers: Leftovers::Open,
            }],
        };
        quests.questlines.insert(line.id.clone(), line);
        let wait = quests.quests.get_mut(&quest_id("wait")).expect("there");
        wait.stages[0].requires = needs_standing("guild", 20_00);
        let found: Vec<(Lock, LockReason)> = quests
            .lockout_problems(&content())
            .into_iter()
            .filter_map(|problem| match problem {
                QuestProblem::Lockout { target, reason, .. } => Some((target, reason)),
                _ => None,
            })
            .collect();
        let guild = Party::Faction(guild());
        assert_eq!(
            found,
            [
                (
                    Lock::Gate(quest_id("wait")),
                    LockReason::Standing {
                        party: guild.clone(),
                        at_least: h(10_00),
                    }
                ),
                (
                    Lock::Stage(quest_id("wait"), StageId::new("s").expect("valid id")),
                    LockReason::Standing {
                        party: guild,
                        at_least: h(20_00),
                    }
                ),
            ]
        );
    }

    #[test]
    fn a_declared_lock_is_no_problem_and_not_stale() {
        let lower = inline(effects((0, 0), standing(&[("guild", -5_00)], &[]), vec![]));
        let mut quests = quests(lower, needs_standing("guild", 10_00));
        let act = quests.quests.get_mut(&quest_id("act")).expect("there");
        act.stages[0].choices[0].locks = vec![Lock::Gate(quest_id("wait"))];
        assert_eq!(quests.lockout_problems(&content()), []);
        assert_eq!(quests.stale_locks(&content()), []);
    }

    /// Whether `act`'s choice is checked against `wait` once `change` is made.
    fn checked_against_wait(change: impl FnOnce(&mut Quests)) -> bool {
        let lower = inline(effects((0, 0), standing(&[("guild", -5_00)], &[]), vec![]));
        let mut quests = quests(lower, needs_standing("guild", 10_00));
        change(&mut quests);
        !quests.lockout_problems(&content()).is_empty()
    }

    fn act(quests: &mut Quests) -> &mut Quest {
        quests.quests.get_mut(&quest_id("act")).expect("there")
    }

    fn done(progress: &str) -> Requirements {
        Requirements {
            done: vec![Progress::parse(progress).expect("valid")],
            ..Requirements::default()
        }
    }

    #[test]
    fn a_quest_finished_before_the_choices_own_starts_is_not_checked() {
        assert!(checked_against_wait(|_| {}));
        assert!(!checked_against_wait(
            |quests| act(quests).requires = done("wait")
        ));
        assert!(
            checked_against_wait(|quests| act(quests).requires = done("wait.s")),
            "a stage reached isn't the quest finished"
        );
        assert!(!checked_against_wait(
            |quests| act(quests).stages[0].requires = done("wait")
        ));
    }

    #[test]
    fn finished_before_carries_through_what_each_finished_quest_needed() {
        assert!(!checked_against_wait(|quests| {
            let mid = one_stage("mid", Requirements::default(), ChoiceEffects::None);
            let mut mid = mid;
            mid.requires = done("wait");
            quests.quests.insert(mid.id.clone(), mid);
            act(quests).requires = done("mid");
        }));
    }

    fn line(steps: Vec<(Vec<&str>, Option<usize>, Requirements)>) -> Questline {
        Questline {
            id: QuestlineId::new("line").expect("valid id"),
            name: "Line".to_owned(),
            giver: None,
            steps: steps
                .into_iter()
                .map(|(quests, need, requires)| Step {
                    quests: quests.into_iter().map(quest_id).collect(),
                    need,
                    requires,
                    leftovers: Leftovers::Open,
                })
                .collect(),
        }
    }

    #[test]
    fn an_earlier_step_needing_all_its_quests_is_finished_first() {
        let with_line = |steps| {
            move |quests: &mut Quests| {
                let line = line(steps);
                quests.questlines.insert(line.id.clone(), line);
            }
        };
        assert!(!checked_against_wait(with_line(vec![
            (vec!["wait"], None, Requirements::default()),
            (vec!["act"], None, Requirements::default()),
        ])));
        assert!(checked_against_wait(with_line(vec![
            (vec!["wait"], Some(0), Requirements::default()),
            (vec!["act"], None, Requirements::default()),
        ])));
        assert!(
            checked_against_wait(with_line(vec![
                (vec!["act"], None, Requirements::default()),
                (vec!["wait"], None, Requirements::default()),
            ])),
            "a later step comes after"
        );
        assert!(!checked_against_wait(with_line(vec![(
            vec!["act"],
            None,
            done("wait")
        )])));
    }

    #[test]
    fn a_span_reaches_from_its_start_by_every_other_choice_and_any_outcome() {
        let at = |quest: &str| ChoiceAt {
            quest: quest_id(quest),
            stage: 0,
            choice: 0,
        };
        let mut span = Span {
            start: h(10_00),
            choices: vec![
                (at("a"), h(-30_00)),
                (at("b"), h(50_00)),
                (at("c"), h(-90_00)),
            ],
            ..Span::default()
        };
        assert_eq!(span.range(None), (h(-100_00), h(60_00)));
        assert_eq!(span.range(Some(&at("c"))), (h(-20_00), h(60_00)));
        assert_eq!(span.range(Some(&at("b"))), (h(-100_00), h(10_00)));
        span.choices.push((at("d"), h(80_00)));
        assert_eq!(span.range(None), (h(-100_00), h(100_00)));
        span.outcome_lowers = true;
        assert_eq!(span.range(Some(&at("c"))).0, h(-100_00));
        span.choices.clear();
        span.outcome_lowers = false;
        span.outcome_raises = true;
        assert_eq!(span.range(None), (h(10_00), h(100_00)));
    }

    #[test]
    fn lockouts_say_what_can_happen_in_a_designers_words() {
        let at = ChoiceAt {
            quest: quest_id("act"),
            stage: 0,
            choice: 0,
        };
        let gate = Lock::Gate(quest_id("wait"));
        let stage = Lock::Stage(quest_id("wait"), StageId::new("s").expect("valid id"));
        let sergeant = Needed::Rank(RankId::new("sergeant").expect("valid id"));
        let message = |target: &Lock, reason: LockReason| {
            QuestProblem::Lockout {
                at: at.clone(),
                target: target.clone(),
                reason,
            }
            .to_string()
        };
        assert_eq!(
            message(
                &gate,
                LockReason::Standing {
                    party: Party::Faction(guild()),
                    at_least: h(10_00),
                }
            ),
            "may lock out wait: it can lower standing with guild, which no action raises, below the 10.00 its gate needs; declare it in locks"
        );
        assert_eq!(
            message(
                &stage,
                LockReason::War {
                    faction: watch(),
                    other: guild(),
                    needed: Needed::Membership,
                }
            ),
            "may lock out wait.s: it can start a war between watch and guild, ending the membership of watch that the stage needs; declare it in locks"
        );
        assert_eq!(
            message(
                &gate,
                LockReason::Drift {
                    faction: watch(),
                    toward: Toward::Good,
                    consequence: Consequence::Expel,
                    needed: sergeant.clone(),
                }
            ),
            "may lock out wait: it moves alignment toward good, and watch expels members who drift out of its tolerance, which can end the sergeant rank in watch that its gate needs; declare it in locks"
        );
        assert_eq!(
            message(
                &stage,
                LockReason::Drift {
                    faction: watch(),
                    toward: Toward::Evil,
                    consequence: Consequence::Demote,
                    needed: Needed::Membership,
                }
            ),
            "may lock out wait.s: it moves alignment toward evil, and watch demotes members who drift out of its tolerance, which can end the membership of watch that the stage needs; declare it in locks"
        );
        assert_eq!(
            message(
                &gate,
                LockReason::Probation {
                    faction: watch(),
                    toward: Toward::Chaotic,
                    needed: sergeant,
                }
            ),
            "may lock out wait: it moves alignment toward chaotic, which no action moves back toward lawful, so watch's probation can run out, ending the sergeant rank in watch that its gate needs; declare it in locks"
        );
        assert_eq!(
            message(
                &stage,
                LockReason::Tolerance {
                    faction: guild(),
                    toward: Toward::Lawful,
                }
            ),
            "may lock out wait.s: it moves alignment toward lawful, which no action moves back toward chaotic, so it can take the character outside guild's member tolerance, which the stage needs; declare it in locks"
        );
        let stale = |lock: &Lock| {
            QuestWarning::StaleLock {
                at: at.clone(),
                index: 0,
                lock: lock.clone(),
            }
            .to_string()
        };
        assert_eq!(
            stale(&gate),
            "it can't lock out wait: nothing it does can make that quest's gate false for good; remove it"
        );
        assert_eq!(
            stale(&stage),
            "it can't lock out wait.s: nothing it does can make that stage's requirements false for good; remove it"
        );
    }
}
