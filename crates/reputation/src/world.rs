use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use factional_core::{Curve, CurveError, Fixed, Ratio, Tick, article, suggest};

use crate::defection::{self, Situation};
use crate::distance::gap;
use crate::shift::{Target, shifts};
use crate::{
    AXIS_LIMIT, Action, ActionId, Alignment, AlignmentDelta, AppliedModifier, Axis, Bands, Change,
    Character, CharacterId, Command, CommandError, Component, ComponentKind, ConflictRule,
    Consequence, Defection, Disposition, DispositionWeights, DriftPolicy, Effects, Event, Faction,
    FactionId, Inertia, InertiaProfile, JoinAssessment, JoinBlock, JournalEntry, LeaveReason,
    Membership, Metric, ModifierId, ModifierObserver, Outcome, OutcomeId, Part, Party, ProfileId,
    PromotionAssessment, RankCheck, RankId, Regard, Relation, RelationSide, RestoreError, Role,
    Rule, SavedCommand, Shift, Spill, StandingEffects, StandingKey, StandingOwner, TableKind,
    TableOwner, TableProblem, TableSource, TargetCurve, TargetRelation, Toward, Verdict, Weights,
    Witnesses, measure,
};

/// World-wide rules and defaults from `balance.toml` (DESIGN.md §12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Balance {
    /// An axis at or beyond ±this reads as Lawful/Chaotic or Good/Evil (DESIGN.md §5.1).
    pub label_threshold: Fixed,
    /// The weights of any faction or character without their own (DESIGN.md §6).
    pub default_weights: Weights,
    /// How weighted gaps combine into a distance, for the whole world (DESIGN.md §6).
    pub metric: Metric,
    /// How alignment distance turns into liking: `disposition.affinity` (DESIGN.md §8.1).
    pub affinity: Curve,
    /// The named bands a disposition score falls into (DESIGN.md §8.1).
    pub bands: Bands,
    /// How much each disposition component counts (DESIGN.md §8.1).
    pub disposition_weights: DispositionWeights,
    /// What sharing a faction counts for, in kinship.
    pub same_faction: Fixed,
    /// The named bands a relation between factions falls into (DESIGN.md §9.4).
    pub relation_bands: Bands,
    /// Two factions are in conflict when either regards the other at or below this.
    pub conflict_threshold: Fixed,
    /// `membership.defectors` and `membership.deserters`; a table left out is built in
    /// (DESIGN.md §9.2).
    pub rule_tables: BTreeMap<TableKind, Vec<Rule>>,
    /// The inertia profiles, and the default (DESIGN.md §5.3).
    pub inertia: Inertia,
    /// `disposition.hysteresis`: how far past a band's edge a watched score must go to
    /// leave the band (DESIGN.md §8.3).
    pub hysteresis: Fixed,
    /// `standing.spillover`: the share of a standing change with one faction that another
    /// gets, by how it regards the first (DESIGN.md §7.1, P-13).
    pub spillover: Curve,
    /// `membership.default_drift`: the drift policy of a faction that sets none (DESIGN.md
    /// §9.3).
    pub default_drift: DriftPolicy,
    /// `membership.conflict`: how a war between two of someone's factions is settled
    /// (DESIGN.md §9.4).
    pub conflict: ConflictRule,
}

impl Balance {
    /// `disposition.same_faction`'s default: 50.00.
    pub const DEFAULT_SAME_FACTION: Fixed = Fixed::from_hundredths(50_00);

    /// `relations.conflict_threshold`'s default: −50.00.
    pub const DEFAULT_CONFLICT_THRESHOLD: Fixed = Fixed::from_hundredths(-50_00);

    /// `alignment.label_threshold`'s default: 33.00.
    pub const DEFAULT_LABEL_THRESHOLD: Fixed = Fixed::from_hundredths(33_00);

    /// `standing.spillover`'s default: bitter enemies feel a little of the opposite, allies
    /// share up to half, and anything between −50 and 50 shares nothing.
    pub fn default_spillover() -> Curve {
        let point = |x: i64, y: i64| (Fixed::from_hundredths(x), Fixed::from_hundredths(y));
        Curve::from_points(vec![
            point(-100_00, -30),
            point(-50_00, 0),
            point(50_00, 0),
            point(100_00, 50),
        ])
        .expect("the default spillover is a valid curve")
    }

    /// `disposition.affinity`'s default: 50 when identical, 0 at 60 apart, −50 at 200.
    pub fn default_affinity() -> Curve {
        let point = |x: i64, y: i64| (Fixed::from_hundredths(x), Fixed::from_hundredths(y));
        Curve::from_points(vec![
            point(0, 50_00),
            point(60_00, 0),
            point(200_00, -50_00),
        ])
        .expect("the default affinity is a valid curve")
    }
}

impl Default for Balance {
    fn default() -> Balance {
        Balance {
            label_threshold: Balance::DEFAULT_LABEL_THRESHOLD,
            default_weights: Weights::EVEN,
            metric: Metric::default(),
            affinity: Balance::default_affinity(),
            bands: Bands::standard(),
            disposition_weights: DispositionWeights::default(),
            same_faction: Balance::DEFAULT_SAME_FACTION,
            relation_bands: Bands::relations(),
            conflict_threshold: Balance::DEFAULT_CONFLICT_THRESHOLD,
            rule_tables: BTreeMap::new(),
            inertia: Inertia::default(),
            hysteresis: Fixed::ZERO,
            spillover: Balance::default_spillover(),
            default_drift: DriftPolicy::Flag,
            conflict: ConflictRule::default(),
        }
    }
}

/// Validated content: everything a world starts from. `factional-content` builds it from a
/// directory of TOML files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Content {
    pub balance: Balance,
    pub characters: BTreeMap<CharacterId, Character>,
    pub factions: BTreeMap<FactionId, Faction>,
    /// The action catalogue.
    pub actions: BTreeMap<ActionId, Action>,
    /// How factions regard each other, as written; any direction left out is 0.
    pub relations: Vec<Relation>,
    /// Named bundles of effects, such as quest results (P-26).
    pub outcomes: BTreeMap<OutcomeId, Outcome>,
}

/// Something wrong with content as a whole, found before a world is built from it (P-32).
/// Problems within one value, such as an axis out of range, can't be built at all, so they
/// never get this far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentProblem {
    /// A faction and a character share an id, so an id alone couldn't say which is meant.
    SharedId(FactionId),
    /// `disposition.affinity` gives a value outside −100…100, the range of a disposition.
    AffinityOutOfRange(CurveError),
    /// A starting membership names a faction that doesn't exist. `index` is its place in the
    /// character's list, from 0.
    UnknownMembershipFaction {
        character: CharacterId,
        index: usize,
        faction: FactionId,
        suggestion: Option<FactionId>,
    },
    /// A character lists the same faction twice; `index` is the second.
    DuplicateMembership {
        character: CharacterId,
        index: usize,
        faction: FactionId,
    },
    /// A relation names a faction that doesn't exist. `index` is the relation's place in the
    /// list, from 0.
    UnknownRelationFaction {
        index: usize,
        side: RelationSide,
        faction: FactionId,
        suggestion: Option<FactionId>,
    },
    /// A relation between a faction and itself.
    SelfRelation { index: usize },
    /// A relation's value is outside −100…100.
    RelationOutOfRange { index: usize, value: Fixed },
    /// A relation sets a direction an earlier one, `first`, already set.
    DuplicateRelation {
        index: usize,
        from: FactionId,
        to: FactionId,
        first: usize,
    },
    /// `relations.conflict_threshold` is outside −100…100.
    ConflictThresholdOutOfRange(Fixed),
    /// A character starts in two factions in conflict (invariant 6): `faction`, at `index`,
    /// and `other`, earlier in their list.
    StartsInConflict {
        character: CharacterId,
        index: usize,
        faction: FactionId,
        other: FactionId,
        relation: Fixed,
    },
    /// A `standing` block names a faction or character that doesn't exist.
    UnknownStandingParty {
        owner: StandingOwner,
        party: Party,
        suggestion: Option<Party>,
    },
    /// A standing value or change outside −100…100.
    StandingOutOfRange {
        owner: StandingOwner,
        key: StandingKey,
        value: Fixed,
    },
    /// A faction's `leave_standing_change` outside −100…100.
    LeaveStandingOutOfRange { faction: FactionId, value: Fixed },
    /// A faction's `expel_standing_change` outside −100…100.
    ExpelStandingOutOfRange { faction: FactionId, value: Fixed },
    /// A `disposition.weights` entry below 0.
    NegativeDispositionWeight {
        component: ComponentKind,
        value: Fixed,
    },
    /// `disposition.same_faction` outside −100…100.
    SameFactionOutOfRange(Fixed),
    /// `disposition.hysteresis` below 0.
    NegativeHysteresis(Fixed),
    /// `standing.spillover` gives a multiplier outside −1…1.
    SpilloverOutOfRange(CurveError),
    /// A faction with no ranks: every faction needs a rung for new members.
    NoRanks(FactionId),
    /// Two ranks in one faction share an id; `index` is the second.
    DuplicateRank {
        faction: FactionId,
        index: usize,
        rank: RankId,
    },
    /// A rank's requirement out of range: standing within ±100, tolerance at least 0.
    RankValueOutOfRange {
        faction: FactionId,
        index: usize,
        key: RankKey,
        value: Fixed,
    },
    /// A starting membership names a rank that isn't on its faction's ladder.
    UnknownRank {
        character: CharacterId,
        index: usize,
        faction: FactionId,
        rank: RankId,
        suggestion: Option<RankId>,
    },
    /// `inertia.default_profile`, or a character's `inertia`, names a profile that doesn't
    /// exist.
    UnknownProfile {
        user: ProfileUser,
        profile: ProfileId,
        suggestion: Option<ProfileId>,
    },
    /// An action's `by_target` curve goes below 0, which would reverse its effect (P-28).
    NegativeTargetScaling {
        action: ActionId,
        curve: TargetCurve,
        value: Fixed,
    },
    /// An inertia curve goes below 0, which would reverse a shift (P-5).
    NegativeInertia {
        profile: ProfileId,
        toward: Toward,
        value: Fixed,
    },
    /// Something wrong with a `defectors` or `deserters` table.
    RuleTable {
        owner: TableOwner,
        kind: TableKind,
        problem: TableProblem,
    },
}

/// What names an inertia profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileUser {
    /// `inertia.default_profile` in `balance.toml`.
    Default,
    /// A character's `inertia`.
    Character(CharacterId),
}

/// Which value of a rank a content problem is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RankKey {
    /// `requires.standing`.
    Standing,
    Tolerance,
}

/// Something in content that's allowed but probably not meant: it's reported, and the world
/// is built anyway (P-32).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentWarning {
    /// A starting member further from their faction than its member tolerance.
    OutsideMemberTolerance {
        character: CharacterId,
        index: usize,
        faction_name: String,
        distance: Fixed,
        member_tolerance: Fixed,
    },
    /// A starting member whose standing is below what their rank requires.
    BelowRankStanding {
        character: CharacterId,
        index: usize,
        rank: RankId,
        required: Fixed,
        standing: Fixed,
    },
    /// A rank whose tolerance is looser than its faction's member tolerance, so it adds
    /// nothing: members are held to the faction's tolerance anyway.
    RankToleranceLooser {
        faction: FactionId,
        index: usize,
        rank: RankId,
        tolerance: Fixed,
        member_tolerance: Fixed,
    },
    /// A faction no starting character is within joining tolerance of, so no one could join
    /// it at the start. `nearest` is the closest character and their distance; `None` if
    /// there are no characters.
    NoOneWithinTolerance {
        faction: FactionId,
        faction_name: String,
        tolerance: Fixed,
        nearest: Option<(CharacterId, Fixed)>,
    },
}

impl Content {
    /// Every problem with this content, in a fixed order; empty if a world can be built
    /// from it.
    pub fn problems(&self) -> Vec<ContentProblem> {
        let affinity = self
            .balance
            .affinity
            .check_y_within(-AXIS_LIMIT, AXIS_LIMIT)
            .err()
            .map(ContentProblem::AffinityOutOfRange);
        let spillover = self
            .balance
            .spillover
            .check_y_within(-Fixed::ONE, Fixed::ONE)
            .err()
            .map(ContentProblem::SpilloverOutOfRange);
        let shared_ids = self
            .factions
            .keys()
            .filter(|faction| {
                CharacterId::new(faction.as_str()).is_ok_and(|id| self.characters.contains_key(&id))
            })
            .map(|faction| ContentProblem::SharedId(faction.clone()));
        let threshold = (!(-AXIS_LIMIT..=AXIS_LIMIT).contains(&self.balance.conflict_threshold))
            .then_some(ContentProblem::ConflictThresholdOutOfRange(
                self.balance.conflict_threshold,
            ));
        let relations = self
            .relations
            .iter()
            .enumerate()
            .flat_map(|(index, relation)| {
                let mut problems: Vec<ContentProblem> = relation
                    .named()
                    .into_iter()
                    .filter(|(_, faction)| !self.factions.contains_key(*faction))
                    .map(|(side, faction)| ContentProblem::UnknownRelationFaction {
                        index,
                        side,
                        faction: faction.clone(),
                        suggestion: closest(faction.as_str(), self.factions.keys()),
                    })
                    .collect();
                let directions = relation.directions();
                if directions[0].0 == directions[0].1 {
                    problems.push(ContentProblem::SelfRelation { index });
                }
                if !(-AXIS_LIMIT..=AXIS_LIMIT).contains(&relation.value) {
                    problems.push(ContentProblem::RelationOutOfRange {
                        index,
                        value: relation.value,
                    });
                }
                for (from, to) in directions {
                    let earlier = self.relations[..index].iter().position(|earlier| {
                        earlier.directions().contains(&(from.clone(), to.clone()))
                    });
                    if let Some(first) = earlier {
                        problems.push(ContentProblem::DuplicateRelation {
                            index,
                            from,
                            to,
                            first,
                        });
                    }
                }
                problems
            });
        let values = &self.relation_values();
        let starts_in_conflict = self.characters.values().flat_map(|character| {
            let listed: Vec<&FactionId> = character.factions().collect();
            listed
                .iter()
                .enumerate()
                .filter_map(|(index, &faction)| {
                    listed[..index].iter().find_map(|&other| {
                        let relation = hostility(values, other, faction);
                        (relation <= self.balance.conflict_threshold && other != faction).then(
                            || ContentProblem::StartsInConflict {
                                character: character.id.clone(),
                                index,
                                faction: faction.clone(),
                                other: other.clone(),
                                relation,
                            },
                        )
                    })
                })
                .collect::<Vec<_>>()
        });
        let memberships = self.characters.values().flat_map(|character| {
            character
                .memberships
                .iter()
                .enumerate()
                .filter_map(|(index, membership)| {
                    let faction = &membership.faction;
                    let Some(found) = self.factions.get(faction) else {
                        return Some(ContentProblem::UnknownMembershipFaction {
                            character: character.id.clone(),
                            index,
                            faction: faction.clone(),
                            suggestion: closest(faction.as_str(), self.factions.keys()),
                        });
                    };
                    let earlier = &character.memberships[..index];
                    if earlier.iter().any(|other| &other.faction == faction) {
                        return Some(ContentProblem::DuplicateMembership {
                            character: character.id.clone(),
                            index,
                            faction: faction.clone(),
                        });
                    }
                    let rank = membership.rank.as_ref()?;
                    let ladder = found.ranks.iter().map(|rung| &rung.id);
                    (found.rank_position(rank).is_none()).then(|| ContentProblem::UnknownRank {
                        character: character.id.clone(),
                        index,
                        faction: faction.clone(),
                        rank: rank.clone(),
                        suggestion: closest(rank.as_str(), ladder),
                    })
                })
        });
        affinity
            .into_iter()
            .chain(spillover)
            .chain(threshold)
            .chain(self.inertia_problems())
            .chain(self.target_scaling_problems())
            .chain(shared_ids)
            .chain(memberships)
            .chain(self.rank_problems())
            .chain(self.rule_table_problems())
            .chain(relations)
            .chain(starts_in_conflict)
            .chain(self.standing_problems())
            .chain(self.disposition_problems())
            .collect()
    }

    /// Every ladder needs a rung, unique rank ids, standing requirements within ±100 and
    /// tolerances of at least 0.
    fn rank_problems(&self) -> Vec<ContentProblem> {
        let mut problems = Vec::new();
        for faction in self.factions.values() {
            if faction.ranks.is_empty() {
                problems.push(ContentProblem::NoRanks(faction.id.clone()));
            }
            for (index, rank) in faction.ranks.iter().enumerate() {
                let out_of_range = |key, value: Fixed| ContentProblem::RankValueOutOfRange {
                    faction: faction.id.clone(),
                    index,
                    key,
                    value,
                };
                if let Some(standing) = rank.requires_standing.filter(|v| !within_range(*v)) {
                    problems.push(out_of_range(RankKey::Standing, standing));
                }
                if let Some(tolerance) = rank.tolerance.filter(|v| *v < Fixed::ZERO) {
                    problems.push(out_of_range(RankKey::Tolerance, tolerance));
                }
                if faction.ranks[..index]
                    .iter()
                    .any(|earlier| earlier.id == rank.id)
                {
                    problems.push(ContentProblem::DuplicateRank {
                        faction: faction.id.clone(),
                        index,
                        rank: rank.id.clone(),
                    });
                }
            }
        }
        problems
    }

    /// The default profile must exist, every curve stay at or above 0, and every character's
    /// profile exist: in that order, profiles and characters in id order.
    fn inertia_problems(&self) -> Vec<ContentProblem> {
        let inertia = &self.balance.inertia;
        let unknown = |user: ProfileUser, profile: &ProfileId| {
            (!inertia.profiles.contains_key(profile)).then(|| ContentProblem::UnknownProfile {
                user,
                profile: profile.clone(),
                suggestion: closest(profile.as_str(), inertia.profiles.keys()),
            })
        };
        let default = unknown(ProfileUser::Default, &inertia.default_profile);
        let negative = inertia.profiles.iter().flat_map(|(id, profile)| {
            profile.curves.iter().filter_map(|(toward, curve)| {
                let lowest = curve.lowest();
                (lowest < Fixed::ZERO).then(|| ContentProblem::NegativeInertia {
                    profile: id.clone(),
                    toward: *toward,
                    value: lowest,
                })
            })
        });
        let characters = self.characters.values().filter_map(|character| {
            let profile = character.inertia.as_ref()?;
            unknown(ProfileUser::Character(character.id.clone()), profile)
        });
        default
            .into_iter()
            .chain(negative)
            .chain(characters)
            .collect()
    }

    /// Every action's `by_target` curves must stay at or above 0: actions in id order, then
    /// curves in `TargetCurve` order.
    fn target_scaling_problems(&self) -> Vec<ContentProblem> {
        self.actions
            .values()
            .flat_map(|action| {
                action.by_target.iter().filter_map(|(curve, shape)| {
                    let lowest = shape.lowest();
                    (lowest < Fixed::ZERO).then(|| ContentProblem::NegativeTargetScaling {
                        action: action.id.clone(),
                        curve: *curve,
                        value: lowest,
                    })
                })
            })
            .collect()
    }

    /// Problems with the world's rule tables, then each faction's own, in id order.
    fn rule_table_problems(&self) -> Vec<ContentProblem> {
        let world = self
            .balance
            .rule_tables
            .iter()
            .map(|(kind, rules)| (TableOwner::World, *kind, rules, None));
        let own = self.factions.values().flat_map(|faction| {
            faction.rule_tables.iter().map(move |(kind, rules)| {
                (
                    TableOwner::Faction(faction.id.clone()),
                    *kind,
                    rules,
                    Some(faction),
                )
            })
        });
        world
            .chain(own)
            .flat_map(|(owner, kind, rules, faction)| {
                defection::problems(rules, faction)
                    .into_iter()
                    .map(move |problem| ContentProblem::RuleTable {
                        owner: owner.clone(),
                        kind,
                        problem,
                    })
            })
            .collect()
    }

    /// `disposition.weights` must each be at least 0, `same_faction` within ±100, and
    /// `hysteresis` at least 0.
    fn disposition_problems(&self) -> Vec<ContentProblem> {
        let weights = self.balance.disposition_weights;
        let mut problems: Vec<ContentProblem> = ComponentKind::ALL
            .into_iter()
            .filter(|kind| weights.get(*kind) < Fixed::ZERO)
            .map(|component| ContentProblem::NegativeDispositionWeight {
                component,
                value: weights.get(component),
            })
            .collect();
        if !within_range(self.balance.same_faction) {
            problems.push(ContentProblem::SameFactionOutOfRange(
                self.balance.same_faction,
            ));
        }
        if self.balance.hysteresis < Fixed::ZERO {
            problems.push(ContentProblem::NegativeHysteresis(self.balance.hysteresis));
        }
        problems
    }

    /// Problems with `leave_standing_change` and every `standing` block: each party must
    /// exist, and each value must be within ±100. Factions, then characters, actions and
    /// outcomes, each in id order.
    fn standing_problems(&self) -> Vec<ContentProblem> {
        let mut problems: Vec<ContentProblem> = self
            .factions
            .values()
            .flat_map(|faction| {
                let leave = (!within_range(faction.leave_standing_change)).then(|| {
                    ContentProblem::LeaveStandingOutOfRange {
                        faction: faction.id.clone(),
                        value: faction.leave_standing_change,
                    }
                });
                let expel = (!within_range(faction.expel_standing_change)).then(|| {
                    ContentProblem::ExpelStandingOutOfRange {
                        faction: faction.id.clone(),
                        value: faction.expel_standing_change,
                    }
                });
                leave.into_iter().chain(expel)
            })
            .collect();
        for character in self.characters.values() {
            let owner = StandingOwner::Character(character.id.clone());
            self.check_standing(&owner, &character.standing, &mut problems);
        }
        for action in self.actions.values() {
            let owner = StandingOwner::Action(action.id.clone());
            let standing = &action.standing;
            for (key, value) in [
                (StandingKey::Target, standing.target),
                (StandingKey::TargetFactions, standing.target_factions),
            ] {
                if let Some(value) = value.filter(|value| !within_range(*value)) {
                    problems.push(ContentProblem::StandingOutOfRange {
                        owner: owner.clone(),
                        key,
                        value,
                    });
                }
            }
            self.check_standing(&owner, &standing.named, &mut problems);
        }
        for outcome in self.outcomes.values() {
            let owner = StandingOwner::Outcome(outcome.id.clone());
            self.check_standing(&owner, &outcome.effects.standing, &mut problems);
        }
        problems
    }

    /// Each named party must exist (with a suggestion if not), and its value be within ±100.
    fn check_standing(
        &self,
        owner: &StandingOwner,
        effects: &StandingEffects,
        problems: &mut Vec<ContentProblem>,
    ) {
        for (party, value) in effects.parties() {
            let unknown = match &party {
                Party::Faction(id) => (!self.factions.contains_key(id))
                    .then(|| closest(id.as_str(), self.factions.keys()).map(Party::Faction)),
                Party::Character(id) => (!self.characters.contains_key(id))
                    .then(|| closest(id.as_str(), self.characters.keys()).map(Party::Character)),
            };
            if let Some(suggestion) = unknown {
                problems.push(ContentProblem::UnknownStandingParty {
                    owner: owner.clone(),
                    party,
                    suggestion,
                });
            } else if !within_range(value) {
                problems.push(ContentProblem::StandingOutOfRange {
                    owner: owner.clone(),
                    key: StandingKey::Party(party),
                    value,
                });
            }
        }
    }

    /// Every direction the relations set, first setting first; any other is 0.
    fn relation_values(&self) -> BTreeMap<(FactionId, FactionId), Fixed> {
        let mut values = BTreeMap::new();
        for relation in &self.relations {
            for direction in relation.directions() {
                values.entry(direction).or_insert(relation.value);
            }
        }
        values
    }

    /// Everything probably not meant, for content with no problems: each character's
    /// memberships, then each faction: whether anyone starts within its tolerance, then its
    /// ranks.
    pub fn warnings(&self) -> Vec<ContentWarning> {
        let balance = &self.balance;
        let mut warnings = Vec::new();
        for character in self.characters.values() {
            for (index, membership) in character.memberships.iter().enumerate() {
                let Some(faction) = self.factions.get(&membership.faction) else {
                    continue;
                };
                let weights = faction.weights.unwrap_or(balance.default_weights);
                let distance = measure(
                    faction.alignment,
                    character.alignment,
                    weights,
                    balance.metric,
                );
                let member_tolerance = faction.tolerances.member();
                if distance > member_tolerance {
                    warnings.push(ContentWarning::OutsideMemberTolerance {
                        character: character.id.clone(),
                        index,
                        faction_name: faction.name.clone(),
                        distance,
                        member_tolerance,
                    });
                }
                let rank = membership
                    .rank
                    .as_ref()
                    .and_then(|rank| faction.ranks.iter().find(|rung| &rung.id == rank));
                let standing = character
                    .standing
                    .factions
                    .get(&faction.id)
                    .copied()
                    .unwrap_or_default();
                if let Some((rank, required)) = rank
                    .and_then(|rank| rank.requires_standing.map(|required| (rank, required)))
                    .filter(|(_, required)| standing < *required)
                {
                    warnings.push(ContentWarning::BelowRankStanding {
                        character: character.id.clone(),
                        index,
                        rank: rank.id.clone(),
                        required,
                        standing,
                    });
                }
            }
        }
        for faction in self.factions.values() {
            let weights = faction.weights.unwrap_or(balance.default_weights);
            let tolerance = faction.tolerances.tolerance();
            let mut nearest: Option<(CharacterId, Fixed)> = None;
            for character in self.characters.values() {
                let distance = measure(
                    faction.alignment,
                    character.alignment,
                    weights,
                    balance.metric,
                );
                if nearest
                    .as_ref()
                    .is_none_or(|(_, closest)| distance < *closest)
                {
                    nearest = Some((character.id.clone(), distance));
                }
            }
            if nearest
                .as_ref()
                .is_none_or(|(_, distance)| *distance > tolerance)
            {
                warnings.push(ContentWarning::NoOneWithinTolerance {
                    faction: faction.id.clone(),
                    faction_name: faction.name.clone(),
                    tolerance,
                    nearest,
                });
            }
            let member_tolerance = faction.tolerances.member();
            for (index, rank) in faction.ranks.iter().enumerate() {
                if let Some(tolerance) = rank.tolerance.filter(|t| *t > member_tolerance) {
                    warnings.push(ContentWarning::RankToleranceLooser {
                        faction: faction.id.clone(),
                        index,
                        rank: rank.id.clone(),
                        tolerance,
                        member_tolerance,
                    });
                }
            }
        }
        warnings
    }
}

impl fmt::Display for ContentProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContentProblem::SharedId(id) => write!(
                f,
                "'{id}' is also a character's id: factions and characters need different ids"
            ),
            ContentProblem::AffinityOutOfRange(error)
            | ContentProblem::SpilloverOutOfRange(error) => error.fmt(f),
            ContentProblem::UnknownMembershipFaction {
                faction,
                suggestion,
                ..
            } => {
                write!(f, "unknown faction '{faction}'")?;
                match suggestion {
                    Some(close) => write!(f, " (did you mean '{close}'?)"),
                    None => Ok(()),
                }
            }
            ContentProblem::DuplicateMembership {
                character, faction, ..
            } => write!(f, "{character} already belongs to {faction}"),
            ContentProblem::UnknownRelationFaction {
                faction,
                suggestion,
                ..
            } => {
                write!(f, "unknown faction '{faction}'")?;
                match suggestion {
                    Some(close) => write!(f, " (did you mean '{close}'?)"),
                    None => Ok(()),
                }
            }
            ContentProblem::SelfRelation { .. } => {
                f.write_str("a faction can't have a relation with itself")
            }
            ContentProblem::RelationOutOfRange { value, .. }
            | ContentProblem::ConflictThresholdOutOfRange(value) => {
                write!(f, "{value} is outside {}..{}", -AXIS_LIMIT, AXIS_LIMIT)
            }
            ContentProblem::DuplicateRelation {
                from, to, first, ..
            } => write!(f, "{from} → {to} is already set by relation[{first}]"),
            ContentProblem::StartsInConflict {
                character,
                faction,
                other,
                relation,
                ..
            } => write!(
                f,
                "{character} can't start in both {other} and {faction}: they're in conflict ({relation})"
            ),
            ContentProblem::UnknownStandingParty {
                party, suggestion, ..
            } => {
                let kind = match party {
                    Party::Faction(_) => "faction",
                    Party::Character(_) => "character",
                };
                write!(f, "unknown {kind} '{party}'")?;
                match suggestion {
                    Some(close) => write!(f, " (did you mean '{close}'?)"),
                    None => Ok(()),
                }
            }
            ContentProblem::NoRanks(_) => f.write_str("a faction needs at least one rank"),
            ContentProblem::DuplicateRank { rank, .. } => {
                write!(f, "another rank is already called '{rank}'")
            }
            ContentProblem::RankValueOutOfRange {
                key: RankKey::Standing,
                value,
                ..
            } => write!(f, "{value} is outside {}..{}", -AXIS_LIMIT, AXIS_LIMIT),
            ContentProblem::RankValueOutOfRange {
                key: RankKey::Tolerance,
                value,
                ..
            } => write!(f, "{value} must be at least {}", Fixed::ZERO),
            ContentProblem::UnknownRank {
                faction,
                rank,
                suggestion,
                ..
            } => {
                write!(f, "unknown rank '{rank}' for {faction}")?;
                match suggestion {
                    Some(close) => write!(f, " (did you mean '{close}'?)"),
                    None => Ok(()),
                }
            }
            ContentProblem::UnknownProfile {
                profile,
                suggestion,
                ..
            } => {
                write!(f, "unknown inertia profile '{profile}'")?;
                match suggestion {
                    Some(close) => write!(f, " (did you mean '{close}'?)"),
                    None => Ok(()),
                }
            }
            ContentProblem::NegativeTargetScaling { value, .. } => write!(
                f,
                "{value} is below {}: a target can soften or sharpen an act, never reverse it",
                Fixed::ZERO
            ),
            ContentProblem::NegativeInertia { value, .. } => write!(
                f,
                "{value} is below {}: inertia can damp or amplify a shift, never reverse it",
                Fixed::ZERO
            ),
            ContentProblem::RuleTable { owner, problem, .. } => match problem {
                TableProblem::RungBelowOne { rung, .. } => {
                    write!(f, "{rung} isn't a rung: rungs count from 1, the lowest")
                }
                TableProblem::RankInWorldTable { rank, .. } => write!(
                    f,
                    "'{rank}' is a rank id, but the world's tables can't name ranks: use a rung number, 1 for the lowest"
                ),
                TableProblem::UnknownRank {
                    rank, suggestion, ..
                } => {
                    write!(f, "unknown rank '{rank}'")?;
                    if let TableOwner::Faction(faction) = owner {
                        write!(f, " for {faction}")?;
                    }
                    match suggestion {
                        Some(close) => write!(f, " (did you mean '{close}'?)"),
                        None => Ok(()),
                    }
                }
                TableProblem::ValueOutOfRange { value, .. } => {
                    write!(f, "{value} is outside {}..{}", -AXIS_LIMIT, AXIS_LIMIT)
                }
                TableProblem::MightNotDecide { rules: 0 } => f.write_str(
                    "a table needs at least one rule, and its last must have no conditions",
                ),
                TableProblem::MightNotDecide { .. } => f.write_str(
                    "the last rule must have no conditions, so the table always decides",
                ),
            },
            ContentProblem::NegativeDispositionWeight { value, .. }
            | ContentProblem::NegativeHysteresis(value) => {
                write!(f, "{value} must be at least {}", Fixed::ZERO)
            }
            ContentProblem::StandingOutOfRange { value, .. }
            | ContentProblem::LeaveStandingOutOfRange { value, .. }
            | ContentProblem::ExpelStandingOutOfRange { value, .. }
            | ContentProblem::SameFactionOutOfRange(value) => {
                write!(f, "{value} is outside {}..{}", -AXIS_LIMIT, AXIS_LIMIT)
            }
        }
    }
}

impl fmt::Display for ContentWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContentWarning::OutsideMemberTolerance {
                character,
                faction_name,
                distance,
                member_tolerance,
                ..
            } => write!(
                f,
                "{character} starts {distance} from {faction_name}, outside its member tolerance of {member_tolerance}"
            ),
            ContentWarning::BelowRankStanding {
                character,
                rank,
                required,
                standing,
                ..
            } => write!(
                f,
                "{character} starts as {} {rank} with standing {standing}, below the {required} it requires",
                article(rank.as_str())
            ),
            ContentWarning::RankToleranceLooser {
                rank,
                tolerance,
                member_tolerance,
                ..
            } => write!(
                f,
                "{rank}'s tolerance {tolerance} is looser than the faction's member tolerance, {member_tolerance}, so it changes nothing"
            ),
            ContentWarning::NoOneWithinTolerance {
                faction_name,
                tolerance,
                nearest,
                ..
            } => {
                write!(
                    f,
                    "no one starts within {faction_name}'s tolerance of {tolerance}: "
                )?;
                match nearest {
                    Some((character, distance)) => {
                        write!(f, "the nearest is {character}, {distance} away")
                    }
                    None => f.write_str("there are no characters"),
                }
            }
        }
    }
}

/// Who is doing the judging: a faction, or a character. Factions come before characters,
/// each in id order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Observer {
    Faction(FactionId),
    Character(CharacterId),
}

impl fmt::Display for Observer {
    /// The observer's id, such as `city_watch` or `captain_hale`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Observer::Faction(id) => id.fmt(f),
            Observer::Character(id) => id.fmt(f),
        }
    }
}

/// Whose weights a measurement used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeightsFrom {
    /// The observer's own.
    Own,
    /// `alignment.default_weights`, because the observer has none of their own.
    Default,
}

/// How far a subject is from an observer, with its working (P-24).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Distance {
    pub value: Fixed,
    pub metric: Metric,
    pub observer: Alignment,
    pub subject: Alignment,
    pub weights: Weights,
    pub weights_from: WeightsFrom,
}

impl Distance {
    /// How far apart observer and subject are on one axis, before weighting.
    pub fn gap(&self, axis: Axis) -> Fixed {
        gap(self.observer.on(axis), self.subject.on(axis))
    }
}

/// The reputation module's world: the content it started from, plus everything that has
/// happened since (DESIGN.md §2).
///
/// State changes only through [`World::execute`]. A command is either refused, changing
/// nothing, or accepted, producing events. Applying events is the only thing that changes
/// state, so replaying the event log rebuilds the state exactly (P-14, P-16).
#[derive(Debug, Clone)]
pub struct World {
    content: Content,
    state: State,
    events: Vec<Event>,
    journal: Vec<JournalEntry>,
    /// For each journal entry, how many events it produced; `None` if it was refused.
    produced: Vec<Option<usize>>,
}

/// Everything that changes during play. Events are the only thing that changes it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct State {
    now: Tick,
    /// Every character's alignment now.
    alignments: BTreeMap<CharacterId, Alignment>,
    /// Every character's factions now, by character; a character in none has no entry.
    memberships: BTreeMap<CharacterId, BTreeMap<FactionId, Membership>>,
    /// Every direction set so far, `(from, to)` → value; any other is 0.
    relations: BTreeMap<(FactionId, FactionId), Fixed>,
    /// Every standing set so far, `(subject, party)` → how `party` regards `subject`; any
    /// other is 0.
    standings: BTreeMap<(CharacterId, Party), Fixed>,
    /// Every watched subject, with the band each observer last put them in (DESIGN.md §8.3).
    watched: BTreeMap<CharacterId, BTreeMap<Observer, String>>,
    /// Members flagged as out of tolerance, until they're back or leave (DESIGN.md §9.3).
    out_of_tolerance: BTreeSet<(CharacterId, FactionId)>,
    /// Members on probation, and when it runs out.
    probation: BTreeMap<(CharacterId, FactionId), Tick>,
    /// Every modifier on how a subject is seen, by subject and id (DESIGN.md §8.1).
    modifiers: BTreeMap<(CharacterId, ModifierId), Modifier>,
    /// Open wars between two of a character's factions, the pair in id order, and when each
    /// opened (DESIGN.md §9.4).
    conflicts: BTreeMap<(CharacterId, FactionId, FactionId), Tick>,
    /// Every faction's alignment now; it starts as content gives it.
    faction_alignments: BTreeMap<FactionId, Alignment>,
}

impl State {
    /// The state before anything has happened.
    fn initial(content: &Content) -> State {
        State {
            now: Tick::default(),
            alignments: content
                .characters
                .values()
                .map(|character| (character.id.clone(), character.alignment))
                .collect(),
            memberships: content
                .characters
                .values()
                .filter(|character| !character.memberships.is_empty())
                .map(|character| {
                    let factions = character.memberships.iter().filter_map(|membership| {
                        let faction = content.factions.get(&membership.faction)?;
                        let rank = membership
                            .rank
                            .clone()
                            .or_else(|| faction.lowest_rank().map(|rank| rank.id.clone()))?;
                        Some((
                            membership.faction.clone(),
                            Membership {
                                since: Tick::default(),
                                rank,
                            },
                        ))
                    });
                    (character.id.clone(), factions.collect())
                })
                .collect(),
            relations: content.relation_values(),
            standings: content
                .characters
                .values()
                .flat_map(|character| {
                    character
                        .standing
                        .parties()
                        .into_iter()
                        .map(|(party, value)| ((character.id.clone(), party), value))
                })
                .collect(),
            watched: BTreeMap::new(),
            out_of_tolerance: BTreeSet::new(),
            probation: BTreeMap::new(),
            conflicts: BTreeMap::new(),
            modifiers: BTreeMap::new(),
            faction_alignments: content
                .factions
                .values()
                .map(|faction| (faction.id.clone(), faction.alignment))
                .collect(),
        }
    }
}

impl World {
    /// A world at tick 0, with nothing yet happened; or, if the content has problems, all of
    /// them, and no world (P-32).
    pub fn new(content: Content) -> Result<World, Vec<ContentProblem>> {
        let problems = content.problems();
        if !problems.is_empty() {
            return Err(problems);
        }
        Ok(World {
            state: State::initial(&content),
            content,
            events: Vec::new(),
            journal: Vec::new(),
            produced: Vec::new(),
        })
    }

    /// Rebuilds a world from its content and its event log, without running any rules: what
    /// saves are built on (P-16).
    pub fn replay(content: Content, events: &[Event]) -> Result<World, Vec<ContentProblem>> {
        let mut world = World::new(content)?;
        for event in events {
            world.apply(event.clone());
        }
        Ok(world)
    }

    /// The journal as a save keeps it: each command, with how many events it produced if it
    /// was accepted (T4, P-54).
    pub fn saved_journal(&self) -> Vec<SavedCommand> {
        self.journal
            .iter()
            .zip(&self.produced)
            .map(|(entry, events)| SavedCommand {
                command: entry.command.clone(),
                events: *events,
            })
            .collect()
    }

    /// Rebuilds a world from a save: its content, its journal and its events. Accepted
    /// commands replay their events, running no rules (P-16); a refused command is decided
    /// again at the same point, to recover its reason, and must still be refused.
    pub fn restore(
        content: Content,
        journal: &[SavedCommand],
        events: &[Event],
    ) -> Result<World, RestoreError> {
        let mut world = World::new(content).map_err(RestoreError::Content)?;
        let accounted = journal
            .iter()
            .filter_map(|saved| saved.events)
            .fold(0_usize, usize::saturating_add);
        if accounted != events.len() {
            return Err(RestoreError::EventCount {
                journal: accounted,
                events: events.len(),
            });
        }
        for (expected, event) in (1..).zip(events) {
            if event.seq != expected {
                return Err(RestoreError::OutOfSequence {
                    expected,
                    found: event.seq,
                });
            }
        }
        let mut next = 0;
        for (index, saved) in journal.iter().enumerate() {
            let result = match saved.events {
                Some(count) => {
                    for event in &events[next..next + count] {
                        world.apply(event.clone());
                    }
                    next += count;
                    Ok(())
                }
                None => match world.decide(&saved.command) {
                    Ok(_) => return Err(RestoreError::NotRefused { index }),
                    Err(refusal) => Err(refusal),
                },
            };
            world.journal.push(JournalEntry {
                command: saved.command.clone(),
                result,
            });
            world.produced.push(saved.events);
        }
        Ok(world)
    }

    /// Runs a command. Refused: nothing changes, and the error says why. Accepted: the events
    /// it produced, already applied. Either way the journal records it.
    pub fn execute(&mut self, command: Command) -> Result<Vec<Event>, CommandError> {
        let decided = self.decide(&command);
        self.journal.push(JournalEntry {
            command,
            result: decided.as_ref().map(|_| ()).map_err(Clone::clone),
        });
        let mut emitted = Vec::new();
        let decided = match decided {
            Ok(decided) => decided,
            Err(refusal) => {
                self.produced.push(None);
                return Err(refusal);
            }
        };
        // The command's own changes first; then, from the state they leave, any band changes
        // watched subjects are owed (DESIGN.md §8.3).
        let mut moved_characters = BTreeSet::new();
        let mut moved_factions = BTreeSet::new();
        for change in &decided {
            match change {
                Change::AlignmentChanged { character, .. } => {
                    moved_characters.insert(character.clone());
                }
                Change::FactionAlignmentChanged { faction, .. } => {
                    moved_factions.insert(faction.clone());
                }
                _ => {}
            }
        }
        for change in decided {
            emitted.push(self.record(change));
        }
        // Wars between someone's own factions open or end with the relations, and any that
        // are due are settled (DESIGN.md §9.4).
        for change in self.conflict_review() {
            emitted.push(self.record(change));
        }
        for (character, leave) in self.due_resolutions() {
            // An earlier settlement in this pass may already have taken them out.
            if self.is_member(&character, &leave) {
                for change in self.resolution(&character, &[leave]) {
                    emitted.push(self.record(change));
                }
            }
        }
        // Every membership whose member or faction moved is reviewed, in id order, each from
        // the state the last review left (DESIGN.md §9.3).
        let mut reviews: BTreeSet<(CharacterId, FactionId)> = BTreeSet::new();
        for character in moved_characters {
            for faction in self.factions_of(&character) {
                reviews.insert((character.clone(), faction));
            }
        }
        for faction in moved_factions {
            for member in self.members(&faction).expect("the faction exists") {
                reviews.insert((member.clone(), faction.clone()));
            }
        }
        for (character, faction) in reviews {
            if self.is_member(&character, &faction) {
                for change in self.drift_review(&character, &faction) {
                    emitted.push(self.record(change));
                }
            }
        }
        // Then any probation that has run out (time only moves on `AdvanceTime`).
        for (character, faction) in self.expired_probations() {
            for change in self.probation_expiry(&character, &faction) {
                emitted.push(self.record(change));
            }
        }
        // Then any modifier that has run out (DESIGN.md §8.1).
        let expired: Vec<Change> = self
            .state
            .modifiers
            .iter()
            .filter(|(_, modifier)| {
                modifier
                    .expires_at
                    .is_some_and(|until| until <= self.state.now)
            })
            .map(|((subject, id), _)| Change::ModifierExpired {
                subject: subject.clone(),
                id: id.clone(),
            })
            .collect();
        for change in expired {
            emitted.push(self.record(change));
        }
        for change in self.band_changes() {
            emitted.push(self.record(change));
        }
        self.produced.push(Some(emitted.len()));
        Ok(emitted)
    }

    /// Numbers and stamps a change as the next event, and applies it.
    fn record(&mut self, change: Change) -> Event {
        let event = Event {
            seq: self.events.len() as u64 + 1,
            tick: self.state.now,
            payload: change,
        };
        self.apply(event.clone());
        event
    }

    /// Works out what a command would change, without changing anything: the rules live
    /// here, and in `band_changes`, which reacts to what an accepted command changed.
    fn decide(&self, command: &Command) -> Result<Vec<Change>, CommandError> {
        match command {
            &Command::AdvanceTime { ticks } => {
                if ticks == 0 {
                    return Err(CommandError::NoTicks);
                }
                let now = self.state.now;
                let to = now
                    .0
                    .checked_add(ticks)
                    .ok_or(CommandError::TimeOverflow { now, ticks })?;
                Ok(vec![Change::TimeAdvanced {
                    from: now,
                    to: Tick(to),
                }])
            }
            Command::PerformAction {
                actor,
                action,
                target,
                scale,
                witnesses,
            } => {
                let from = self.existing(actor, Role::Actor)?;
                let catalogued = self.content.actions.get(action).ok_or_else(|| {
                    CommandError::UnknownAction {
                        action: action.clone(),
                        suggestion: closest(action.as_str(), self.content.actions.keys()),
                    }
                })?;
                if let Some(target) = target {
                    self.existing(target, Role::Target)?;
                    if target == actor {
                        return Err(CommandError::TargetIsActor);
                    }
                }
                if *scale <= Fixed::ZERO {
                    return Err(CommandError::ScaleNotPositive { scale: *scale });
                }
                if let Witnesses::These(witnesses) = witnesses {
                    for witness in witnesses {
                        self.existing(witness, Role::Witness)?;
                    }
                }
                let mut changes = vec![Change::ActionPerformed {
                    actor: actor.clone(),
                    action: action.clone(),
                    target: target.clone(),
                    scale: *scale,
                    witnesses: witnesses.clone(),
                }];
                let to = self
                    .act_shift(actor, catalogued, target.as_ref(), *scale)
                    .applied_to(from);
                if to != from {
                    changes.push(Change::AlignmentChanged {
                        character: actor.clone(),
                        from,
                        to,
                    });
                }
                let effects = &catalogued.standing;
                let mut deltas = Deltas::new();
                if let Some(target) = target {
                    if let Some(change) = effects.target {
                        add(&mut deltas, Party::Character(target.clone()), change);
                    }
                    if let Some(change) = effects.target_factions {
                        let factions = self.state.memberships.get(target).into_iter().flatten();
                        for (faction, _) in factions {
                            add(&mut deltas, Party::Faction(faction.clone()), change);
                        }
                    }
                }
                for (party, change) in effects.named.parties() {
                    add(&mut deltas, party, change);
                }
                changes.extend(self.standing_changes(actor, deltas));
                Ok(changes)
            }
            Command::JoinFaction { character, faction } => {
                self.existing(character, Role::Member)?;
                let assessment = self
                    .assess_join(character, faction)
                    .ok_or_else(|| self.unknown_faction(faction))?;
                if !assessment.allowed() {
                    return Err(CommandError::JoinRefused(Box::new(assessment)));
                }
                let rank = self.content.factions[faction]
                    .lowest_rank()
                    .expect("World::new checks every faction has a rank")
                    .id
                    .clone();
                // Defecting: leave each enemy faction, paying what its deserters table asks,
                // then join, paying what the defectors table asks, once for all of them.
                let mut changes = Vec::new();
                let mut joining = Deltas::new();
                for defection in &assessment.defections {
                    let from = &defection.from;
                    changes.push(Change::LeftFaction {
                        character: character.clone(),
                        faction: from.clone(),
                        reason: LeaveReason::Defected,
                    });
                    if let Verdict::Allow { standing_change } = defection.deserters.verdict {
                        let mut leaving = Deltas::new();
                        add(&mut leaving, Party::Faction(from.clone()), standing_change);
                        changes.extend(self.standing_changes(character, leaving));
                    }
                    if let Verdict::Allow { standing_change } = defection.defectors.verdict {
                        add(
                            &mut joining,
                            Party::Faction(faction.clone()),
                            standing_change,
                        );
                    }
                }
                changes.push(Change::JoinedFaction {
                    character: character.clone(),
                    faction: faction.clone(),
                    rank,
                });
                changes.extend(self.standing_changes(character, joining));
                Ok(changes)
            }
            Command::Promote { character, faction } => {
                self.existing(character, Role::Member)?;
                self.faction(faction)
                    .ok_or_else(|| self.unknown_faction(faction))?;
                let assessment = self.assess_promotion(character, faction).ok_or_else(|| {
                    CommandError::NotAMember {
                        character: character.clone(),
                        faction: faction.clone(),
                    }
                })?;
                let Some(next) = assessment.next.clone() else {
                    return Err(CommandError::AtTopRank {
                        character: character.clone(),
                        faction: faction.clone(),
                        rank: assessment.current,
                    });
                };
                if !assessment.allowed() {
                    return Err(CommandError::PromotionRefused(Box::new(assessment)));
                }
                Ok(vec![Change::RankChanged {
                    character: character.clone(),
                    faction: faction.clone(),
                    from: assessment.current,
                    to: next,
                }])
            }
            Command::Demote { character, faction } => {
                self.existing(character, Role::Member)?;
                let found = self
                    .faction(faction)
                    .ok_or_else(|| self.unknown_faction(faction))?;
                let current = self
                    .state
                    .memberships
                    .get(character)
                    .and_then(|factions| factions.get(faction))
                    .map(|membership| membership.rank.clone())
                    .ok_or_else(|| CommandError::NotAMember {
                        character: character.clone(),
                        faction: faction.clone(),
                    })?;
                let position = found
                    .rank_position(&current)
                    .expect("a member's rank is on their faction's ladder");
                let Some(lower) = position.checked_sub(1).map(|below| &found.ranks[below]) else {
                    return Err(CommandError::AtBottomRank {
                        character: character.clone(),
                        faction: faction.clone(),
                        rank: current,
                    });
                };
                Ok(vec![Change::RankChanged {
                    character: character.clone(),
                    faction: faction.clone(),
                    from: current,
                    to: lower.id.clone(),
                }])
            }
            Command::LeaveFaction { character, faction } => {
                self.existing(character, Role::Member)?;
                self.faction(faction)
                    .ok_or_else(|| self.unknown_faction(faction))?;
                if !self.is_member(character, faction) {
                    return Err(CommandError::NotAMember {
                        character: character.clone(),
                        faction: faction.clone(),
                    });
                }
                let mut changes = vec![Change::LeftFaction {
                    character: character.clone(),
                    faction: faction.clone(),
                    reason: LeaveReason::Voluntary,
                }];
                let cost = self.content.factions[faction].leave_standing_change;
                let mut deltas = Deltas::new();
                add(&mut deltas, Party::Faction(faction.clone()), cost);
                changes.extend(self.standing_changes(character, deltas));
                Ok(changes)
            }
            Command::SetRelation {
                from,
                to,
                value,
                mutual,
            } => {
                if !(-AXIS_LIMIT..=AXIS_LIMIT).contains(value) {
                    self.relation_ends(from, to)?;
                    return Err(CommandError::ValueOutOfRange { value: *value });
                }
                self.relation_changes(from, to, *mutual, |_| *value)
            }
            Command::ShiftRelation {
                from,
                to,
                by,
                mutual,
            } => self.relation_changes(from, to, *mutual, |before| {
                // A sum too big to hold means a shift that reaches the end by itself.
                let moved = before.checked_add(*by).unwrap_or(*by);
                moved.clamp(-AXIS_LIMIT, AXIS_LIMIT)
            }),
            Command::ApplyOutcome { outcome, character } => {
                self.existing(character, Role::Member)?;
                let found = self.content.outcomes.get(outcome).ok_or_else(|| {
                    CommandError::UnknownOutcome {
                        outcome: outcome.clone(),
                        suggestion: closest(outcome.as_str(), self.content.outcomes.keys()),
                    }
                })?;
                let mut changes = vec![Change::OutcomeApplied {
                    outcome: outcome.clone(),
                    character: character.clone(),
                }];
                changes.extend(self.effect_changes(character, &found.effects));
                Ok(changes)
            }
            Command::SetFactionAlignment { faction, alignment } => {
                let from = self
                    .faction_alignment(faction)
                    .ok_or_else(|| self.unknown_faction(faction))?;
                Ok(faction_moved(faction, from, *alignment))
            }
            Command::ShiftFactionAlignment { faction, by } => {
                let from = self
                    .faction_alignment(faction)
                    .ok_or_else(|| self.unknown_faction(faction))?;
                // A sum too big to hold means a shift that reaches the end by itself.
                let axis = |position: Fixed, shift: Fixed| {
                    position
                        .checked_add(shift)
                        .unwrap_or(shift)
                        .clamp(-AXIS_LIMIT, AXIS_LIMIT)
                };
                let to = Alignment::new(axis(from.law(), by.law), axis(from.good(), by.good))
                    .expect("clamped to the axes");
                Ok(faction_moved(faction, from, to))
            }
            Command::ResolveConflict { character, keep } => {
                self.existing(character, Role::Member)?;
                self.faction(keep)
                    .ok_or_else(|| self.unknown_faction(keep))?;
                let others: Vec<FactionId> = self
                    .state
                    .conflicts
                    .keys()
                    .filter(|(who, _, _)| who == character)
                    .filter_map(|(_, a, b)| match (a == keep, b == keep) {
                        (true, _) => Some(b.clone()),
                        (_, true) => Some(a.clone()),
                        _ => None,
                    })
                    .collect();
                if others.is_empty() {
                    return Err(CommandError::NoConflict {
                        character: character.clone(),
                        faction: keep.clone(),
                    });
                }
                Ok(self.resolution(character, &others))
            }
            Command::AddModifier {
                id,
                observer,
                subject,
                amount,
                expires_at,
            } => {
                self.existing(subject, Role::Subject)?;
                match observer {
                    ModifierObserver::Everyone => {}
                    ModifierObserver::Faction(faction) => {
                        self.faction(faction)
                            .ok_or_else(|| self.unknown_faction(faction))?;
                    }
                    ModifierObserver::Character(character) => {
                        self.existing(character, Role::Observer)?;
                    }
                }
                if !within_range(*amount) {
                    return Err(CommandError::ValueOutOfRange { value: *amount });
                }
                if let Some(at) = expires_at.filter(|at| *at <= self.state.now) {
                    return Err(CommandError::ExpiryNotAfterNow {
                        expires_at: at,
                        now: self.state.now,
                    });
                }
                if self
                    .state
                    .modifiers
                    .contains_key(&(subject.clone(), id.clone()))
                {
                    return Err(CommandError::AlreadyModified {
                        subject: subject.clone(),
                        id: id.clone(),
                    });
                }
                Ok(vec![Change::ModifierAdded {
                    subject: subject.clone(),
                    id: id.clone(),
                    observer: observer.clone(),
                    amount: *amount,
                    expires_at: *expires_at,
                }])
            }
            Command::RemoveModifier { subject, id } => {
                self.existing(subject, Role::Subject)?;
                if !self
                    .state
                    .modifiers
                    .contains_key(&(subject.clone(), id.clone()))
                {
                    return Err(CommandError::NoSuchModifier {
                        subject: subject.clone(),
                        id: id.clone(),
                    });
                }
                Ok(vec![Change::ModifierRemoved {
                    subject: subject.clone(),
                    id: id.clone(),
                }])
            }
            Command::Watch { subject } => {
                self.existing(subject, Role::Subject)?;
                if self.state.watched.contains_key(subject) {
                    return Err(CommandError::AlreadyWatched {
                        subject: subject.clone(),
                    });
                }
                let bands = self
                    .observers_of(subject)
                    .into_iter()
                    .map(|observer| {
                        let band = self
                            .disposition(&observer, subject)
                            .expect("both exist")
                            .band;
                        (observer, band)
                    })
                    .collect();
                Ok(vec![Change::Watched {
                    subject: subject.clone(),
                    bands,
                }])
            }
            Command::Unwatch { subject } => {
                self.existing(subject, Role::Subject)?;
                if !self.state.watched.contains_key(subject) {
                    return Err(CommandError::NotWatched {
                        subject: subject.clone(),
                    });
                }
                Ok(vec![Change::Unwatched {
                    subject: subject.clone(),
                }])
            }
            Command::ApplyEffects {
                source,
                character,
                effects,
            } => {
                self.existing(character, Role::Member)?;
                for (party, change) in effects.standing.parties() {
                    match &party {
                        Party::Faction(faction) => {
                            self.faction(faction)
                                .ok_or_else(|| self.unknown_faction(faction))?;
                        }
                        Party::Character(id) => {
                            self.existing(id, Role::Member)?;
                        }
                    }
                    if !(-AXIS_LIMIT..=AXIS_LIMIT).contains(&change) {
                        return Err(CommandError::ValueOutOfRange { value: change });
                    }
                }
                let mut changes = vec![Change::EffectsApplied {
                    source: source.clone(),
                    character: character.clone(),
                }];
                changes.extend(self.effect_changes(character, effects));
                Ok(changes)
            }
        }
    }

    /// The changes from applying `effects` to `character`: alignment as an action at scale
    /// 1.00 would move it, then standing.
    fn effect_changes(&self, character: &CharacterId, effects: &Effects) -> Vec<Change> {
        let mut changes = Vec::new();
        let from = self.state.alignments[character];
        let to = from.shifted(effects.alignment, Fixed::ONE, self.inertia_of(character).1);
        if to != from {
            changes.push(Change::AlignmentChanged {
                character: character.clone(),
                from,
                to,
            });
        }
        let mut deltas = Deltas::new();
        for (party, change) in effects.standing.parties() {
            add(&mut deltas, party, change);
        }
        changes.extend(self.standing_changes(character, deltas));
        changes
    }

    /// One `StandingChanged` for each party whose standing toward `subject` moves, in party
    /// order. Each change is scaled by the party's awareness of it, and stops at ±100.
    fn standing_changes(&self, subject: &CharacterId, deltas: Deltas) -> Vec<Change> {
        // Each party's direct change first, as before and after.
        let mut moved: BTreeMap<Party, (Fixed, Fixed, Vec<Spill>)> = deltas
            .into_iter()
            .map(|(party, delta)| {
                let change = delta.saturating_mul(self.awareness(&party));
                let before = self.standing_now(subject, &party);
                // A sum too big to hold means a change that reaches the end by itself.
                let after = before
                    .checked_add(change)
                    .unwrap_or(change)
                    .clamp(-AXIS_LIMIT, AXIS_LIMIT);
                (party, (before, after, Vec::new()))
            })
            .collect();
        // Then one hop of spillover from each faction's change as applied (DESIGN.md §7.1,
        // P-46): spills come only from direct changes, never from other spills.
        let direct: Vec<(FactionId, Fixed)> = moved
            .iter()
            .filter_map(|(party, (before, after, _))| match party {
                // A change that landed as 0 spills 0, which is left out below.
                Party::Faction(faction) => Some((faction.clone(), *after - *before)),
                Party::Character(_) => None,
            })
            .collect();
        for (from, change) in direct {
            for to in self.content.factions.keys().filter(|to| **to != from) {
                let relation = relation_value(&self.state.relations, to, &from);
                let multiplier = self.content.balance.spillover.exact_at(relation);
                let amount = Ratio::from_fixed(change)
                    .checked_mul(multiplier)
                    .and_then(Ratio::round)
                    .expect("a change within ±200 times a multiplier within ±1 fits");
                if amount == Fixed::ZERO {
                    continue;
                }
                let party = Party::Faction(to.clone());
                let (_, after, spills) = moved.entry(party.clone()).or_insert_with(|| {
                    let before = self.standing_now(subject, &party);
                    (before, before, Vec::new())
                });
                *after = (*after + amount).clamp(-AXIS_LIMIT, AXIS_LIMIT);
                spills.push(Spill {
                    from: from.clone(),
                    change,
                    relation,
                    multiplier,
                    amount,
                });
            }
        }
        moved
            .into_iter()
            .filter(|(_, (before, after, _))| after != before)
            .map(
                |(party, (before, after, spilled))| Change::StandingChanged {
                    subject: subject.clone(),
                    party,
                    before,
                    after,
                    spilled,
                },
            )
            .collect()
    }

    /// How much `party` learns of an act: 1.00 under the omniscient knowledge model, the only
    /// one so far. Later models replace this seam (DESIGN.md §10, D-7).
    fn awareness(&self, party: &Party) -> Fixed {
        let _ = party;
        Fixed::ONE
    }

    /// A character's inertia profile, and its id: their own, or the default. Content checks
    /// both exist (P-32).
    fn inertia_of(&self, character: &CharacterId) -> (&ProfileId, &InertiaProfile) {
        let inertia = &self.content.balance.inertia;
        let id = self.content.characters[character]
            .inertia
            .as_ref()
            .unwrap_or(&inertia.default_profile);
        (id, &inertia.profiles[id])
    }

    /// Everyone who has a view of `subject`: every faction, then every other character,
    /// each in id order.
    fn observers_of(&self, subject: &CharacterId) -> Vec<Observer> {
        let factions = self.content.factions.keys().cloned().map(Observer::Faction);
        let characters = self
            .content
            .characters
            .keys()
            .filter(|character| *character != subject)
            .cloned()
            .map(Observer::Character);
        factions.chain(characters).collect()
    }

    /// What `faction`'s drift policy does now about `character`, a member whose alignment
    /// just moved (DESIGN.md §9.3).
    fn drift_review(&self, character: &CharacterId, faction: &FactionId) -> Vec<Change> {
        let found = &self.content.factions[faction];
        let membership = &self.state.memberships[character][faction];
        let rung = found
            .rank_position(&membership.rank)
            .expect("a member's rank is on their faction's ladder");
        let distance = self
            .distance(&Observer::Faction(faction.clone()), character)
            .expect("both exist")
            .value;
        let tolerance = found.member_tolerance(rung);
        let flagged = self
            .state
            .out_of_tolerance
            .contains(&(character.clone(), faction.clone()));
        let policy = found.drift.unwrap_or(self.content.balance.default_drift);
        let mut changes = Vec::new();
        match policy {
            DriftPolicy::Ignore => {}
            DriftPolicy::Flag => {
                if distance > tolerance && !flagged {
                    changes.push(Change::MemberOutOfTolerance {
                        character: character.clone(),
                        faction: faction.clone(),
                        distance,
                        tolerance,
                    });
                } else if distance <= tolerance && flagged {
                    changes.push(Change::MemberBackInTolerance {
                        character: character.clone(),
                        faction: faction.clone(),
                        distance,
                        tolerance,
                    });
                }
            }
            DriftPolicy::Demote => changes.extend(self.demotion(character, found, rung, distance)),
            DriftPolicy::Expel => {
                if distance > tolerance {
                    changes.extend(self.expulsion(character, found));
                }
            }
            DriftPolicy::Probation { grace_ticks, .. } => {
                let key = (character.clone(), faction.clone());
                let on_probation = self.state.probation.contains_key(&key);
                if distance > tolerance && !on_probation {
                    changes.push(Change::ProbationStarted {
                        character: character.clone(),
                        faction: faction.clone(),
                        until: Tick(self.state.now.0.saturating_add(grace_ticks)),
                    });
                } else if distance <= tolerance && on_probation {
                    changes.push(Change::ProbationCleared {
                        character: character.clone(),
                        faction: faction.clone(),
                    });
                }
            }
        }
        changes
    }

    /// `character` down a rung at a time from `rung` to the highest whose tolerance allows
    /// them at `distance`, or out if none does.
    fn demotion(
        &self,
        character: &CharacterId,
        found: &Faction,
        rung: usize,
        distance: Fixed,
    ) -> Vec<Change> {
        let allowed = (0..=rung)
            .rev()
            .find(|&lower| distance <= found.member_tolerance(lower));
        let mut changes: Vec<Change> = (allowed.unwrap_or(0)..rung)
            .rev()
            .map(|step| Change::RankChanged {
                character: character.clone(),
                faction: found.id.clone(),
                from: found.ranks[step + 1].id.clone(),
                to: found.ranks[step].id.clone(),
            })
            .collect();
        if allowed.is_none() {
            changes.extend(self.expulsion(character, found));
        }
        changes
    }

    /// Wars that opened or ended with the relations as they are now: for each character in
    /// id order, each pair of their factions in id order (DESIGN.md §9.4).
    fn conflict_review(&self) -> Vec<Change> {
        let threshold = self.content.balance.conflict_threshold;
        let mut changes = Vec::new();
        for (character, factions) in &self.state.memberships {
            let factions: Vec<&FactionId> = factions.keys().collect();
            for (index, a) in factions.iter().enumerate() {
                for b in &factions[index + 1..] {
                    let at_war = hostility(&self.state.relations, a, b) <= threshold;
                    let key = (character.clone(), (*a).clone(), (*b).clone());
                    let pair = ((*a).clone(), (*b).clone());
                    match (at_war, self.state.conflicts.contains_key(&key)) {
                        (true, false) => changes.push(Change::MembershipConflict {
                            character: character.clone(),
                            factions: pair,
                        }),
                        (false, true) => changes.push(Change::MembershipConflictEnded {
                            character: character.clone(),
                            factions: pair,
                        }),
                        _ => {}
                    }
                }
            }
        }
        changes
    }

    /// Open wars the automatic rule settles now, in id order, as `(character, faction to
    /// leave)`. It keeps the higher rung, then the higher standing, then the longer service,
    /// then the lower id (DESIGN.md §9.4).
    fn due_resolutions(&self) -> Vec<(CharacterId, FactionId)> {
        let Some(after) = self.content.balance.conflict.auto_after() else {
            return Vec::new();
        };
        self.state
            .conflicts
            .iter()
            .filter(|(_, since)| Tick(since.0.saturating_add(after)) <= self.state.now)
            .map(|((character, a, b), _)| {
                let claim = |faction: &FactionId| {
                    let membership = &self.state.memberships[character][faction];
                    let rung = self.content.factions[faction]
                        .rank_position(&membership.rank)
                        .expect("a member's rank is on their faction's ladder");
                    let standing = self.standing_now(character, &Party::Faction(faction.clone()));
                    // Earlier service, then the lower id, count for more.
                    (
                        rung,
                        standing,
                        Reverse(membership.since),
                        Reverse(faction.clone()),
                    )
                };
                let leave = if claim(a) >= claim(b) { b } else { a };
                (character.clone(), leave.clone())
            })
            .collect()
    }

    /// `character` leaves each of `leave` to settle a war, paying each one's
    /// `leave_standing_change` in one standing step, which spills (P-49).
    fn resolution(&self, character: &CharacterId, leave: &[FactionId]) -> Vec<Change> {
        let mut changes: Vec<Change> = leave
            .iter()
            .map(|faction| Change::LeftFaction {
                character: character.clone(),
                faction: faction.clone(),
                reason: LeaveReason::ConflictResolved,
            })
            .collect();
        let mut cost = Deltas::new();
        for faction in leave {
            add(
                &mut cost,
                Party::Faction(faction.clone()),
                self.content.factions[faction].leave_standing_change,
            );
        }
        changes.extend(self.standing_changes(character, cost));
        changes
    }

    /// Every probation that has run out by now, in id order.
    fn expired_probations(&self) -> Vec<(CharacterId, FactionId)> {
        self.state
            .probation
            .iter()
            .filter(|(_, until)| **until <= self.state.now)
            .map(|(key, _)| key.clone())
            .collect()
    }

    /// What happens when `character`'s probation in `faction` runs out: its `then`. Reviews
    /// clear a probation as soon as they're back, so they're still out.
    fn probation_expiry(&self, character: &CharacterId, faction: &FactionId) -> Vec<Change> {
        let found = &self.content.factions[faction];
        let DriftPolicy::Probation { then, .. } =
            found.drift.unwrap_or(self.content.balance.default_drift)
        else {
            unreachable!("only a faction on probation puts members on probation")
        };
        let rung = found
            .rank_position(&self.state.memberships[character][faction].rank)
            .expect("a member's rank is on their faction's ladder");
        let distance = self
            .distance(&Observer::Faction(faction.clone()), character)
            .expect("both exist")
            .value;
        let mut changes = vec![Change::ProbationExpired {
            character: character.clone(),
            faction: faction.clone(),
        }];
        changes.extend(match then {
            Consequence::Expel => self.expulsion(character, found),
            Consequence::Demote => self.demotion(character, found, rung, distance),
        });
        changes
    }

    /// `character` thrown out of `faction`, at its `expel_standing_change`, which spills like
    /// any standing change.
    fn expulsion(&self, character: &CharacterId, faction: &Faction) -> Vec<Change> {
        let mut changes = vec![Change::LeftFaction {
            character: character.clone(),
            faction: faction.id.clone(),
            reason: LeaveReason::Expelled,
        }];
        let mut cost = Deltas::new();
        add(
            &mut cost,
            Party::Faction(faction.id.clone()),
            faction.expel_standing_change,
        );
        changes.extend(self.standing_changes(character, cost));
        changes
    }

    /// The band changes the world now owes watched subjects: for each, in id order, every
    /// observer whose disposition has left the band they last put the subject in (DESIGN.md
    /// §8.3).
    fn band_changes(&self) -> Vec<Change> {
        let balance = &self.content.balance;
        let mut changes = Vec::new();
        for (subject, remembered) in &self.state.watched {
            for (observer, current) in remembered {
                let score = self
                    .disposition(observer, subject)
                    .expect("watched subjects and their observers exist")
                    .score;
                let to = &balance
                    .bands
                    .band_after(current, score, balance.hysteresis)
                    .name;
                if to != current {
                    changes.push(Change::DispositionBandChanged {
                        observer: observer.clone(),
                        subject: subject.clone(),
                        from: current.clone(),
                        to: to.clone(),
                        score,
                    });
                }
            }
        }
        changes
    }

    /// A character's factions now, in id order.
    fn factions_of(&self, character: &CharacterId) -> Vec<FactionId> {
        self.state
            .memberships
            .get(character)
            .into_iter()
            .flat_map(BTreeMap::keys)
            .cloned()
            .collect()
    }

    fn standing_now(&self, subject: &CharacterId, party: &Party) -> Fixed {
        self.state
            .standings
            .get(&(subject.clone(), party.clone()))
            .copied()
            .unwrap_or_default()
    }

    /// Checks a relation's two ends: both exist, and they're different factions.
    fn relation_ends(&self, from: &FactionId, to: &FactionId) -> Result<(), CommandError> {
        for end in [from, to] {
            self.faction(end).ok_or_else(|| self.unknown_faction(end))?;
        }
        if from == to {
            return Err(CommandError::SelfRelation);
        }
        Ok(())
    }

    /// The changes from giving `from → to` (and with `mutual`, `to → from`) the value `after`
    /// works out from what's there now. A war it starts between two of someone's factions is
    /// reported after it (DESIGN.md §9.4).
    fn relation_changes(
        &self,
        from: &FactionId,
        to: &FactionId,
        mutual: bool,
        after: impl Fn(Fixed) -> Fixed,
    ) -> Result<Vec<Change>, CommandError> {
        self.relation_ends(from, to)?;
        let mut directions = vec![(from.clone(), to.clone())];
        if mutual {
            directions.push((to.clone(), from.clone()));
        }
        let mut relations = self.state.relations.clone();
        let mut changes = Vec::new();
        for (from, to) in directions {
            let before = relations
                .get(&(from.clone(), to.clone()))
                .copied()
                .unwrap_or_default();
            let after = after(before);
            if after != before {
                relations.insert((from.clone(), to.clone()), after);
                changes.push(Change::RelationChanged {
                    from,
                    to,
                    before,
                    after,
                });
            }
        }
        Ok(changes)
    }

    fn unknown_faction(&self, faction: &FactionId) -> CommandError {
        CommandError::UnknownFaction {
            faction: faction.clone(),
            suggestion: closest(faction.as_str(), self.content.factions.keys()),
        }
    }

    fn is_member(&self, character: &CharacterId, faction: &FactionId) -> bool {
        self.state
            .memberships
            .get(character)
            .is_some_and(|factions| factions.contains_key(faction))
    }

    /// A character's alignment now, or a refusal naming them in their `role`.
    fn existing(&self, id: &CharacterId, role: Role) -> Result<Alignment, CommandError> {
        self.state
            .alignments
            .get(id)
            .copied()
            .ok_or_else(|| CommandError::UnknownCharacter {
                role,
                id: id.clone(),
                suggestion: closest(id.as_str(), self.state.alignments.keys()),
            })
    }

    /// Records an event and makes its change: the only place state changes. No rules run
    /// here, so replaying events needs none (P-15).
    fn apply(&mut self, event: Event) {
        match event.payload {
            Change::TimeAdvanced { to, .. } => self.state.now = to,
            Change::ActionPerformed { .. } => {}
            Change::JoinedFaction {
                ref character,
                ref faction,
                ref rank,
            } => {
                let since = event.tick;
                self.state
                    .memberships
                    .entry(character.clone())
                    .or_default()
                    .insert(
                        faction.clone(),
                        Membership {
                            since,
                            rank: rank.clone(),
                        },
                    );
            }
            Change::RankChanged {
                ref character,
                ref faction,
                ref to,
                ..
            } => {
                if let Some(membership) = self
                    .state
                    .memberships
                    .get_mut(character)
                    .and_then(|factions| factions.get_mut(faction))
                {
                    membership.rank = to.clone();
                }
            }
            Change::LeftFaction {
                ref character,
                ref faction,
                ..
            } => {
                if let Some(factions) = self.state.memberships.get_mut(character) {
                    factions.remove(faction);
                    if factions.is_empty() {
                        self.state.memberships.remove(character);
                    }
                }
                self.state
                    .out_of_tolerance
                    .remove(&(character.clone(), faction.clone()));
                self.state
                    .probation
                    .remove(&(character.clone(), faction.clone()));
                self.state
                    .conflicts
                    .retain(|(who, a, b), _| who != character || (a != faction && b != faction));
            }
            Change::ProbationStarted {
                ref character,
                ref faction,
                until,
            } => {
                self.state
                    .probation
                    .insert((character.clone(), faction.clone()), until);
            }
            Change::ProbationCleared {
                ref character,
                ref faction,
            }
            | Change::ProbationExpired {
                ref character,
                ref faction,
            } => {
                self.state
                    .probation
                    .remove(&(character.clone(), faction.clone()));
            }
            Change::MembershipConflict {
                ref character,
                factions: (ref a, ref b),
            } => {
                self.state
                    .conflicts
                    .insert((character.clone(), a.clone(), b.clone()), event.tick);
            }
            Change::MembershipConflictEnded {
                ref character,
                factions: (ref a, ref b),
            } => {
                self.state
                    .conflicts
                    .remove(&(character.clone(), a.clone(), b.clone()));
            }
            Change::ModifierAdded {
                ref subject,
                ref id,
                ref observer,
                amount,
                expires_at,
            } => {
                self.state.modifiers.insert(
                    (subject.clone(), id.clone()),
                    Modifier {
                        observer: observer.clone(),
                        amount,
                        expires_at,
                    },
                );
            }
            Change::ModifierRemoved {
                ref subject,
                ref id,
            }
            | Change::ModifierExpired {
                ref subject,
                ref id,
            } => {
                self.state.modifiers.remove(&(subject.clone(), id.clone()));
            }
            Change::FactionAlignmentChanged {
                ref faction, to, ..
            } => {
                self.state.faction_alignments.insert(faction.clone(), to);
            }
            Change::MemberOutOfTolerance {
                ref character,
                ref faction,
                ..
            } => {
                self.state
                    .out_of_tolerance
                    .insert((character.clone(), faction.clone()));
            }
            Change::MemberBackInTolerance {
                ref character,
                ref faction,
                ..
            } => {
                self.state
                    .out_of_tolerance
                    .remove(&(character.clone(), faction.clone()));
            }
            Change::AlignmentChanged {
                ref character, to, ..
            } => {
                self.state.alignments.insert(character.clone(), to);
            }
            Change::RelationChanged {
                ref from,
                ref to,
                after,
                ..
            } => {
                self.state
                    .relations
                    .insert((from.clone(), to.clone()), after);
            }
            Change::StandingChanged {
                ref subject,
                ref party,
                after,
                ..
            } => {
                self.state
                    .standings
                    .insert((subject.clone(), party.clone()), after);
            }
            Change::OutcomeApplied { .. } | Change::EffectsApplied { .. } => {}
            Change::Watched {
                ref subject,
                ref bands,
            } => {
                self.state
                    .watched
                    .insert(subject.clone(), bands.iter().cloned().collect());
            }
            Change::Unwatched { ref subject } => {
                self.state.watched.remove(subject);
            }
            Change::DispositionBandChanged {
                ref observer,
                ref subject,
                ref to,
                ..
            } => {
                if let Some(bands) = self.state.watched.get_mut(subject) {
                    bands.insert(observer.clone(), to.clone());
                }
            }
        }
        self.events.push(event);
    }

    /// The current tick.
    pub fn now(&self) -> Tick {
        self.state.now
    }

    /// Every event so far, oldest first.
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    /// The events after `seq`, oldest first.
    pub fn events_since(&self, seq: u64) -> &[Event] {
        // Events are numbered from 1 with no gaps, so event `seq` sits at index `seq - 1`.
        let start =
            usize::try_from(seq).map_or(self.events.len(), |seq| seq.min(self.events.len()));
        &self.events[start..]
    }

    /// Every command issued, in order, and whether it was accepted.
    pub fn journal(&self) -> &[JournalEntry] {
        &self.journal
    }

    pub fn balance(&self) -> &Balance {
        &self.content.balance
    }

    /// Every character, in id order.
    pub fn characters(&self) -> impl Iterator<Item = &Character> {
        self.content.characters.values()
    }

    pub fn character(&self, id: &CharacterId) -> Option<&Character> {
        self.content.characters.get(id)
    }

    /// A character's alignment now.
    pub fn alignment(&self, id: &CharacterId) -> Option<Alignment> {
        self.state.alignments.get(id).copied()
    }

    /// The action catalogue, in id order.
    pub fn actions(&self) -> impl Iterator<Item = &Action> {
        self.content.actions.values()
    }

    /// Every faction, in id order.
    pub fn factions(&self) -> impl Iterator<Item = &Faction> {
        self.content.factions.values()
    }

    pub fn faction(&self, id: &FactionId) -> Option<&Faction> {
        self.content.factions.get(id)
    }

    /// A character's factions now, in id order, with when they joined. `None` for an unknown
    /// character; empty for one in no faction.
    pub fn memberships(
        &self,
        character: &CharacterId,
    ) -> Option<impl Iterator<Item = (&FactionId, &Membership)>> {
        self.character(character)?;
        Some(
            self.state
                .memberships
                .get(character)
                .into_iter()
                .flat_map(BTreeMap::iter),
        )
    }

    /// How `party` regards `subject` now, from what has passed between them (DESIGN.md §7.1).
    /// `None` if either is unknown.
    pub fn standing(&self, subject: &CharacterId, party: &Party) -> Option<Fixed> {
        self.character(subject)?;
        match party {
            Party::Faction(id) => self.faction(id).map(|_| ()),
            Party::Character(id) => self.character(id).map(|_| ()),
        }?;
        Some(self.standing_now(subject, party))
    }

    /// Everyone who regards `subject` other than neutrally, factions first, each in id order.
    /// `None` for an unknown character.
    pub fn standings(&self, subject: &CharacterId) -> Option<Vec<(Party, Fixed)>> {
        self.character(subject)?;
        Some(
            self.state
                .standings
                .iter()
                .filter(|((who, _), value)| who == subject && **value != Fixed::ZERO)
                .map(|((_, party), value)| (party.clone(), *value))
                .collect(),
        )
    }

    /// How `actor` doing `action` to `target` at `scale` would move them now, with who it's
    /// done to and their inertia: each axis it touches, with its working (DESIGN.md §5.2–5.4).
    /// `None` if the actor, action or target is unknown.
    pub fn action_shift(
        &self,
        actor: &CharacterId,
        action: &ActionId,
        target: Option<&CharacterId>,
        scale: Fixed,
    ) -> Option<Shift> {
        self.alignment(actor)?;
        let action = self.content.actions.get(action)?;
        if let Some(target) = target {
            self.alignment(target)?;
        }
        Some(self.act_shift(actor, action, target, scale))
    }

    /// The working of an act whose actor, and target if any, exist.
    fn act_shift(
        &self,
        actor: &CharacterId,
        action: &Action,
        target: Option<&CharacterId>,
        scale: Fixed,
    ) -> Shift {
        let from = self.state.alignments[actor];
        let (profile, inertia) = self.inertia_of(actor);
        let target = target.map(|target| Target {
            curves: &action.by_target,
            alignment: self.state.alignments[target],
            relation: self.target_relation(actor, target),
        });
        Shift {
            profile: profile.clone(),
            axes: shifts(from, action.alignment, scale, target.as_ref(), inertia),
        }
    }

    /// The most hostile relation from any of `actor`'s factions toward any of `target`'s,
    /// the first in id order on a tie; a faction and itself don't count, and with no pair
    /// left it's 0 (DESIGN.md §5.4).
    fn target_relation(&self, actor: &CharacterId, target: &CharacterId) -> TargetRelation {
        let theirs = self.factions_of(target);
        let mut most_hostile = TargetRelation {
            value: Fixed::ZERO,
            between: None,
        };
        for from in self.factions_of(actor) {
            for to in theirs.iter().filter(|to| **to != from) {
                let value = relation_value(&self.state.relations, &from, to);
                if most_hostile.between.is_none() || value < most_hostile.value {
                    most_hostile = TargetRelation {
                        value,
                        between: Some((from.clone(), to.clone())),
                    };
                }
            }
        }
        most_hostile
    }

    /// How `delta` × `scale` would move `character` now, with their inertia: each axis it
    /// touches, with its working (DESIGN.md §5.2, §5.3). `None` for an unknown character.
    pub fn shift(
        &self,
        character: &CharacterId,
        delta: AlignmentDelta,
        scale: Fixed,
    ) -> Option<Shift> {
        let from = self.alignment(character)?;
        let (profile, inertia) = self.inertia_of(character);
        Some(Shift {
            profile: profile.clone(),
            axes: shifts(from, delta, scale, None, inertia),
        })
    }

    /// The modifiers on how `subject` is seen, in id order, each with when it expires.
    /// `None` for an unknown character.
    pub fn modifiers(&self, subject: &CharacterId) -> Option<Vec<(AppliedModifier, Option<Tick>)>> {
        self.character(subject)?;
        Some(
            self.state
                .modifiers
                .iter()
                .filter(|((who, _), _)| who == subject)
                .map(|((_, id), modifier)| {
                    (
                        AppliedModifier {
                            id: id.clone(),
                            observer: modifier.observer.clone(),
                            amount: modifier.amount,
                        },
                        modifier.expires_at,
                    )
                })
                .collect(),
        )
    }

    /// `character`'s open wars between two of their factions, each pair in id order.
    /// `None` for an unknown character.
    pub fn conflicts(&self, character: &CharacterId) -> Option<Vec<(FactionId, FactionId)>> {
        self.character(character)?;
        Some(
            self.state
                .conflicts
                .keys()
                .filter(|(who, _, _)| who == character)
                .map(|(_, a, b)| (a.clone(), b.clone()))
                .collect(),
        )
    }

    /// `faction`'s alignment now, which commands can change (DESIGN.md §9.1). `None` for an
    /// unknown faction.
    pub fn faction_alignment(&self, faction: &FactionId) -> Option<Alignment> {
        self.state.faction_alignments.get(faction).copied()
    }

    /// What `faction` does about members who drift (DESIGN.md §9.3): its own policy, or
    /// the balance default. `None` for an unknown faction.
    pub fn drift_policy(&self, faction: &FactionId) -> Option<DriftPolicy> {
        let found = self.faction(faction)?;
        Some(found.drift.unwrap_or(self.content.balance.default_drift))
    }

    /// Every watched subject, in id order, with the band each observer last put them in.
    pub fn watched(&self) -> impl Iterator<Item = (&CharacterId, &BTreeMap<Observer, String>)> {
        self.state.watched.iter()
    }

    /// The outcomes, in id order.
    pub fn outcomes(&self) -> impl Iterator<Item = &Outcome> {
        self.content.outcomes.values()
    }

    /// How `from` regards `to` now, with its band (DESIGN.md §9.4). `None` if either is unknown.
    pub fn relation(&self, from: &FactionId, to: &FactionId) -> Option<Regard> {
        self.faction(from)?;
        self.faction(to)?;
        let value = self
            .state
            .relations
            .get(&(from.clone(), to.clone()))
            .copied()
            .unwrap_or_default();
        Some(self.regard(from, to, value))
    }

    fn regard(&self, from: &FactionId, to: &FactionId, value: Fixed) -> Regard {
        Regard {
            from: from.clone(),
            to: to.clone(),
            value,
            band: self
                .content
                .balance
                .relation_bands
                .band_for(value)
                .name
                .clone(),
        }
    }

    /// Whether either faction regards the other at or below `relations.conflict_threshold`.
    /// `None` if either is unknown.
    pub fn in_conflict(&self, a: &FactionId, b: &FactionId) -> Option<bool> {
        self.faction(a)?;
        self.faction(b)?;
        Some(hostility(&self.state.relations, a, b) <= self.content.balance.conflict_threshold)
    }

    /// Every direction written in content or set since, in `(from, to)` order.
    pub fn relations(&self) -> Vec<Regard> {
        self.state
            .relations
            .iter()
            .map(|((from, to), value)| self.regard(from, to, *value))
            .collect()
    }

    /// A faction's members now, in id order. `None` for an unknown faction.
    pub fn members(&self, faction: &FactionId) -> Option<Vec<&CharacterId>> {
        self.faction(faction)?;
        Some(
            self.state
                .memberships
                .iter()
                .filter(|(_, factions)| factions.contains_key(faction))
                .map(|(character, _)| character)
                .collect(),
        )
    }

    /// Whether `character` may be promoted in `faction` now, and every requirement of the
    /// next rank with whether it holds (DESIGN.md §7.2). `None` if either is unknown or the
    /// character isn't a member.
    pub fn assess_promotion(
        &self,
        character: &CharacterId,
        faction: &FactionId,
    ) -> Option<PromotionAssessment> {
        let found = self.faction(faction)?;
        let current = self
            .state
            .memberships
            .get(character)?
            .get(faction)?
            .rank
            .clone();
        let position = found
            .rank_position(&current)
            .expect("a member's rank is on their faction's ladder");
        let next = found.ranks.get(position + 1);
        let mut checks = Vec::new();
        if let Some(next) = next {
            if let Some(required) = next.requires_standing {
                let has = self.standing_now(character, &Party::Faction(faction.clone()));
                checks.push(RankCheck::Standing { required, has });
            }
            if let Some(limit) = next.tolerance {
                let distance = self
                    .distance(&Observer::Faction(faction.clone()), character)
                    .expect("both exist")
                    .value;
                checks.push(RankCheck::Tolerance { limit, distance });
            }
        }
        Some(PromotionAssessment {
            character: character.clone(),
            faction: faction.clone(),
            faction_name: found.name.clone(),
            current,
            next: next.map(|rank| rank.id.clone()),
            checks,
        })
    }

    /// Whether `character` may join `faction` now, and every reason they can't, with how the
    /// rule tables decided for each faction they're in that's its enemy (DESIGN.md §9.1,
    /// §9.2). `None` if either is unknown.
    pub fn assess_join(
        &self,
        character: &CharacterId,
        faction: &FactionId,
    ) -> Option<JoinAssessment> {
        let found = self.faction(faction)?;
        let distance = self.distance(&Observer::Faction(faction.clone()), character)?;
        let tolerance = found.tolerances.tolerance();
        let mut blocks = Vec::new();
        if self.is_member(character, faction) {
            blocks.push(JoinBlock::AlreadyMember);
        }
        if distance.value > tolerance {
            blocks.push(JoinBlock::OutsideTolerance {
                distance: distance.value,
                tolerance,
            });
        }
        let current = self.state.memberships.get(character).into_iter().flatten();
        let defections = current
            .filter(|(member_of, _)| *member_of != faction)
            .filter_map(|(member_of, membership)| {
                let relation = hostility(&self.state.relations, member_of, faction);
                (relation <= self.content.balance.conflict_threshold).then(|| {
                    self.defection(character, membership, member_of, found, relation, &distance)
                })
            })
            .collect();
        Some(JoinAssessment {
            character: character.clone(),
            faction: faction.clone(),
            faction_name: found.name.clone(),
            distance,
            tolerance,
            blocks,
            defections,
        })
    }

    /// Whether `from`'s deserters table lets `character` go and `target`'s defectors table
    /// takes them (DESIGN.md §9.2). `to_target` is their distance to the target.
    fn defection(
        &self,
        character: &CharacterId,
        membership: &Membership,
        from: &FactionId,
        target: &Faction,
        relation: Fixed,
        to_target: &Distance,
    ) -> Defection {
        let current = &self.content.factions[from];
        let position = current
            .rank_position(&membership.rank)
            .expect("a member's rank is on their faction's ladder");
        let situation = Situation {
            rank: membership.rank.clone(),
            rung: position + 1,
            standing_with_current: self.standing_now(character, &Party::Faction(from.clone())),
            standing_with_target: self.standing_now(character, &Party::Faction(target.id.clone())),
            distance_to_target: to_target.value,
            distance_to_current: self
                .distance(&Observer::Faction(from.clone()), character)
                .expect("both exist")
                .value,
            member_tolerance: current.member_tolerance(position),
        };
        let table = |kind: TableKind, owner: &Faction| {
            let (rules, source) = match (
                owner.rule_tables.get(&kind),
                self.content.balance.rule_tables.get(&kind),
            ) {
                (Some(own), _) => (own.clone(), TableSource::Faction),
                (None, Some(world)) => (world.clone(), TableSource::World),
                (None, None) => (kind.built_in(), TableSource::BuiltIn),
            };
            defection::decide(kind, &rules, source, owner, &situation)
        };
        Defection {
            from: from.clone(),
            from_name: current.name.clone(),
            relation,
            deserters: table(TableKind::Deserters, current),
            defectors: table(TableKind::Defectors, target),
        }
    }

    /// How `observer` regards `subject`: the score, its band and the working (DESIGN.md §8).
    /// `None` if either is unknown.
    pub fn disposition(&self, observer: &Observer, subject: &CharacterId) -> Option<Disposition> {
        let distance = self.distance(observer, subject)?;
        let balance = &self.content.balance;
        // Content checks keep the affinity within ±100 (P-32), and standing stays within ±100
        // by construction, so only the sums (kinship, faction opinion, the score) need clamping.
        let affinity = balance.affinity.at(distance.value);
        let (standing, observer_factions) = match observer {
            Observer::Character(id) => (
                self.standing_now(subject, &Party::Character(id.clone())),
                self.factions_of(id),
            ),
            Observer::Faction(id) => (
                self.standing_now(subject, &Party::Faction(id.clone())),
                vec![id.clone()],
            ),
        };
        let subject_factions = self.factions_of(subject);
        let kinship: Vec<Part> = observer_factions
            .iter()
            .flat_map(|from| {
                subject_factions.iter().map(move |to| Part {
                    from: from.clone(),
                    to: Some(to.clone()),
                    value: if from == to {
                        balance.same_faction
                    } else {
                        relation_value(&self.state.relations, from, to)
                    },
                })
            })
            .collect();
        let opinion: Vec<Part> = match observer {
            Observer::Character(_) => observer_factions
                .iter()
                .map(|faction| Part {
                    from: faction.clone(),
                    to: None,
                    value: self.standing_now(subject, &Party::Faction(faction.clone())),
                })
                .collect(),
            Observer::Faction(_) => Vec::new(),
        };
        // The modifiers for everyone, for the observer itself, and for a character's factions.
        let applies = |who: &ModifierObserver| match (who, observer) {
            (ModifierObserver::Everyone, _) => true,
            (ModifierObserver::Faction(faction), Observer::Faction(observer)) => {
                faction == observer
            }
            (ModifierObserver::Faction(faction), Observer::Character(_)) => {
                observer_factions.contains(faction)
            }
            (ModifierObserver::Character(character), Observer::Character(observer)) => {
                character == observer
            }
            (ModifierObserver::Character(_), Observer::Faction(_)) => false,
        };
        let modifiers: Vec<AppliedModifier> = self
            .state
            .modifiers
            .iter()
            .filter(|((who, _), modifier)| who == subject && applies(&modifier.observer))
            .map(|((_, id), modifier)| AppliedModifier {
                id: id.clone(),
                observer: modifier.observer.clone(),
                amount: modifier.amount,
            })
            .collect();
        // Every modifier is within ±100, so even many can't overflow before the clamp.
        let modified = modifiers
            .iter()
            .fold(Fixed::ZERO, |total, modifier| total + modifier.amount)
            .clamp(-AXIS_LIMIT, AXIS_LIMIT);
        let components: Vec<Component> = ComponentKind::ALL
            .into_iter()
            .map(|kind| {
                let (value, parts) = match kind {
                    ComponentKind::Affinity => (affinity, Vec::new()),
                    ComponentKind::Standing => (standing, Vec::new()),
                    ComponentKind::Kinship => (sum(&kinship), kinship.clone()),
                    ComponentKind::FactionOpinion => (sum(&opinion), opinion.clone()),
                    ComponentKind::Modifiers => (modified, Vec::new()),
                };
                let weight = balance.disposition_weights.get(kind);
                Component {
                    kind,
                    value,
                    weight,
                    weighted: weight * value,
                    parts,
                    modifiers: if kind == ComponentKind::Modifiers {
                        modifiers.clone()
                    } else {
                        Vec::new()
                    },
                }
            })
            .collect();
        let score = components
            .iter()
            .fold(Fixed::ZERO, |score, component| score + component.weighted)
            .clamp(-AXIS_LIMIT, AXIS_LIMIT);
        Some(Disposition {
            score,
            band: balance.bands.band_for(score).name.clone(),
            distance,
            components,
        })
    }

    /// How far a point on the alignment plane is from `faction` now, as it measures
    /// characters (DESIGN.md §6): for drawing who could join. `None` for an unknown faction.
    pub fn distance_to_point(&self, faction: &FactionId, point: Alignment) -> Option<Fixed> {
        let weights = self
            .faction(faction)?
            .weights
            .unwrap_or(self.content.balance.default_weights);
        let from = self.faction_alignment(faction)?;
        Some(measure(from, point, weights, self.content.balance.metric))
    }

    /// How far `subject` is from `observer`, as the observer sees it: measured with the
    /// observer's weights and the world's metric (DESIGN.md §6). `None` if either is unknown.
    pub fn distance(&self, observer: &Observer, subject: &CharacterId) -> Option<Distance> {
        let (from, own_weights) = match observer {
            Observer::Faction(id) => (self.faction_alignment(id)?, self.faction(id)?.weights),
            Observer::Character(id) => (self.alignment(id)?, self.character(id)?.weights),
        };
        let to = self.alignment(subject)?;
        let (weights, weights_from) = match own_weights {
            Some(weights) => (weights, WeightsFrom::Own),
            None => (self.content.balance.default_weights, WeightsFrom::Default),
        };
        let metric = self.content.balance.metric;
        Some(Distance {
            value: measure(from, to, weights, metric),
            metric,
            observer: from,
            subject: to,
            weights,
            weights_from,
        })
    }
}

/// A modifier as the world keeps it, under its subject and id.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Modifier {
    observer: ModifierObserver,
    amount: Fixed,
    expires_at: Option<Tick>,
}

/// The change a faction alignment command makes: none if it doesn't move.
fn faction_moved(faction: &FactionId, from: Alignment, to: Alignment) -> Vec<Change> {
    if to == from {
        return Vec::new();
    }
    vec![Change::FactionAlignmentChanged {
        faction: faction.clone(),
        from,
        to,
    }]
}

fn within_range(value: Fixed) -> bool {
    (-AXIS_LIMIT..=AXIS_LIMIT).contains(&value)
}

/// Standing changes by party, before they're applied.
type Deltas = BTreeMap<Party, Fixed>;

/// Adds `change` to what `party` is already due, so effects on one party make one change.
fn add(deltas: &mut Deltas, party: Party, change: Fixed) {
    let due = deltas.entry(party).or_default();
    // Every effect is checked to be within ±100, so a few added together can't overflow.
    *due = *due + change;
}

/// How `from` regards `to`; any direction not set is 0.
fn relation_value(
    relations: &BTreeMap<(FactionId, FactionId), Fixed>,
    from: &FactionId,
    to: &FactionId,
) -> Fixed {
    relations
        .get(&(from.clone(), to.clone()))
        .copied()
        .unwrap_or_default()
}

/// Parts added up and clamped to ±100, so a component never outweighs its range (P-7).
fn sum(parts: &[Part]) -> Fixed {
    // Every part is within ±100, so even many can't overflow before the clamp.
    parts
        .iter()
        .fold(Fixed::ZERO, |total, part| total + part.value)
        .clamp(-AXIS_LIMIT, AXIS_LIMIT)
}

/// The more hostile of the two directions between `a` and `b`; any direction not set is 0.
fn hostility(
    relations: &BTreeMap<(FactionId, FactionId), Fixed>,
    a: &FactionId,
    b: &FactionId,
) -> Fixed {
    relation_value(relations, a, b).min(relation_value(relations, b, a))
}

/// The id among `ids` closest to a misspelt `word`, if one is close enough to suggest.
fn closest<'a, Id>(word: &str, mut ids: impl Iterator<Item = &'a Id> + Clone) -> Option<Id>
where
    Id: Clone + AsRef<str> + 'a,
{
    let close = suggest(word, ids.clone().map(AsRef::as_ref))?;
    ids.find(|id| id.as_ref() == close).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AXIS_LIMIT, ActionStanding, AlignmentDelta, AppliedModifier, AxisShift, Band, Condition,
        ConditionCheck, Consequence, Effects, JoinBlock, LeaveReason, Observed, Part, Rank,
        RankCheck, RankRef, RelationEnds, Role, Spill, StandingEffects, StartingMembership,
        Tolerances, Verdict, Witnesses,
    };

    const fn h(hundredths: i64) -> Fixed {
        Fixed::from_hundredths(hundredths)
    }

    fn id(text: &str) -> CharacterId {
        CharacterId::new(text).expect("a valid id")
    }

    fn character(name: &str, law: i64, good: i64) -> Character {
        Character {
            id: id(&name.to_lowercase()),
            name: name.to_owned(),
            alignment: Alignment::new(h(law), h(good)).expect("in range"),
            weights: None,
            inertia: None,
            memberships: Vec::new(),
            standing: StandingEffects::default(),
        }
    }

    fn weights(law: i64, good: i64) -> Weights {
        Weights::new(h(law), h(good)).expect("valid weights")
    }

    fn faction_id(text: &str) -> FactionId {
        FactionId::new(text).expect("a valid id")
    }

    fn faction(id: &str, law: i64, good: i64, w: Option<(i64, i64)>) -> Faction {
        let (name, tolerance, member) = match id {
            "city_watch" => ("The City Watch", 40_00, 50_00),
            "lantern_guild" => ("The Lantern Guild", 45_00, 60_00),
            "temple" => ("Temple of the Dawn", 35_00, 45_00),
            _ => ("The Free Company", 60_00, 80_00),
        };
        Faction {
            id: faction_id(id),
            name: name.to_owned(),
            alignment: Alignment::new(h(law), h(good)).expect("in range"),
            weights: w.map(|(law, good)| weights(law, good)),
            tolerances: Tolerances::new(h(tolerance), Some(h(member))).expect("valid"),
            leave_standing_change: Fixed::ZERO,
            ranks: ladder(id),
            rule_tables: BTreeMap::new(),
            drift: None,
            expel_standing_change: Faction::DEFAULT_EXPEL_STANDING_CHANGE,
        }
    }

    fn rung(id: &str, standing: Option<i64>, tolerance: Option<i64>) -> Rank {
        Rank {
            id: rank_id(id),
            requires_standing: standing.map(h),
            tolerance: tolerance.map(h),
        }
    }

    /// Riverhold's rank ladders (DESIGN.md §13).
    fn ladder(faction: &str) -> Vec<Rank> {
        match faction {
            "city_watch" => vec![
                rung("recruit", None, None),
                rung("sergeant", Some(30_00), None),
                rung("captain", Some(70_00), Some(25_00)),
            ],
            "lantern_guild" => vec![
                rung("cutpurse", None, None),
                rung("fence", Some(25_00), None),
                rung("shadow", Some(60_00), None),
            ],
            "temple" => vec![
                rung("acolyte", None, None),
                rung("ordained", Some(30_00), None),
                rung("high_priest", Some(80_00), Some(20_00)),
            ],
            _ => vec![
                rung("sellsword", None, None),
                rung("sergeant", Some(30_00), None),
            ],
        }
    }

    /// Riverhold's factions with their weights (DESIGN.md §13), plus one that has none.
    fn factions() -> BTreeMap<FactionId, Faction> {
        [
            faction("city_watch", 70_00, 20_00, Some((1_00, 25))),
            faction("lantern_guild", -60_00, -10_00, Some((1_00, 50))),
            faction("temple", 30_00, 80_00, Some((50, 1_00))),
            faction("free_company", -10_00, 0, None),
        ]
        .into_iter()
        .map(|f| (f.id.clone(), f))
        .collect()
    }

    fn action_id(text: &str) -> ActionId {
        ActionId::new(text).expect("a valid id")
    }

    fn action(id: &str, law: i64, good: i64) -> Action {
        Action {
            id: action_id(id),
            alignment: AlignmentDelta {
                law: h(law),
                good: h(good),
            },
            standing: ActionStanding::default(),
            by_target: BTreeMap::new(),
        }
    }

    fn named(factions: &[(&str, i64)], characters: &[(&str, i64)]) -> StandingEffects {
        StandingEffects {
            factions: factions
                .iter()
                .map(|&(f, v)| (faction_id(f), h(v)))
                .collect(),
            characters: characters.iter().map(|&(c, v)| (id(c), h(v))).collect(),
        }
    }

    /// Riverhold's `steal`, `help_stranger` and `donate_to_temple` with their standing
    /// effects (DESIGN.md §13).
    fn actions() -> BTreeMap<ActionId, Action> {
        let mut steal = action("steal", -5_00, -3_00);
        steal.standing = ActionStanding {
            target: Some(h(-20_00)),
            target_factions: Some(h(-10_00)),
            named: StandingEffects::default(),
        };
        let mut help = action("help_stranger", 0, 4_00);
        help.standing.target = Some(h(10_00));
        let mut donate = action("donate_to_temple", 0, 3_00);
        donate.standing.named = named(&[("temple", 10_00)], &[]);
        [steal, help, donate]
            .into_iter()
            .map(|a| (a.id.clone(), a))
            .collect()
    }

    /// A world with these characters and the test actions.
    fn world_of(characters: impl IntoIterator<Item = Character>) -> World {
        World::new(Content {
            balance: Balance::default(),
            characters: characters.into_iter().map(|c| (c.id.clone(), c)).collect(),
            factions: factions(),
            actions: actions(),
            relations: relations(),
            outcomes: BTreeMap::new(),
        })
        .expect("valid content")
    }

    fn between(a: &str, b: &str, value: i64) -> Relation {
        Relation {
            ends: RelationEnds::Between(faction_id(a), faction_id(b)),
            value: h(value),
        }
    }

    fn one_way(from: &str, to: &str, value: i64) -> Relation {
        Relation {
            ends: RelationEnds::Directed {
                from: faction_id(from),
                to: faction_id(to),
            },
            value: h(value),
        }
    }

    /// Riverhold's relations among the test factions (DESIGN.md §13).
    fn relations() -> Vec<Relation> {
        vec![
            between("city_watch", "lantern_guild", -80_00),
            between("city_watch", "temple", 60_00),
            between("lantern_guild", "free_company", 20_00),
            one_way("city_watch", "free_company", -30_00),
            one_way("free_company", "city_watch", -10_00),
        ]
    }

    fn riverhold() -> World {
        world_of([
            character("Vex", -55_00, -20_00),
            character("Ava", 20_00, 10_00),
            character("Player", 0, 0),
        ])
    }

    #[test]
    fn the_default_label_threshold_is_33() {
        assert_eq!(Balance::default().label_threshold, h(33_00));
    }

    #[test]
    fn looks_up_a_characters_alignment() {
        let world = riverhold();
        assert_eq!(
            world.alignment(&id("vex")),
            Some(Alignment::new(h(-55_00), h(-20_00)).expect("in range"))
        );
        assert_eq!(world.alignment(&id("nobody")), None);
        assert_eq!(
            world.character(&id("ava")).map(|c| c.name.as_str()),
            Some("Ava")
        );
    }

    #[test]
    fn lists_characters_in_id_order() {
        let world = riverhold();
        let ids: Vec<&str> = world.characters().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["ava", "player", "vex"]);
    }

    #[test]
    fn keeps_the_balance_it_was_given() {
        let balance = Balance {
            label_threshold: h(40_00),
            default_weights: weights(50, 1_00),
            metric: Metric::Chebyshev,
            affinity: Curve::constant(h(10_00)),
            bands: Bands::new(vec![Band {
                name: "indifferent".to_owned(),
                up_to: None,
            }])
            .expect("valid bands"),
            relation_bands: Bands::standard(),
            conflict_threshold: h(-60_00),
            disposition_weights: DispositionWeights {
                modifiers: h(0),
                ..DispositionWeights::default()
            },
            same_faction: h(40_00),
            rule_tables: [(TableKind::Defectors, sample_defectors())].into(),
            hysteresis: h(5_00),
            spillover: Curve::constant(h(25)),
            default_drift: DriftPolicy::Demote,
            conflict: ConflictRule::Auto,
            inertia: Inertia {
                default_profile: profile_id("hardening"),
                profiles: [(profile_id("hardening"), hardening())].into(),
            },
        };
        let world = World::new(Content {
            balance: balance.clone(),
            ..Content::default()
        })
        .expect("valid content");
        assert_eq!(world.balance(), &balance);
    }

    #[test]
    fn default_weights_are_even_and_the_default_metric_is_euclidean() {
        let balance = Balance::default();
        assert_eq!(balance.default_weights, Weights::EVEN);
        assert_eq!(balance.metric, Metric::Euclidean);
    }

    // Factions and content problems

    #[test]
    fn lists_factions_in_id_order() {
        let world = riverhold();
        let ids: Vec<&str> = world.factions().map(|f| f.id.as_str()).collect();
        assert_eq!(
            ids,
            ["city_watch", "free_company", "lantern_guild", "temple"]
        );
        let temple = world.faction(&faction_id("temple")).expect("exists");
        assert_eq!(temple.weights, Some(weights(50, 1_00)));
        assert_eq!(world.faction(&faction_id("nobody")), None);
    }

    #[test]
    fn a_faction_and_a_character_cannot_share_an_id() {
        let content = Content {
            characters: [
                character("Temple", 0, 0),
                character("Vex", 0, 0),
                character("City_watch", 0, 0),
            ]
            .into_iter()
            .map(|c| (c.id.clone(), c))
            .collect(),
            factions: factions(),
            ..Content::default()
        };
        let problems = vec![
            ContentProblem::SharedId(faction_id("city_watch")),
            ContentProblem::SharedId(faction_id("temple")),
        ];
        assert_eq!(content.problems(), problems);
        assert_eq!(World::new(content.clone()).err(), Some(problems.clone()));
        assert_eq!(World::replay(content, &[]).err(), Some(problems));
    }

    #[test]
    fn describes_content_problems() {
        assert_eq!(
            ContentProblem::SharedId(faction_id("vex")).to_string(),
            "'vex' is also a character's id: factions and characters need different ids"
        );
    }

    // Distance (DESIGN.md §6)

    fn as_faction(text: &str) -> Observer {
        Observer::Faction(faction_id(text))
    }

    fn as_character(text: &str) -> Observer {
        Observer::Character(id(text))
    }

    fn distance(world: &World, observer: &Observer, subject: &str) -> Fixed {
        world
            .distance(observer, &id(subject))
            .expect("both exist")
            .value
    }

    #[test]
    fn a_faction_measures_distance_with_its_own_weights() {
        let world = riverhold();
        let measured = world
            .distance(&as_faction("city_watch"), &id("player"))
            .expect("both exist");
        assert_eq!(
            measured,
            Distance {
                value: h(70_18),
                metric: Metric::Euclidean,
                observer: aligned(70_00, 20_00),
                subject: aligned(0, 0),
                weights: weights(1_00, 25),
                weights_from: WeightsFrom::Own,
            }
        );
        assert_eq!(
            (measured.gap(Axis::Law), measured.gap(Axis::Good)),
            (h(70_00), h(20_00))
        );
        assert_eq!(distance(&world, &as_faction("temple"), "vex"), {
            // Gaps 85 and 100, weighted 42.5 and 100: sqrt(1806.25 + 10000) = 108.656…
            h(108_66)
        });
    }

    #[test]
    fn gaps_are_how_far_apart_the_two_are_whichever_side_is_higher() {
        let world = riverhold();
        let measured = world
            .distance(&as_faction("lantern_guild"), &id("ava"))
            .expect("both exist");
        assert_eq!(
            (measured.gap(Axis::Law), measured.gap(Axis::Good)),
            (h(80_00), h(20_00))
        );
    }

    #[test]
    fn an_observer_without_weights_uses_the_default() {
        let world = riverhold();
        let measured = world
            .distance(&as_faction("free_company"), &id("player"))
            .expect("both exist");
        assert_eq!(measured.weights, Weights::EVEN);
        assert_eq!(measured.weights_from, WeightsFrom::Default);
        assert_eq!(measured.value, h(10_00));
        // Ava (20 / 10, default weights) sees the player (0 / 0): sqrt(400 + 100) = 22.360…
        assert_eq!(distance(&world, &as_character("ava"), "player"), h(22_36));
    }

    #[test]
    fn a_character_measures_distance_with_their_own_weights() {
        let mut hale = character("Captain_hale", 75_00, 30_00);
        hale.weights = Some(weights(1_00, 25));
        let world = world_of([hale, character("Player", 0, 0)]);
        let measured = world
            .distance(&as_character("captain_hale"), &id("player"))
            .expect("both exist");
        // Gaps 75 and 30, weighted 75 and 7.5: sqrt(5681.25) = 75.374…
        assert_eq!(measured.value, h(75_37));
        assert_eq!(measured.weights_from, WeightsFrom::Own);
    }

    #[test]
    fn distance_follows_alignment_as_it_moves() {
        let mut world = riverhold();
        let guild = as_faction("lantern_guild");
        for _ in 0..2 {
            world
                .execute(act("player", "steal", None, 1_00))
                .expect("accepted");
        }
        assert_eq!(distance(&world, &guild, "player"), h(50_04));
        for _ in 0..2 {
            world
                .execute(act("player", "steal", None, 1_00))
                .expect("accepted");
        }
        assert_eq!(distance(&world, &guild, "player"), h(40_01));
        let observed = world
            .distance(&as_character("player"), &id("vex"))
            .expect("both exist");
        assert_eq!(observed.observer, aligned(-20_00, -12_00), "the player now");
    }

    #[test]
    fn the_worlds_metric_applies_to_every_measurement() {
        let content = |metric| Content {
            balance: Balance {
                metric,
                ..Balance::default()
            },
            characters: [character("Player", 0, 0)]
                .into_iter()
                .map(|c| (c.id.clone(), c))
                .collect(),
            factions: factions(),
            ..Content::default()
        };
        for (metric, expected) in [
            (Metric::Euclidean, h(70_18)),
            (Metric::Manhattan, h(75_00)),
            (Metric::Chebyshev, h(70_00)),
        ] {
            let world = World::new(content(metric)).expect("valid content");
            let measured = world
                .distance(&as_faction("city_watch"), &id("player"))
                .expect("both exist");
            assert_eq!((measured.value, measured.metric), (expected, metric));
        }
    }

    #[test]
    fn the_default_weights_are_a_setting() {
        let content = Content {
            balance: Balance {
                default_weights: weights(50, 1_00),
                ..Balance::default()
            },
            characters: [character("Player", 0, 0)]
                .into_iter()
                .map(|c| (c.id.clone(), c))
                .collect(),
            factions: factions(),
            ..Content::default()
        };
        let world = World::new(content).expect("valid content");
        // free_company (−10 / 0) has no weights of its own: gap 10 × 0.50 = 5.00.
        assert_eq!(
            distance(&world, &as_faction("free_company"), "player"),
            h(5_00)
        );
    }

    // Disposition (DESIGN.md §8.1): affinity only, until M4

    /// Riverhold's people that the disposition examples use.
    fn riverhold_people() -> Vec<Character> {
        vec![
            character("Player", 0, 0),
            character("Vex", -55_00, -20_00),
            character("Sister_mira", 35_00, 85_00),
            character("Brother_ash", 25_00, -70_00),
        ]
    }

    fn regard(world: &World, observer: &Observer, subject: &str) -> (Fixed, String) {
        let disposition = world
            .disposition(observer, &id(subject))
            .expect("both exist");
        (disposition.score, disposition.band)
    }

    fn scored(score: i64, band: &str) -> (Fixed, String) {
        (h(score), band.to_owned())
    }

    #[test]
    fn the_watch_is_neutral_toward_a_neutral_player() {
        let world = world_of(riverhold_people());
        let disposition = world
            .disposition(&as_faction("city_watch"), &id("player"))
            .expect("both exist");
        // Distance 70.18 is on the 60 → 200 segment: −50 × 10.18 / 140 = −3.636…
        // −3.64, written ungrouped: clippy reads `_64` as an `i64` suffix.
        assert_eq!(
            (disposition.score, disposition.band.as_str()),
            (h(-364), "neutral")
        );
        assert_eq!(
            disposition.component(ComponentKind::Affinity).value,
            h(-364)
        );
        assert_eq!(
            disposition.distance,
            world
                .distance(&as_faction("city_watch"), &id("player"))
                .expect("both exist")
        );
    }

    // The full disposition (DESIGN.md §8.1, §8.2)

    fn component(
        world: &World,
        observer: &Observer,
        subject: &str,
        kind: ComponentKind,
    ) -> Component {
        world
            .disposition(observer, &id(subject))
            .expect("both exist")
            .component(kind)
            .clone()
    }

    fn weighted(
        kind: ComponentKind,
        value: i64,
        weight: i64,
        weighted: i64,
        parts: Vec<Part>,
    ) -> Component {
        Component {
            kind,
            value: h(value),
            weight: h(weight),
            weighted: h(weighted),
            parts,
            modifiers: Vec::new(),
        }
    }

    fn part(from: &str, to: Option<&str>, value: i64) -> Part {
        Part {
            from: faction_id(from),
            to: to.map(faction_id),
            value: h(value),
        }
    }

    #[test]
    fn hale_thinks_ill_of_a_fined_thief() {
        // DESIGN.md §8.2: two thefts from a merchant, then a fine from the Watch.
        let mut world = with_outcomes();
        for _ in 0..2 {
            world
                .execute(act("player", "steal", Some("ava"), 1_00))
                .expect("accepted");
        }
        world
            .execute(outcome("fined_by_watch", "player"))
            .expect("accepted");
        let disposition = world
            .disposition(&as_character("captain_hale"), &id("player"))
            .expect("both exist");
        assert_eq!(disposition.distance.value, h(85_48));
        assert_eq!(
            disposition.components,
            [
                // Distance 85.48: −50 × 25.48 / 140 = −9.10
                weighted(ComponentKind::Affinity, -9_10, 1_00, -9_10, vec![]),
                weighted(ComponentKind::Standing, -10_00, 1_00, -10_00, vec![]),
                weighted(ComponentKind::Kinship, 0, 50, 0, vec![]),
                weighted(
                    ComponentKind::FactionOpinion,
                    -20_00,
                    50,
                    -10_00,
                    vec![part("city_watch", None, -20_00)]
                ),
                weighted(ComponentKind::Modifiers, 0, 1_00, 0, vec![]),
            ]
        );
        assert_eq!(
            (disposition.score, disposition.band.as_str()),
            (h(-29_10), "unfriendly")
        );
    }

    #[test]
    fn the_watch_distrusts_a_member_of_its_enemy() {
        let world = world_of(riverhold_people_in_factions());
        let disposition = world
            .disposition(&as_faction("city_watch"), &id("vex"))
            .expect("both exist");
        assert_eq!(
            disposition.component(ComponentKind::Kinship),
            &weighted(
                ComponentKind::Kinship,
                -80_00,
                50,
                -40_00,
                vec![part("city_watch", Some("lantern_guild"), -80_00)]
            )
        );
        assert_eq!(
            disposition.component(ComponentKind::FactionOpinion),
            &weighted(ComponentKind::FactionOpinion, 0, 50, 0, vec![]),
            "a faction has no faction opinion"
        );
        assert_eq!(
            (disposition.score, disposition.band.as_str()),
            (h(-63_36), "unfriendly")
        );
    }

    /// Riverhold's people in their factions, with Hale's own weights.
    fn riverhold_people_in_factions() -> Vec<Character> {
        let mut hale = member_of(character("Captain_hale", 75_00, 30_00), &["city_watch"]);
        hale.weights = Some(weights(1_00, 25));
        vec![
            hale,
            character("Player", 0, 0),
            member_of(character("Vex", -55_00, -20_00), &["lantern_guild"]),
            member_of(character("Sister_mira", 35_00, 85_00), &["temple"]),
            member_of(character("Recruit", 70_00, 20_00), &["city_watch"]),
        ]
    }

    #[test]
    fn hale_warms_to_a_priestess_of_an_allied_faith() {
        let world = world_of(riverhold_people_in_factions());
        let observer = as_character("captain_hale");
        // Gaps 40 and 55, weighted 40 and 13.75: sqrt(1789.0625) = 42.297…; 50 − 50 × 42.30 / 60
        assert_eq!(
            component(&world, &observer, "sister_mira", ComponentKind::Affinity).value,
            h(14_75)
        );
        assert_eq!(
            component(&world, &observer, "sister_mira", ComponentKind::Kinship),
            weighted(
                ComponentKind::Kinship,
                60_00,
                50,
                30_00,
                vec![part("city_watch", Some("temple"), 60_00)]
            )
        );
        let disposition = world
            .disposition(&observer, &id("sister_mira"))
            .expect("both exist");
        assert_eq!(
            (disposition.score, disposition.band.as_str()),
            (h(44_75), "friendly")
        );
    }

    #[test]
    fn members_of_the_same_faction_count_it_as_kin() {
        let world = world_of(riverhold_people_in_factions());
        assert_eq!(
            component(
                &world,
                &as_character("captain_hale"),
                "recruit",
                ComponentKind::Kinship
            ),
            weighted(
                ComponentKind::Kinship,
                50_00,
                50,
                25_00,
                vec![part("city_watch", Some("city_watch"), 50_00)]
            )
        );
        assert_eq!(
            component(
                &world,
                &as_faction("city_watch"),
                "recruit",
                ComponentKind::Kinship
            )
            .value,
            h(50_00),
            "a faction is kin to its own members"
        );
    }

    #[test]
    fn components_and_the_score_are_clamped() {
        let mut ash = member_of(
            character("Ash", 100_00, -100_00),
            &["lantern_guild", "free_company"],
        );
        ash.standing = named(&[("temple", -100_00)], &[]);
        let content = Content {
            characters: [(ash.id.clone(), ash)].into(),
            factions: factions(),
            relations: vec![
                between("temple", "lantern_guild", -80_00),
                between("temple", "free_company", -80_00),
            ],
            ..Content::default()
        };
        let world = World::new(content).expect("valid content");
        let disposition = world
            .disposition(&as_faction("temple"), &id("ash"))
            .expect("both exist");
        let kinship = disposition.component(ComponentKind::Kinship);
        assert_eq!(
            (kinship.value, kinship.weighted),
            (h(-100_00), h(-50_00)),
            "-160 clamps"
        );
        assert_eq!(kinship.parts.len(), 2);
        assert_eq!(disposition.score, h(-100_00));
    }

    #[test]
    fn the_component_weights_and_same_faction_are_settings() {
        let content = |weights: DispositionWeights, same_faction: i64| Content {
            balance: Balance {
                disposition_weights: weights,
                same_faction: h(same_faction),
                ..Balance::default()
            },
            characters: riverhold_people_in_factions()
                .into_iter()
                .map(|c| (c.id.clone(), c))
                .collect(),
            factions: factions(),
            relations: relations(),
            ..Content::default()
        };
        let no_kinship = DispositionWeights {
            kinship: h(0),
            ..DispositionWeights::default()
        };
        let world = World::new(content(no_kinship, 50_00)).expect("valid content");
        assert_eq!(
            world
                .disposition(&as_faction("city_watch"), &id("vex"))
                .expect("both exist")
                .score,
            h(-23_36)
        );
        let world =
            World::new(content(DispositionWeights::default(), 20_00)).expect("valid content");
        assert_eq!(
            component(
                &world,
                &as_faction("city_watch"),
                "recruit",
                ComponentKind::Kinship
            )
            .value,
            h(20_00)
        );
    }

    #[test]
    fn disposition_weights_cannot_be_negative() {
        let content = Content {
            balance: Balance {
                disposition_weights: DispositionWeights {
                    kinship: h(-1),
                    ..DispositionWeights::default()
                },
                same_faction: h(100_01),
                ..Balance::default()
            },
            ..Content::default()
        };
        let problems = content.problems();
        assert_eq!(
            problems,
            [
                ContentProblem::NegativeDispositionWeight {
                    component: ComponentKind::Kinship,
                    value: h(-1),
                },
                ContentProblem::SameFactionOutOfRange(h(100_01)),
            ]
        );
        assert_eq!(problems[0].to_string(), "-0.01 must be at least 0.00");
        assert_eq!(problems[1].to_string(), "100.01 is outside -100.00..100.00");
        assert_eq!(
            ComponentKind::ALL.map(ComponentKind::key),
            [
                "affinity",
                "standing",
                "kinship",
                "faction_opinion",
                "modifiers"
            ]
        );
    }

    #[test]
    fn close_alignments_are_friendly_and_distant_ones_unfriendly() {
        let world = world_of(riverhold_people());
        let temple = as_faction("temple");
        // Distance 5.59: 50 − 50 × 5.59 / 60 = 45.341…
        assert_eq!(
            regard(&world, &temple, "sister_mira"),
            scored(45_34, "friendly")
        );
        // Distance 150.02: −50 × 90.02 / 140 = −32.15
        assert_eq!(
            regard(&world, &temple, "brother_ash"),
            scored(-32_15, "unfriendly")
        );
        // Distance 125.40: −50 × 65.40 / 140 = −23.357…
        assert_eq!(
            regard(&world, &as_faction("city_watch"), "vex"),
            scored(-23_36, "neutral")
        );
    }

    #[test]
    fn a_character_regards_others_by_their_own_lights() {
        let world = world_of(riverhold_people());
        // Vex sees the player at sqrt(55² + 20²) = 58.52: 50 − 50 × 58.52 / 60 = 1.233…
        assert_eq!(
            regard(&world, &as_character("vex"), "player"),
            scored(1_23, "neutral")
        );
    }

    fn balanced(balance: Balance) -> World {
        World::new(Content {
            balance,
            characters: riverhold_people()
                .into_iter()
                .map(|c| (c.id.clone(), c))
                .collect(),
            factions: factions(),
            ..Content::default()
        })
        .expect("valid content")
    }

    #[test]
    fn the_bands_are_a_setting() {
        let band = |name: &str, up_to: Option<i64>| Band {
            name: name.to_owned(),
            up_to: up_to.map(h),
        };
        let world = balanced(Balance {
            bands: Bands::new(vec![
                band("hostile", Some(-30_00)),
                band("unfriendly", Some(-25_00)),
                band("neutral", Some(25_00)),
                band("friendly", None),
            ])
            .expect("valid bands"),
            ..Balance::default()
        });
        assert_eq!(
            regard(&world, &as_faction("temple"), "brother_ash"),
            scored(-32_15, "hostile")
        );
    }

    #[test]
    fn the_affinity_curve_is_a_setting() {
        let curve = Curve::from_points(vec![(h(0), h(100_00)), (h(100_00), h(-100_00))])
            .expect("valid curve");
        let world = balanced(Balance {
            affinity: curve,
            ..Balance::default()
        });
        // 100 − 200 × 70.18 / 100 = −40.36
        assert_eq!(
            regard(&world, &as_faction("city_watch"), "player"),
            scored(-40_36, "unfriendly")
        );
    }

    #[test]
    fn an_affinity_beyond_the_range_of_a_disposition_is_refused() {
        let curve =
            Curve::from_points(vec![(h(0), h(150_00)), (h(100_00), h(0))]).expect("valid curve");
        let content = Content {
            balance: Balance {
                affinity: curve,
                ..Balance::default()
            },
            ..Content::default()
        };
        let problems = content.problems();
        assert_eq!(problems.len(), 1);
        assert_eq!(
            problems[0].to_string(),
            "curve value 150.00 is outside -100.00 to 100.00"
        );
        assert!(World::new(content).is_err());
        let at_the_edges = Content {
            balance: Balance {
                affinity: Curve::from_points(vec![(h(0), h(100_00)), (h(1_00), h(-100_00))])
                    .expect("valid curve"),
                ..Balance::default()
            },
            ..Content::default()
        };
        assert_eq!(at_the_edges.problems(), []);
    }

    #[test]
    fn disposition_follows_alignment_as_it_moves() {
        let mut world = world_of(riverhold_people());
        for _ in 0..4 {
            world
                .execute(act("player", "steal", None, 1_00))
                .expect("accepted");
        }
        // The guild sees the player at −20 / −12 from 40.01 away: 50 − 50 × 40.01 / 60 = 16.658…
        assert_eq!(
            regard(&world, &as_faction("lantern_guild"), "player"),
            scored(16_66, "neutral")
        );
    }

    #[test]
    fn disposition_needs_an_observer_and_a_subject_that_exist() {
        let world = riverhold();
        assert_eq!(
            world.disposition(&as_faction("city_wach"), &id("player")),
            None
        );
        assert_eq!(
            world.disposition(&as_faction("city_watch"), &id("nobody")),
            None
        );
    }

    // Membership (DESIGN.md §9.1)

    fn member_of(mut character: Character, factions: &[&str]) -> Character {
        character.memberships = factions
            .iter()
            .map(|f| StartingMembership {
                faction: faction_id(f),
                rank: None,
            })
            .collect();
        character
    }

    /// A character starting in `faction` as `rank`.
    fn ranked(mut character: Character, faction: &str, rank: &str) -> Character {
        character.memberships = vec![StartingMembership {
            faction: faction_id(faction),
            rank: Some(rank_id(rank)),
        }];
        character
    }

    fn rank_id(text: &str) -> RankId {
        RankId::new(text).expect("a valid id")
    }

    fn join(character: &str, faction: &str) -> Command {
        Command::JoinFaction {
            character: id(character),
            faction: faction_id(faction),
        }
    }

    fn leave(character: &str, faction: &str) -> Command {
        Command::LeaveFaction {
            character: id(character),
            faction: faction_id(faction),
        }
    }

    fn factions_of(world: &World, character: &str) -> Vec<(String, Tick)> {
        world
            .memberships(&id(character))
            .expect("the character exists")
            .map(|(faction, membership)| (faction.to_string(), membership.since))
            .collect()
    }

    fn steal_times(world: &mut World, times: usize) {
        for _ in 0..times {
            world
                .execute(act("player", "steal", None, 1_00))
                .expect("accepted");
        }
    }

    #[test]
    fn starting_members_belong_from_tick_zero() {
        let world = world_of([
            member_of(character("Vex", -55_00, -20_00), &["lantern_guild"]),
            member_of(character("Ava", 20_00, 10_00), &["temple", "city_watch"]),
            character("Player", 0, 0),
        ]);
        assert_eq!(
            factions_of(&world, "vex"),
            [("lantern_guild".to_owned(), Tick(0))]
        );
        assert_eq!(
            factions_of(&world, "ava"),
            [
                ("city_watch".to_owned(), Tick(0)),
                ("temple".to_owned(), Tick(0))
            ]
        );
        assert!(factions_of(&world, "player").is_empty());
        assert!(world.memberships(&id("nobody")).is_none());
        let members = |faction| {
            world
                .members(&faction_id(faction))
                .expect("the faction exists")
                .into_iter()
                .map(CharacterId::to_string)
                .collect::<Vec<_>>()
        };
        assert_eq!(members("lantern_guild"), ["vex"]);
        assert_eq!(members("temple"), ["ava"]);
        assert!(members("free_company").is_empty());
        assert_eq!(world.members(&faction_id("nobody")), None);
    }

    #[test]
    fn a_character_too_far_from_a_faction_cannot_join_it() {
        let world = riverhold();
        let assessment = world
            .assess_join(&id("player"), &faction_id("lantern_guild"))
            .expect("both exist");
        // Gaps 60 and 10, weighted 60 and 5: sqrt(3625) = 60.207…
        assert_eq!(assessment.distance.value, h(60_21));
        assert_eq!(assessment.tolerance, h(45_00));
        assert_eq!(
            assessment.blocks,
            [JoinBlock::OutsideTolerance {
                distance: h(60_21),
                tolerance: h(45_00)
            }]
        );
        assert!(!assessment.allowed());
        assert_eq!(
            assessment.reasons(),
            ["60.21 from The Lantern Guild, tolerance is 45.00"]
        );
    }

    #[test]
    fn a_thief_drifts_into_the_lantern_guilds_reach() {
        let mut world = riverhold();
        let guild = faction_id("lantern_guild");
        steal_times(&mut world, 2);
        let refused = world
            .assess_join(&id("player"), &guild)
            .expect("both exist");
        assert_eq!(
            refused.reasons(),
            ["50.04 from The Lantern Guild, tolerance is 45.00"]
        );
        steal_times(&mut world, 2);
        let accepted = world
            .assess_join(&id("player"), &guild)
            .expect("both exist");
        assert_eq!(accepted.distance.value, h(40_01));
        assert!(accepted.allowed());
        assert!(accepted.reasons().is_empty());
    }

    #[test]
    fn joining_makes_a_character_a_member_from_that_tick() {
        let mut world = riverhold();
        steal_times(&mut world, 4);
        world.execute(advance(3)).expect("accepted");
        let events = world
            .execute(join("player", "lantern_guild"))
            .expect("accepted");
        assert_eq!(
            events,
            [Event {
                // Each theft is two events, and advancing time one.
                seq: 10,
                tick: Tick(3),
                payload: Change::JoinedFaction {
                    character: id("player"),
                    faction: faction_id("lantern_guild"),
                    rank: rank_id("cutpurse"),
                },
            }]
        );
        assert_eq!(
            factions_of(&world, "player"),
            [("lantern_guild".to_owned(), Tick(3))]
        );
        assert_eq!(
            world.members(&faction_id("lantern_guild")),
            Some(vec![&id("player")])
        );
    }

    #[test]
    fn a_refused_join_says_why_and_changes_nothing() {
        let mut world = riverhold();
        let error = refused(&mut world, join("player", "lantern_guild"));
        assert_eq!(
            error.to_string(),
            "player can't join lantern_guild: 60.21 from The Lantern Guild, tolerance is 45.00"
        );
        let CommandError::JoinRefused(assessment) = error else {
            panic!("expected a refused join");
        };
        assert_eq!(
            Some(*assessment),
            world.assess_join(&id("player"), &faction_id("lantern_guild"))
        );
    }

    #[test]
    fn a_member_cannot_join_again() {
        let mut world = world_of([member_of(
            character("Vex", -55_00, -20_00),
            &["lantern_guild"],
        )]);
        assert_eq!(
            refused(&mut world, join("vex", "lantern_guild")).to_string(),
            "vex can't join lantern_guild: vex is already a member of lantern_guild"
        );
        // Every failing check is listed, in order.
        let mut far = world_of([member_of(
            character("Player", 100_00, 100_00),
            &["lantern_guild"],
        )]);
        let error = refused(&mut far, join("player", "lantern_guild"));
        let CommandError::JoinRefused(assessment) = error else {
            panic!("expected a refused join");
        };
        assert_eq!(assessment.blocks.len(), 2);
        assert_eq!(assessment.blocks[0], JoinBlock::AlreadyMember);
    }

    #[test]
    fn a_character_exactly_at_the_tolerance_may_join() {
        let edge = |tolerance| {
            let mut faction = faction("free_company", 10_00, 0, None);
            faction.tolerances = Tolerances::new(h(tolerance), None).expect("valid");
            let content = Content {
                characters: [character("Player", 0, 0)]
                    .into_iter()
                    .map(|c| (c.id.clone(), c))
                    .collect(),
                factions: [(faction.id.clone(), faction)].into(),
                ..Content::default()
            };
            World::new(content)
                .expect("valid content")
                .assess_join(&id("player"), &faction_id("free_company"))
                .expect("both exist")
                .allowed()
        };
        assert!(edge(10_00), "10.00 away, tolerance 10.00");
        assert!(!edge(9_99), "10.00 away, tolerance 9.99");
    }

    #[test]
    fn leaving_ends_a_membership() {
        let mut world = world_of([member_of(
            character("Vex", -55_00, -20_00),
            &["lantern_guild"],
        )]);
        assert_eq!(
            world.execute(leave("vex", "lantern_guild")),
            Ok(vec![Event {
                seq: 1,
                tick: Tick(0),
                payload: Change::LeftFaction {
                    character: id("vex"),
                    faction: faction_id("lantern_guild"),
                    reason: LeaveReason::Voluntary,
                },
            }])
        );
        assert!(factions_of(&world, "vex").is_empty());
        assert_eq!(
            refused(&mut world, leave("vex", "lantern_guild")),
            CommandError::NotAMember {
                character: id("vex"),
                faction: faction_id("lantern_guild"),
            }
        );
        assert_eq!(
            refused(&mut world, leave("vex", "lantern_guild")).to_string(),
            "vex isn't a member of lantern_guild"
        );
    }

    #[test]
    fn joining_and_leaving_need_a_character_and_faction_that_exist() {
        let mut world = riverhold();
        for command in [join, leave] {
            assert_eq!(
                refused(&mut world, command("plyer", "temple")),
                CommandError::UnknownCharacter {
                    role: Role::Member,
                    id: id("plyer"),
                    suggestion: Some(id("player")),
                }
            );
            assert_eq!(
                refused(&mut world, command("player", "tempel")),
                CommandError::UnknownFaction {
                    faction: faction_id("tempel"),
                    suggestion: Some(faction_id("temple")),
                }
            );
        }
        assert_eq!(
            refused(&mut world, join("player", "tempel")).to_string(),
            "unknown faction 'tempel' (did you mean 'temple'?)"
        );
        assert_eq!(
            refused(&mut world, join("plyer", "temple")).to_string(),
            "unknown character 'plyer' (did you mean 'player'?)"
        );
        assert_eq!(
            world.assess_join(&id("nobody"), &faction_id("temple")),
            None
        );
        assert_eq!(
            world.assess_join(&id("player"), &faction_id("nobody")),
            None
        );
    }

    #[test]
    fn replaying_rebuilds_memberships() {
        let mut world = world_of([
            member_of(character("Vex", -55_00, -20_00), &["lantern_guild"]),
            character("Player", 0, 0),
        ]);
        world
            .execute(leave("vex", "lantern_guild"))
            .expect("accepted");
        world.execute(advance(2)).expect("accepted");
        world
            .execute(join("player", "free_company"))
            .expect("accepted");
        let replayed = World::replay(world.content.clone(), world.events()).expect("valid content");
        assert_eq!(replayed.state, world.state);
        assert_eq!(
            factions_of(&replayed, "player"),
            [("free_company".to_owned(), Tick(2))]
        );
    }

    // Membership in content (P-32)

    #[test]
    fn a_membership_must_name_a_faction_that_exists_once() {
        let content = Content {
            characters: [
                member_of(
                    character("Vex", 0, 0),
                    &["lantern_gild", "temple", "temple"],
                ),
                member_of(character("Ava", 0, 0), &["nowhere"]),
            ]
            .into_iter()
            .map(|c| (c.id.clone(), c))
            .collect(),
            factions: factions(),
            ..Content::default()
        };
        let problems = content.problems();
        assert_eq!(
            problems,
            [
                ContentProblem::UnknownMembershipFaction {
                    character: id("ava"),
                    index: 0,
                    faction: faction_id("nowhere"),
                    suggestion: None,
                },
                ContentProblem::UnknownMembershipFaction {
                    character: id("vex"),
                    index: 0,
                    faction: faction_id("lantern_gild"),
                    suggestion: Some(faction_id("lantern_guild")),
                },
                ContentProblem::DuplicateMembership {
                    character: id("vex"),
                    index: 2,
                    faction: faction_id("temple"),
                },
            ]
        );
        let messages: Vec<String> = problems.iter().map(ToString::to_string).collect();
        assert_eq!(
            messages,
            [
                "unknown faction 'nowhere'",
                "unknown faction 'lantern_gild' (did you mean 'lantern_guild'?)",
                "vex already belongs to temple",
            ]
        );
        assert!(World::new(content).is_err());
    }

    #[test]
    fn a_starting_member_outside_member_tolerance_is_a_warning() {
        let content = |law, good| Content {
            characters: [member_of(character("Vex", law, good), &["lantern_guild"])]
                .into_iter()
                .map(|c| (c.id.clone(), c))
                .collect(),
            factions: factions(),
            ..Content::default()
        };
        // Vex is alone, so some factions have no one within tolerance; that's another warning.
        let outside = |content: &Content| -> Vec<ContentWarning> {
            content
                .warnings()
                .into_iter()
                .filter(|warning| matches!(warning, ContentWarning::OutsideMemberTolerance { .. }))
                .collect()
        };
        // A reformed Vex at 35 / 10 is 95.52 from the guild, beyond its 60.00.
        let reformed = content(35_00, 10_00);
        let warnings = outside(&reformed);
        assert_eq!(
            warnings,
            [ContentWarning::OutsideMemberTolerance {
                character: id("vex"),
                index: 0,
                faction_name: "The Lantern Guild".to_owned(),
                distance: h(95_52),
                member_tolerance: h(60_00),
            }]
        );
        assert_eq!(
            warnings[0].to_string(),
            "vex starts 95.52 from The Lantern Guild, outside its member tolerance of 60.00"
        );
        assert!(
            World::new(reformed).is_ok(),
            "a warning doesn't stop the world"
        );
        assert_eq!(outside(&content(-55_00, -20_00)), [], "7.07 away");
        // Exactly 60.00 away on the law axis is still within.
        assert_eq!(outside(&content(0, -10_00)), []);
        assert_eq!(outside(&content(1, -10_00)).len(), 1);
    }

    // Relations (DESIGN.md §9.4)

    fn relation_of(world: &World, from: &str, to: &str) -> (Fixed, String) {
        let regard = world
            .relation(&faction_id(from), &faction_id(to))
            .expect("both exist");
        (regard.value, regard.band)
    }

    fn conflict(world: &World, a: &str, b: &str) -> bool {
        world
            .in_conflict(&faction_id(a), &faction_id(b))
            .expect("both exist")
    }

    fn set(from: &str, to: &str, value: i64, mutual: bool) -> Command {
        Command::SetRelation {
            from: faction_id(from),
            to: faction_id(to),
            value: h(value),
            mutual,
        }
    }

    fn shift(from: &str, to: &str, by: i64, mutual: bool) -> Command {
        Command::ShiftRelation {
            from: faction_id(from),
            to: faction_id(to),
            by: h(by),
            mutual,
        }
    }

    fn changed(seq: u64, from: &str, to: &str, before: i64, after: i64) -> Event {
        Event {
            seq,
            tick: Tick(0),
            payload: Change::RelationChanged {
                from: faction_id(from),
                to: faction_id(to),
                before: h(before),
                after: h(after),
            },
        }
    }

    #[test]
    fn relations_have_a_value_and_a_band_in_each_direction() {
        let world = riverhold();
        assert_eq!(
            relation_of(&world, "city_watch", "lantern_guild"),
            (h(-80_00), "enemy".into())
        );
        assert_eq!(
            relation_of(&world, "lantern_guild", "city_watch"),
            (h(-80_00), "enemy".into())
        );
        assert_eq!(
            relation_of(&world, "city_watch", "free_company"),
            (h(-30_00), "rival".into())
        );
        assert_eq!(
            relation_of(&world, "free_company", "city_watch"),
            (h(-10_00), "neutral".into())
        );
        assert_eq!(
            relation_of(&world, "city_watch", "temple"),
            (h(60_00), "allied".into())
        );
        assert_eq!(
            relation_of(&world, "temple", "lantern_guild"),
            (h(0), "neutral".into()),
            "unwritten"
        );
        assert_eq!(
            world.relation(&faction_id("nowhere"), &faction_id("temple")),
            None
        );
        assert_eq!(
            world.relation(&faction_id("temple"), &faction_id("nowhere")),
            None
        );
    }

    #[test]
    fn factions_are_in_conflict_when_either_regards_the_other_as_an_enemy() {
        let world = riverhold();
        assert!(conflict(&world, "city_watch", "lantern_guild"));
        assert!(conflict(&world, "lantern_guild", "city_watch"));
        assert!(
            !conflict(&world, "city_watch", "free_company"),
            "-30 and -10"
        );
        assert!(!conflict(&world, "lantern_guild", "free_company"));
        assert_eq!(
            world.in_conflict(&faction_id("nowhere"), &faction_id("temple")),
            None
        );
        assert_eq!(
            world.in_conflict(&faction_id("temple"), &faction_id("nowhere")),
            None
        );
        // One side at exactly the threshold is enough.
        let content = Content {
            factions: factions(),
            relations: vec![one_way("temple", "free_company", -50_00)],
            ..Content::default()
        };
        let edge = World::new(content).expect("valid content");
        assert!(conflict(&edge, "free_company", "temple"));
        let content = Content {
            factions: factions(),
            relations: vec![one_way("temple", "free_company", -49_99)],
            ..Content::default()
        };
        assert!(!conflict(
            &World::new(content).expect("valid content"),
            "free_company",
            "temple"
        ));
    }

    #[test]
    fn the_conflict_threshold_is_a_setting() {
        let content = Content {
            balance: Balance {
                conflict_threshold: h(-30_00),
                ..Balance::default()
            },
            factions: factions(),
            relations: relations(),
            ..Content::default()
        };
        let world = World::new(content).expect("valid content");
        assert!(
            conflict(&world, "city_watch", "free_company"),
            "-30 is now enough"
        );
    }

    #[test]
    fn lists_every_relation_set_in_order() {
        let world = riverhold();
        let listed: Vec<(String, String, Fixed)> = world
            .relations()
            .into_iter()
            .map(|r| (r.from.to_string(), r.to.to_string(), r.value))
            .collect();
        let expected = [
            ("city_watch", "free_company", -30_00),
            ("city_watch", "lantern_guild", -80_00),
            ("city_watch", "temple", 60_00),
            ("free_company", "city_watch", -10_00),
            ("free_company", "lantern_guild", 20_00),
            ("lantern_guild", "city_watch", -80_00),
            ("lantern_guild", "free_company", 20_00),
            ("temple", "city_watch", 60_00),
        ]
        .map(|(from, to, value)| (from.to_owned(), to.to_owned(), h(value)));
        assert_eq!(listed, expected);
    }

    #[test]
    fn a_member_of_an_enemy_faction_cannot_join() {
        let mut world = riverhold();
        steal_times(&mut world, 4);
        world
            .execute(join("player", "lantern_guild"))
            .expect("accepted");
        let assessment = world
            .assess_join(&id("player"), &faction_id("city_watch"))
            .expect("both exist");
        assert_eq!(
            assessment.blocks,
            [
                // Gaps 90 and 32, weighted 90 and 8: sqrt(8164) = 90.354…
                JoinBlock::OutsideTolerance {
                    distance: h(90_35),
                    tolerance: h(40_00)
                },
            ]
        );
        // With no tables in content, the built-in ones decide: D-4, enemies exclude each other.
        let [defection] = &assessment.defections[..] else {
            panic!("one enemy membership: {:?}", assessment.defections);
        };
        assert_eq!(
            (&defection.from, defection.relation),
            (&faction_id("lantern_guild"), h(-80_00))
        );
        assert_eq!(
            (defection.deserters.source, defection.deserters.allows()),
            (TableSource::BuiltIn, true)
        );
        assert_eq!(
            (defection.defectors.source, defection.defectors.allows()),
            (TableSource::BuiltIn, false)
        );
        assert_eq!(
            assessment.reasons(),
            [
                "90.35 from The City Watch, tolerance is 40.00",
                "player belongs to The Lantern Guild, in conflict with The City Watch (-80.00): refused by the built-in defectors rule",
            ]
        );
    }

    #[test]
    fn enemy_exclusion_names_the_more_hostile_direction() {
        let content = Content {
            characters: [member_of(character("Ava", 70_00, 20_00), &["free_company"])]
                .into_iter()
                .map(|c| (c.id.clone(), c))
                .collect(),
            factions: factions(),
            relations: vec![
                one_way("free_company", "city_watch", -60_00),
                one_way("city_watch", "free_company", -20_00),
            ],
            ..Content::default()
        };
        let world = World::new(content).expect("valid content");
        let assessment = world
            .assess_join(&id("ava"), &faction_id("city_watch"))
            .expect("both exist");
        let enemies: Vec<(&FactionId, Fixed)> = assessment
            .defections
            .iter()
            .map(|defection| (&defection.from, defection.relation))
            .collect();
        assert_eq!(enemies, [(&faction_id("free_company"), h(-60_00))]);
        assert!(!assessment.allowed());
    }

    #[test]
    fn a_rival_faction_does_not_bar_joining() {
        let world = world_of([member_of(character("Ava", 70_00, 20_00), &["free_company"])]);
        let assessment = world
            .assess_join(&id("ava"), &faction_id("city_watch"))
            .expect("both exist");
        assert!(assessment.allowed(), "{:?}", assessment.blocks);
    }

    #[test]
    fn setting_a_relation_one_way_changes_that_direction_only() {
        let mut world = riverhold();
        assert_eq!(
            world.execute(set("city_watch", "free_company", -60_00, false)),
            Ok(vec![changed(
                1,
                "city_watch",
                "free_company",
                -30_00,
                -60_00
            )])
        );
        assert_eq!(
            relation_of(&world, "free_company", "city_watch"),
            (h(-10_00), "neutral".into())
        );
        assert!(conflict(&world, "city_watch", "free_company"));
    }

    #[test]
    fn setting_a_relation_mutually_changes_each_direction_that_moves() {
        let mut world = riverhold();
        assert_eq!(
            world.execute(set("free_company", "city_watch", -10_00, true)),
            Ok(vec![changed(
                1,
                "city_watch",
                "free_company",
                -30_00,
                -10_00
            )]),
            "free_company → city_watch is already -10"
        );
        assert_eq!(
            world.execute(set("temple", "city_watch", 60_00, true)),
            Ok(vec![])
        );
        assert_eq!(
            world.execute(set("temple", "lantern_guild", -55_00, true)),
            Ok(vec![
                changed(2, "temple", "lantern_guild", 0, -55_00),
                changed(3, "lantern_guild", "temple", 0, -55_00),
            ])
        );
    }

    #[test]
    fn shifting_a_relation_moves_it_and_stops_at_the_ends() {
        let mut world = riverhold();
        assert_eq!(
            world.execute(shift("city_watch", "lantern_guild", 50_00, true)),
            Ok(vec![
                changed(1, "city_watch", "lantern_guild", -80_00, -30_00),
                changed(2, "lantern_guild", "city_watch", -80_00, -30_00),
            ])
        );
        assert!(!conflict(&world, "city_watch", "lantern_guild"));
        assert_eq!(
            world.execute(shift("city_watch", "temple", 50_00, false)),
            Ok(vec![changed(3, "city_watch", "temple", 60_00, 100_00)])
        );
        assert_eq!(
            world.execute(shift("temple", "free_company", -150_00, false)),
            Ok(vec![changed(4, "temple", "free_company", 0, -100_00)])
        );
        assert_eq!(
            world.execute(shift("city_watch", "temple", 1_00, false)),
            Ok(vec![])
        );
    }

    #[test]
    fn relation_changes_are_refused_for_bad_factions_and_values() {
        let mut world = riverhold();
        for command in [
            set("city_wach", "temple", 0, false),
            shift("temple", "city_wach", 0, true),
        ] {
            assert_eq!(
                refused(&mut world, command),
                CommandError::UnknownFaction {
                    faction: faction_id("city_wach"),
                    suggestion: Some(faction_id("city_watch")),
                }
            );
        }
        assert_eq!(
            refused(&mut world, set("temple", "temple", 10_00, false)),
            CommandError::SelfRelation
        );
        assert_eq!(
            refused(&mut world, shift("temple", "temple", 10_00, true)),
            CommandError::SelfRelation
        );
        for value in [100_01, -100_01] {
            assert_eq!(
                refused(&mut world, set("temple", "free_company", value, true)),
                CommandError::ValueOutOfRange { value: h(value) }
            );
        }
        assert!(
            world
                .execute(set("temple", "free_company", -100_00, true))
                .is_ok()
        );
        assert_eq!(
            CommandError::ValueOutOfRange { value: h(120_00) }.to_string(),
            "120.00 is outside -100.00..100.00"
        );
        assert_eq!(
            CommandError::SelfRelation.to_string(),
            "a faction can't have a relation with itself"
        );
    }

    #[test]
    fn a_faction_is_never_in_conflict_with_itself() {
        // With a threshold above 0, any two unrelated factions are in conflict, but a
        // membership is never checked against itself.
        let content = Content {
            balance: Balance {
                conflict_threshold: h(10_00),
                ..Balance::default()
            },
            characters: [member_of(
                character("Vex", -55_00, -20_00),
                &["lantern_guild"],
            )]
            .into_iter()
            .map(|c| (c.id.clone(), c))
            .collect(),
            factions: factions(),
            ..Content::default()
        };
        let mut world = World::new(content).expect("valid content");
        assert!(
            world
                .execute(set("temple", "free_company", 50_00, true))
                .is_ok()
        );
    }

    #[test]
    fn replaying_rebuilds_relations() {
        let mut world = riverhold();
        world
            .execute(set("temple", "free_company", -70_00, true))
            .expect("accepted");
        world
            .execute(shift("city_watch", "lantern_guild", 10_00, false))
            .expect("accepted");
        let replayed = World::replay(world.content.clone(), world.events()).expect("valid content");
        assert_eq!(replayed.state, world.state);
        assert_eq!(
            relation_of(&replayed, "city_watch", "lantern_guild").0,
            h(-70_00)
        );
    }

    // Relations in content (P-32)

    #[test]
    fn relations_must_name_two_different_factions_that_exist() {
        let content = Content {
            factions: factions(),
            relations: vec![
                between("city_watch", "lantern_gild", -80_00),
                one_way("nowhere", "temple", 10_00),
                between("temple", "temple", 50_00),
            ],
            ..Content::default()
        };
        assert_eq!(
            content.problems(),
            [
                ContentProblem::UnknownRelationFaction {
                    index: 0,
                    side: RelationSide::Between(1),
                    faction: faction_id("lantern_gild"),
                    suggestion: Some(faction_id("lantern_guild")),
                },
                ContentProblem::UnknownRelationFaction {
                    index: 1,
                    side: RelationSide::From,
                    faction: faction_id("nowhere"),
                    suggestion: None,
                },
                ContentProblem::SelfRelation { index: 2 },
            ]
        );
    }

    #[test]
    fn each_direction_is_set_once_and_within_range() {
        let content = Content {
            factions: factions(),
            relations: vec![
                between("city_watch", "lantern_guild", -80_00),
                one_way("lantern_guild", "city_watch", -60_00),
                one_way("temple", "free_company", 100_01),
                one_way("free_company", "temple", -100_00),
            ],
            balance: Balance {
                conflict_threshold: h(-100_01),
                ..Balance::default()
            },
            ..Content::default()
        };
        let problems = content.problems();
        assert_eq!(
            problems,
            [
                ContentProblem::ConflictThresholdOutOfRange(h(-100_01)),
                ContentProblem::DuplicateRelation {
                    index: 1,
                    from: faction_id("lantern_guild"),
                    to: faction_id("city_watch"),
                    first: 0,
                },
                ContentProblem::RelationOutOfRange {
                    index: 2,
                    value: h(100_01),
                },
            ]
        );
        let messages: Vec<String> = problems.iter().map(ToString::to_string).collect();
        assert_eq!(
            messages,
            [
                "-100.01 is outside -100.00..100.00",
                "lantern_guild → city_watch is already set by relation[0]",
                "100.01 is outside -100.00..100.00",
            ]
        );
    }

    #[test]
    fn no_one_starts_in_two_factions_in_conflict() {
        let content = Content {
            characters: [
                member_of(
                    character("Vex", 0, 0),
                    &["lantern_guild", "temple", "city_watch"],
                ),
                member_of(character("Ava", 0, 0), &["city_watch", "free_company"]),
            ]
            .into_iter()
            .map(|c| (c.id.clone(), c))
            .collect(),
            factions: factions(),
            relations: relations(),
            ..Content::default()
        };
        let problems = content.problems();
        assert_eq!(
            problems,
            [ContentProblem::StartsInConflict {
                character: id("vex"),
                index: 2,
                faction: faction_id("city_watch"),
                other: faction_id("lantern_guild"),
                relation: h(-80_00),
            }]
        );
        assert_eq!(
            problems[0].to_string(),
            "vex can't start in both lantern_guild and city_watch: they're in conflict (-80.00)"
        );
    }

    // Standing (DESIGN.md §7.1)

    fn outcome_id(text: &str) -> OutcomeId {
        OutcomeId::new(text).expect("a valid id")
    }

    fn party_faction(text: &str) -> Party {
        Party::Faction(faction_id(text))
    }

    fn party_character(text: &str) -> Party {
        Party::Character(id(text))
    }

    fn standing_of(world: &World, subject: &str, party: &Party) -> Fixed {
        world.standing(&id(subject), party).expect("both exist")
    }

    fn stamped(seq: u64, payload: Change) -> Event {
        Event {
            seq,
            tick: Tick(0),
            payload,
        }
    }

    fn standing_changed(seq: u64, subject: &str, party: Party, before: i64, after: i64) -> Event {
        Event {
            seq,
            tick: Tick(0),
            payload: Change::StandingChanged {
                subject: id(subject),
                party,
                before: h(before),
                after: h(after),
                spilled: Vec::new(),
            },
        }
    }

    /// Riverhold's people, with Hale's starting standing, and its two outcomes.
    fn with_outcomes() -> World {
        World::new(outcomes_content()).expect("valid content")
    }

    fn outcomes_content() -> Content {
        let mut hale = member_of(character("Captain_hale", 75_00, 30_00), &["city_watch"]);
        hale.weights = Some(weights(1_00, 25));
        hale.standing = named(&[("city_watch", 75_00)], &[]);
        let outcomes = [
            Outcome {
                id: outcome_id("fined_by_watch"),
                effects: Effects {
                    alignment: AlignmentDelta::default(),
                    standing: named(&[("city_watch", -20_00)], &[("captain_hale", -10_00)]),
                },
            },
            Outcome {
                id: outcome_id("rescued_merchant"),
                effects: Effects {
                    alignment: AlignmentDelta {
                        law: h(0),
                        good: h(6_00),
                    },
                    standing: named(&[("city_watch", 10_00)], &[("ava", 30_00)]),
                },
            },
        ];
        Content {
            characters: [
                hale,
                character("Player", 0, 0),
                character("Ava", 20_00, 10_00),
                member_of(character("Vex", -55_00, -20_00), &["lantern_guild"]),
            ]
            .into_iter()
            .map(|c| (c.id.clone(), c))
            .collect(),
            factions: factions(),
            actions: actions_with_slander(),
            relations: relations(),
            outcomes: outcomes.into_iter().map(|o| (o.id.clone(), o)).collect(),
            ..Content::default()
        }
    }

    /// The test actions, and `slander`, which names vex, so its effects on him add up.
    fn actions_with_slander() -> BTreeMap<ActionId, Action> {
        let mut slander = action("slander", 0, -2_00);
        slander.standing.target = Some(h(-5_00));
        slander.standing.named = named(&[], &[("vex", -5_00)]);
        let mut actions = actions();
        actions.insert(slander.id.clone(), slander);
        actions
    }

    fn outcome(outcome: &str, character: &str) -> Command {
        Command::ApplyOutcome {
            outcome: outcome_id(outcome),
            character: id(character),
        }
    }

    #[test]
    fn lists_the_outcomes_in_id_order() {
        let world = with_outcomes();
        let ids: Vec<&str> = world.outcomes().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, ["fined_by_watch", "rescued_merchant"]);
    }

    #[test]
    fn stealing_from_someone_lowers_their_standing_toward_the_thief() {
        let mut world = with_outcomes();
        let events = world
            .execute(act("player", "steal", Some("ava"), 1_00))
            .expect("accepted");
        assert_eq!(events.len(), 3);
        assert_eq!(
            events[2],
            standing_changed(3, "player", party_character("ava"), 0, -20_00)
        );
        assert_eq!(
            standing_of(&world, "player", &party_character("ava")),
            h(-20_00)
        );
    }

    #[test]
    fn stealing_from_a_member_also_costs_standing_with_their_factions() {
        let mut world = with_outcomes();
        let events = world
            .execute(act("player", "steal", Some("vex"), 1_00))
            .expect("accepted");
        assert_eq!(
            events[2..],
            [
                stamped(
                    3,
                    spilled(
                        "player",
                        "city_watch",
                        0,
                        1_80,
                        vec![spill("lantern_guild", -10_00, -80_00, -18, 1_80)]
                    )
                ),
                standing_changed(4, "player", party_faction("lantern_guild"), 0, -10_00),
                standing_changed(5, "player", party_character("vex"), 0, -20_00),
            ]
        );
    }

    #[test]
    fn an_action_without_a_target_changes_only_named_standing() {
        let mut world = with_outcomes();
        let events = world
            .execute(act("player", "donate_to_temple", None, 1_00))
            .expect("accepted");
        // The Watch regards the Temple at +60, so it shares 0.10 of the +10.00 (P-46).
        assert_eq!(
            events[2..],
            [
                stamped(
                    3,
                    spilled(
                        "player",
                        "city_watch",
                        0,
                        1_00,
                        vec![spill("temple", 10_00, 60_00, 10, 1_00)]
                    )
                ),
                standing_changed(4, "player", party_faction("temple"), 0, 10_00),
            ]
        );
        let events = world
            .execute(act("player", "steal", None, 1_00))
            .expect("accepted");
        assert_eq!(events.len(), 2, "no target, so no standing to change");
    }

    #[test]
    fn effects_on_the_same_party_add_up_to_one_change() {
        let mut world = with_outcomes();
        let events = world
            .execute(act("player", "slander", Some("vex"), 1_00))
            .expect("accepted");
        assert_eq!(
            events[2..],
            [standing_changed(
                3,
                "player",
                party_character("vex"),
                0,
                -10_00
            )]
        );
    }

    #[test]
    fn scale_does_not_change_standing_effects() {
        let mut world = with_outcomes();
        world
            .execute(act("player", "steal", Some("ava"), 3_00))
            .expect("accepted");
        assert_eq!(
            standing_of(&world, "player", &party_character("ava")),
            h(-20_00)
        );
    }

    #[test]
    fn an_outcome_applies_its_bundle_of_effects() {
        let mut world = with_outcomes();
        assert_eq!(
            world.execute(outcome("fined_by_watch", "player")),
            Ok(vec![
                Event {
                    seq: 1,
                    tick: Tick(0),
                    payload: Change::OutcomeApplied {
                        outcome: outcome_id("fined_by_watch"),
                        character: id("player"),
                    },
                },
                standing_changed(2, "player", party_faction("city_watch"), 0, -20_00),
                // The Watch's −20.00 spills: −0.18 to the Guild, 0.10 to the Temple.
                stamped(
                    3,
                    spilled(
                        "player",
                        "lantern_guild",
                        0,
                        3_60,
                        vec![spill("city_watch", -20_00, -80_00, -18, 3_60)]
                    )
                ),
                stamped(
                    4,
                    spilled(
                        "player",
                        "temple",
                        0,
                        -2_00,
                        vec![spill("city_watch", -20_00, 60_00, 10, -2_00)]
                    )
                ),
                standing_changed(5, "player", party_character("captain_hale"), 0, -10_00),
            ])
        );
        let events = world
            .execute(outcome("rescued_merchant", "player"))
            .expect("accepted");
        assert_eq!(
            events[1].payload,
            Change::AlignmentChanged {
                character: id("player"),
                from: aligned(0, 0),
                to: aligned(0, 6_00),
            }
        );
        assert_eq!(
            events[2..],
            [
                standing_changed(8, "player", party_faction("city_watch"), -20_00, -10_00),
                stamped(
                    9,
                    spilled(
                        "player",
                        "lantern_guild",
                        3_60,
                        1_80,
                        vec![spill("city_watch", 10_00, -80_00, -18, -1_80)]
                    )
                ),
                stamped(
                    10,
                    spilled(
                        "player",
                        "temple",
                        -2_00,
                        -1_00,
                        vec![spill("city_watch", 10_00, 60_00, 10, 1_00)]
                    )
                ),
                standing_changed(11, "player", party_character("ava"), 0, 30_00),
            ]
        );
    }

    #[test]
    fn standing_stops_at_minus_100() {
        let mut world = with_outcomes();
        for _ in 0..5 {
            world
                .execute(outcome("fined_by_watch", "player"))
                .expect("accepted");
        }
        assert_eq!(
            standing_of(&world, "player", &party_faction("city_watch")),
            h(-100_00)
        );
        let events = world
            .execute(outcome("fined_by_watch", "player"))
            .expect("accepted");
        assert_eq!(
            events[1..],
            // Each fine made 5 events: the Watch's −20.00 spills +3.60 to the Guild and −2.00
            // to the Temple. At −100.00 the Watch's change is 0 as applied, so nothing spills.
            [standing_changed(
                27,
                "player",
                party_character("captain_hale"),
                -50_00,
                -60_00
            )]
        );
    }

    #[test]
    fn starting_standing_comes_from_content() {
        let world = with_outcomes();
        assert_eq!(
            standing_of(&world, "captain_hale", &party_faction("city_watch")),
            h(75_00)
        );
        assert_eq!(
            standing_of(&world, "captain_hale", &party_faction("temple")),
            h(0)
        );
        assert_eq!(
            world.standings(&id("captain_hale")),
            Some(vec![(party_faction("city_watch"), h(75_00))])
        );
        assert_eq!(world.standings(&id("player")), Some(vec![]));
        assert_eq!(world.standings(&id("nobody")), None);
        assert_eq!(
            world.standing(&id("nobody"), &party_faction("temple")),
            None
        );
        assert_eq!(
            world.standing(&id("player"), &party_faction("nowhere")),
            None
        );
        assert_eq!(
            world.standing(&id("player"), &party_character("nobody")),
            None
        );
    }

    #[test]
    fn standings_lists_factions_then_characters() {
        let mut world = with_outcomes();
        world
            .execute(outcome("rescued_merchant", "player"))
            .expect("accepted");
        world
            .execute(act("player", "steal", Some("vex"), 1_00))
            .expect("accepted");
        assert_eq!(
            world.standings(&id("player")),
            // The Watch's +10.00 spills −1.80 to the Guild and +1.00 to the Temple; the
            // Guild's −10.00 spills +1.80 to the Watch.
            Some(vec![
                (party_faction("city_watch"), h(11_80)),
                (party_faction("lantern_guild"), h(-11_80)),
                (party_faction("temple"), h(1_00)),
                (party_character("ava"), h(30_00)),
                (party_character("vex"), h(-20_00)),
            ])
        );
    }

    #[test]
    fn leaving_a_faction_can_cost_standing_with_it() {
        let mut faction = faction("free_company", -10_00, 0, None);
        faction.leave_standing_change = h(-15_00);
        let content = Content {
            characters: [member_of(
                character("Vex", -55_00, -20_00),
                &["free_company"],
            )]
            .into_iter()
            .map(|c| (c.id.clone(), c))
            .collect(),
            factions: [(faction.id.clone(), faction)].into(),
            ..Content::default()
        };
        let mut world = World::new(content).expect("valid content");
        let events = world
            .execute(leave("vex", "free_company"))
            .expect("accepted");
        assert_eq!(
            events[1],
            standing_changed(2, "vex", party_faction("free_company"), 0, -15_00)
        );
        let mut riverhold = riverhold();
        let events = riverhold
            .execute(join("player", "free_company"))
            .and_then(|_| riverhold.execute(leave("player", "free_company")))
            .expect("accepted");
        assert_eq!(
            events.len(),
            1,
            "a leave_standing_change of 0 changes nothing"
        );
    }

    #[test]
    fn other_modules_can_apply_effects_directly() {
        let mut world = with_outcomes();
        let effects = Effects {
            alignment: AlignmentDelta::default(),
            standing: named(&[("temple", 25_00)], &[]),
        };
        let command = Command::ApplyEffects {
            source: "quest:lost_relic".to_owned(),
            character: id("player"),
            effects,
        };
        assert_eq!(
            world.execute(command),
            Ok(vec![
                Event {
                    seq: 1,
                    tick: Tick(0),
                    payload: Change::EffectsApplied {
                        source: "quest:lost_relic".to_owned(),
                        character: id("player"),
                    },
                },
                stamped(
                    2,
                    spilled(
                        "player",
                        "city_watch",
                        0,
                        2_50,
                        vec![spill("temple", 25_00, 60_00, 10, 2_50)]
                    )
                ),
                standing_changed(3, "player", party_faction("temple"), 0, 25_00),
            ])
        );
    }

    #[test]
    fn outcomes_and_effects_need_things_that_exist_and_values_in_range() {
        let mut world = with_outcomes();
        assert_eq!(
            refused(&mut world, outcome("fined_by_wach", "player")),
            CommandError::UnknownOutcome {
                outcome: outcome_id("fined_by_wach"),
                suggestion: Some(outcome_id("fined_by_watch")),
            }
        );
        assert_eq!(
            refused(&mut world, outcome("fined_by_wach", "player")).to_string(),
            "unknown outcome 'fined_by_wach' (did you mean 'fined_by_watch'?)"
        );
        assert_eq!(
            refused(&mut world, outcome("fined_by_watch", "plyer")),
            CommandError::UnknownCharacter {
                role: Role::Member,
                id: id("plyer"),
                suggestion: Some(id("player")),
            }
        );
        let effects = |standing| Command::ApplyEffects {
            source: "test".to_owned(),
            character: id("player"),
            effects: Effects {
                alignment: AlignmentDelta::default(),
                standing,
            },
        };
        assert_eq!(
            refused(&mut world, effects(named(&[("tempel", 5_00)], &[]))),
            CommandError::UnknownFaction {
                faction: faction_id("tempel"),
                suggestion: Some(faction_id("temple")),
            }
        );
        assert_eq!(
            refused(&mut world, effects(named(&[], &[("avx", 5_00)]))),
            CommandError::UnknownCharacter {
                role: Role::Member,
                id: id("avx"),
                suggestion: Some(id("ava")),
            }
        );
        assert_eq!(
            refused(&mut world, effects(named(&[("temple", 100_01)], &[]))),
            CommandError::ValueOutOfRange { value: h(100_01) }
        );
        assert_eq!(
            refused(&mut world, effects(named(&[], &[("ava", -100_01)]))),
            CommandError::ValueOutOfRange { value: h(-100_01) }
        );
        assert!(
            world
                .execute(effects(named(&[("temple", -100_00)], &[])))
                .is_ok()
        );
    }

    #[test]
    fn replaying_rebuilds_standings() {
        let mut world = with_outcomes();
        world
            .execute(outcome("fined_by_watch", "player"))
            .expect("accepted");
        world
            .execute(act("player", "steal", Some("vex"), 1_00))
            .expect("accepted");
        let replayed = World::replay(world.content.clone(), world.events()).expect("valid content");
        assert_eq!(replayed.state, world.state);
    }

    // Standing in content (P-32)

    #[test]
    fn standing_must_name_parties_that_exist_with_values_in_range() {
        let mut vex = member_of(character("Vex", 0, 0), &["lantern_guild"]);
        vex.standing = named(
            &[("lantern_gild", 30_00), ("temple", 100_01)],
            &[("vx", 5_00)],
        );
        let mut gossip = action("gossip", 0, 0);
        gossip.standing = ActionStanding {
            target: Some(h(-100_01)),
            target_factions: Some(h(100_01)),
            named: named(&[], &[("nobody", 1_00)]),
        };
        let fine = Outcome {
            id: outcome_id("fine"),
            effects: Effects {
                alignment: AlignmentDelta::default(),
                standing: named(&[("city_wach", -20_00)], &[]),
            },
        };
        let mut company = faction("free_company", -10_00, 0, None);
        company.leave_standing_change = h(-100_01);
        let mut factions = factions();
        factions.insert(company.id.clone(), company);
        let content = Content {
            characters: [(vex.id.clone(), vex)].into(),
            factions,
            actions: [(gossip.id.clone(), gossip)].into(),
            outcomes: [(fine.id.clone(), fine)].into(),
            ..Content::default()
        };
        let problems = content.problems();
        let owner_character = StandingOwner::Character(id("vex"));
        assert_eq!(
            problems,
            [
                ContentProblem::LeaveStandingOutOfRange {
                    faction: faction_id("free_company"),
                    value: h(-100_01),
                },
                ContentProblem::UnknownStandingParty {
                    owner: owner_character.clone(),
                    party: party_faction("lantern_gild"),
                    suggestion: Some(party_faction("lantern_guild")),
                },
                ContentProblem::StandingOutOfRange {
                    owner: owner_character.clone(),
                    key: StandingKey::Party(party_faction("temple")),
                    value: h(100_01),
                },
                ContentProblem::UnknownStandingParty {
                    owner: owner_character,
                    party: party_character("vx"),
                    suggestion: Some(party_character("vex")),
                },
                ContentProblem::StandingOutOfRange {
                    owner: StandingOwner::Action(action_id("gossip")),
                    key: StandingKey::Target,
                    value: h(-100_01),
                },
                ContentProblem::StandingOutOfRange {
                    owner: StandingOwner::Action(action_id("gossip")),
                    key: StandingKey::TargetFactions,
                    value: h(100_01),
                },
                ContentProblem::UnknownStandingParty {
                    owner: StandingOwner::Action(action_id("gossip")),
                    party: party_character("nobody"),
                    suggestion: None,
                },
                ContentProblem::UnknownStandingParty {
                    owner: StandingOwner::Outcome(outcome_id("fine")),
                    party: party_faction("city_wach"),
                    suggestion: Some(party_faction("city_watch")),
                },
            ]
        );
        let messages: Vec<String> = problems.iter().map(ToString::to_string).collect();
        assert_eq!(messages[0], "-100.01 is outside -100.00..100.00");
        assert_eq!(
            messages[1],
            "unknown faction 'lantern_gild' (did you mean 'lantern_guild'?)"
        );
        assert_eq!(messages[3], "unknown character 'vx' (did you mean 'vex'?)");
    }

    // Ranks (DESIGN.md §7.2)

    fn rank_of(world: &World, character: &str, faction: &str) -> String {
        world
            .memberships(&id(character))
            .expect("the character exists")
            .find(|(member_of, _)| member_of.as_str() == faction)
            .map(|(_, membership)| membership.rank.to_string())
            .expect("a member")
    }

    fn promote(character: &str, faction: &str) -> Command {
        Command::Promote {
            character: id(character),
            faction: faction_id(faction),
        }
    }

    fn demote(character: &str, faction: &str) -> Command {
        Command::Demote {
            character: id(character),
            faction: faction_id(faction),
        }
    }

    /// Riverhold's ranked people, with their starting standing.
    fn ranked_world() -> World {
        let mut vex = ranked(character("Vex", -55_00, -20_00), "lantern_guild", "fence");
        vex.standing = named(&[("lantern_guild", 30_00)], &[]);
        let mut hale = ranked(
            character("Captain_hale", 75_00, 30_00),
            "city_watch",
            "captain",
        );
        hale.standing = named(&[("city_watch", 75_00)], &[]);
        let mut mira = ranked(character("Sister_mira", 35_00, 85_00), "temple", "ordained");
        mira.standing = named(&[("temple", 40_00)], &[]);
        let mut far = ranked(character("Far", 40_00, 20_00), "city_watch", "sergeant");
        far.standing = named(&[("city_watch", 80_00)], &[]);
        world_of([vex, hale, mira, far, character("Player", 0, 0)])
    }

    fn give(world: &mut World, character: &str, faction: &str, standing: i64) {
        world
            .execute(Command::ApplyEffects {
                source: "test".to_owned(),
                character: id(character),
                effects: Effects {
                    alignment: AlignmentDelta::default(),
                    standing: named(&[(faction, standing)], &[]),
                },
            })
            .expect("accepted");
    }

    #[test]
    fn members_start_on_their_rank_or_the_lowest_rung() {
        let mut world = ranked_world();
        assert_eq!(rank_of(&world, "vex", "lantern_guild"), "fence");
        world
            .execute(join("player", "free_company"))
            .expect("accepted");
        assert_eq!(rank_of(&world, "player", "free_company"), "sellsword");
        let unranked = world_of([member_of(
            character("Vex", -55_00, -20_00),
            &["lantern_guild"],
        )]);
        assert_eq!(rank_of(&unranked, "vex", "lantern_guild"), "cutpurse");
    }

    #[test]
    fn promotion_needs_the_next_ranks_standing() {
        let mut world = ranked_world();
        let refusal = refused(&mut world, promote("vex", "lantern_guild"));
        assert_eq!(
            refusal.to_string(),
            "vex can't be promoted in lantern_guild: shadow needs standing 60.00, vex has 30.00"
        );
        give(&mut world, "vex", "lantern_guild", 30_00);
        assert_eq!(
            rank_of(&world, "vex", "lantern_guild"),
            "fence",
            "meeting the requirements never promotes anyone by itself"
        );
        let events = world
            .execute(promote("vex", "lantern_guild"))
            .expect("accepted");
        assert_eq!(
            events,
            [Event {
                // Giving 30.00 with the Guild also spilled −5.40 to the Watch (event 3).
                seq: 4,
                tick: Tick(0),
                payload: Change::RankChanged {
                    character: id("vex"),
                    faction: faction_id("lantern_guild"),
                    from: rank_id("fence"),
                    to: rank_id("shadow"),
                },
            }]
        );
        assert_eq!(rank_of(&world, "vex", "lantern_guild"), "shadow");
    }

    #[test]
    fn the_highest_rank_cannot_be_promoted() {
        let mut world = ranked_world();
        assert_eq!(
            refused(&mut world, promote("captain_hale", "city_watch")),
            CommandError::AtTopRank {
                character: id("captain_hale"),
                faction: faction_id("city_watch"),
                rank: rank_id("captain"),
            }
        );
        assert_eq!(
            refused(&mut world, promote("captain_hale", "city_watch")).to_string(),
            "captain_hale is already a captain, the highest rank of city_watch"
        );
        let assessment = world
            .assess_promotion(&id("captain_hale"), &faction_id("city_watch"))
            .expect("a member");
        assert_eq!((&assessment.next, assessment.checks.len()), (&None, 0));
        assert!(!assessment.allowed());
    }

    #[test]
    fn a_rank_can_hold_members_to_a_stricter_tolerance() {
        let world = ranked_world();
        let mira = world
            .assess_promotion(&id("sister_mira"), &faction_id("temple"))
            .expect("a member");
        assert_eq!(mira.next, Some(rank_id("high_priest")));
        assert_eq!(
            mira.checks,
            [
                RankCheck::Standing {
                    required: h(80_00),
                    has: h(40_00)
                },
                RankCheck::Tolerance {
                    limit: h(20_00),
                    distance: h(5_59)
                },
            ]
        );
        assert_eq!(
            mira.reasons(),
            ["high_priest needs standing 80.00, sister_mira has 40.00"],
            "within the rank's tolerance, so only standing fails"
        );
        // Far is a sergeant with standing 80, but 30.00 from the Watch: too far for a captain.
        let far = world
            .assess_promotion(&id("far"), &faction_id("city_watch"))
            .expect("a member");
        assert_eq!(
            far.reasons(),
            ["captain needs to be within 25.00 of The City Watch, far is 30.00 away"]
        );
    }

    #[test]
    fn demotion_moves_down_a_rung_until_the_bottom() {
        let mut world = ranked_world();
        assert_eq!(
            world.execute(demote("vex", "lantern_guild")),
            Ok(vec![Event {
                seq: 1,
                tick: Tick(0),
                payload: Change::RankChanged {
                    character: id("vex"),
                    faction: faction_id("lantern_guild"),
                    from: rank_id("fence"),
                    to: rank_id("cutpurse"),
                },
            }])
        );
        assert_eq!(
            refused(&mut world, demote("vex", "lantern_guild")).to_string(),
            "vex is already a cutpurse, the lowest rank of lantern_guild"
        );
    }

    #[test]
    fn only_members_can_be_promoted_or_demoted() {
        let mut world = ranked_world();
        for command in [promote, demote] {
            assert_eq!(
                refused(&mut world, command("player", "temple")),
                CommandError::NotAMember {
                    character: id("player"),
                    faction: faction_id("temple"),
                }
            );
            assert!(matches!(
                refused(&mut world, command("plyer", "temple")),
                CommandError::UnknownCharacter { .. }
            ));
            assert!(matches!(
                refused(&mut world, command("player", "tempel")),
                CommandError::UnknownFaction { .. }
            ));
        }
        assert_eq!(
            world.assess_promotion(&id("player"), &faction_id("temple")),
            None
        );
        assert_eq!(
            world.assess_promotion(&id("nobody"), &faction_id("temple")),
            None
        );
        assert_eq!(
            world.assess_promotion(&id("vex"), &faction_id("nowhere")),
            None
        );
    }

    #[test]
    fn replaying_rebuilds_ranks() {
        let mut world = ranked_world();
        give(&mut world, "vex", "lantern_guild", 30_00);
        world
            .execute(promote("vex", "lantern_guild"))
            .expect("accepted");
        world
            .execute(demote("captain_hale", "city_watch"))
            .expect("accepted");
        let replayed = World::replay(world.content.clone(), world.events()).expect("valid content");
        assert_eq!(replayed.state, world.state);
    }

    // Ranks in content (P-32)

    #[test]
    fn ladders_need_a_rung_unique_ids_and_values_in_range() {
        let mut watch = faction("city_watch", 70_00, 20_00, None);
        watch.ranks = vec![];
        let mut guild = faction("lantern_guild", -60_00, -10_00, None);
        guild.ranks = vec![
            rung("cutpurse", None, None),
            rung("fence", Some(120_00), Some(-1)),
            rung("cutpurse", None, None),
        ];
        let content = Content {
            factions: [(watch.id.clone(), watch), (guild.id.clone(), guild)].into(),
            ..Content::default()
        };
        let problems = content.problems();
        assert_eq!(
            problems,
            [
                ContentProblem::NoRanks(faction_id("city_watch")),
                ContentProblem::RankValueOutOfRange {
                    faction: faction_id("lantern_guild"),
                    index: 1,
                    key: RankKey::Standing,
                    value: h(120_00),
                },
                ContentProblem::RankValueOutOfRange {
                    faction: faction_id("lantern_guild"),
                    index: 1,
                    key: RankKey::Tolerance,
                    value: h(-1),
                },
                ContentProblem::DuplicateRank {
                    faction: faction_id("lantern_guild"),
                    index: 2,
                    rank: rank_id("cutpurse"),
                },
            ]
        );
        let messages: Vec<String> = problems.iter().map(ToString::to_string).collect();
        assert_eq!(
            messages,
            [
                "a faction needs at least one rank",
                "120.00 is outside -100.00..100.00",
                "-0.01 must be at least 0.00",
                "another rank is already called 'cutpurse'",
            ]
        );
    }

    #[test]
    fn rank_checks_and_warnings_allow_their_edges() {
        let mut guild = faction("lantern_guild", -60_00, -10_00, None);
        guild.ranks[1].tolerance = Some(h(0));
        guild.ranks[2].tolerance = Some(h(60_00));
        let mut vex = ranked(character("Vex", -55_00, -20_00), "lantern_guild", "shadow");
        vex.standing = named(&[("lantern_guild", 60_00)], &[]);
        let content = Content {
            characters: [(vex.id.clone(), vex)].into(),
            factions: [(guild.id.clone(), guild)].into(),
            ..Content::default()
        };
        assert_eq!(
            content.problems(),
            [],
            "a tolerance of exactly 0 is allowed"
        );
        assert_eq!(
            content.warnings(),
            [],
            "standing exactly at the requirement, and a rank tolerance equal to the member tolerance"
        );
    }

    #[test]
    fn a_starting_rank_must_be_on_the_factions_ladder() {
        let content = Content {
            characters: [ranked(character("Vex", 0, 0), "lantern_guild", "fense")]
                .into_iter()
                .map(|c| (c.id.clone(), c))
                .collect(),
            factions: factions(),
            ..Content::default()
        };
        let problems = content.problems();
        assert_eq!(
            problems,
            [ContentProblem::UnknownRank {
                character: id("vex"),
                index: 0,
                faction: faction_id("lantern_guild"),
                rank: rank_id("fense"),
                suggestion: Some(rank_id("fence")),
            }]
        );
        assert_eq!(
            problems[0].to_string(),
            "unknown rank 'fense' for lantern_guild (did you mean 'fence'?)"
        );
    }

    #[test]
    fn rank_warnings_flag_content_that_is_probably_a_mistake() {
        let mut guild = faction("lantern_guild", -60_00, -10_00, None);
        guild.ranks[2].tolerance = Some(h(70_00));
        let mut vex = ranked(character("Vex", -55_00, -20_00), "lantern_guild", "shadow");
        vex.standing = named(&[("lantern_guild", 30_00)], &[]);
        let content = Content {
            characters: [(vex.id.clone(), vex)].into(),
            factions: [(guild.id.clone(), guild)].into(),
            ..Content::default()
        };
        let warnings = content.warnings();
        assert_eq!(
            warnings,
            [
                ContentWarning::BelowRankStanding {
                    character: id("vex"),
                    index: 0,
                    rank: rank_id("shadow"),
                    required: h(60_00),
                    standing: h(30_00),
                },
                ContentWarning::RankToleranceLooser {
                    faction: faction_id("lantern_guild"),
                    index: 2,
                    rank: rank_id("shadow"),
                    tolerance: h(70_00),
                    member_tolerance: h(60_00),
                },
            ]
        );
        let messages: Vec<String> = warnings.iter().map(ToString::to_string).collect();
        assert_eq!(
            messages,
            [
                "vex starts as a shadow with standing 30.00, below the 60.00 it requires",
                "shadow's tolerance 70.00 is looser than the faction's member tolerance, 60.00, so it changes nothing",
            ]
        );
        assert!(World::new(content).is_ok());
    }

    #[test]
    fn a_faction_no_one_starts_within_tolerance_of_is_a_warning() {
        // The Temple, at 30 / 80, weighs law by half and good in full; its tolerance is 35.00.
        let content = |characters: Vec<Character>| Content {
            characters: characters.into_iter().map(|c| (c.id.clone(), c)).collect(),
            factions: [faction("temple", 30_00, 80_00, Some((50, 1_00)))]
                .into_iter()
                .map(|f| (f.id.clone(), f))
                .collect(),
            ..Content::default()
        };
        // Ava at 20 / 10 is √(5² + 70²) = 70.18 away; Vex at -55 / -20 is √(42.5² + 100²) =
        // 108.66.
        let far = content(vec![
            character("Vex", -55_00, -20_00),
            character("Ava", 20_00, 10_00),
        ]);
        let warnings = far.warnings();
        assert_eq!(
            warnings,
            [ContentWarning::NoOneWithinTolerance {
                faction: faction_id("temple"),
                faction_name: "Temple of the Dawn".to_owned(),
                tolerance: h(35_00),
                nearest: Some((id("ava"), h(70_18))),
            }]
        );
        assert_eq!(
            warnings[0].to_string(),
            "no one starts within Temple of the Dawn's tolerance of 35.00: the nearest is ava, 70.18 away"
        );
        assert!(World::new(far).is_ok(), "a warning doesn't stop the world");
        // Exactly 35.00 away is within; 35.01 isn't.
        assert_eq!(content(vec![character("Ava", 30_00, 45_00)]).warnings(), []);
        assert_eq!(
            content(vec![character("Ava", 30_00, 44_99)])
                .warnings()
                .len(),
            1
        );
        // Of two as near, the lower id.
        let tied = content(vec![
            character("Bea", 40_00, 10_00),
            character("Ava", 20_00, 10_00),
        ]);
        assert!(matches!(
            &tied.warnings()[..],
            [ContentWarning::NoOneWithinTolerance { nearest: Some((ava, _)), .. }] if *ava == id("ava")
        ));
        let empty = content(Vec::new()).warnings();
        assert_eq!(
            empty[0].to_string(),
            "no one starts within Temple of the Dawn's tolerance of 35.00: there are no characters"
        );
    }

    #[test]
    fn measures_a_point_on_the_plane_as_a_faction_sees_it() {
        let world = riverhold();
        let watch = faction_id("city_watch");
        // The Watch at 70 / 20 weighs good by a quarter: Hale's 75 / 30 is √(5² + 2.5²).
        let hale = Alignment::new(h(75_00), h(30_00)).expect("in range");
        assert_eq!(world.distance_to_point(&watch, hale), Some(h(5_59)));
        // Ava's 20 / 10 is √(50² + 2.5²) = 50.06, as distance measures Ava herself.
        let ava = Alignment::new(h(20_00), h(10_00)).expect("in range");
        assert_eq!(world.distance_to_point(&watch, ava), Some(h(50_06)));
        assert_eq!(
            world
                .distance(&Observer::Faction(watch.clone()), &id("ava"))
                .map(|distance| distance.value),
            Some(h(50_06))
        );
        // The Free Company has no weights of its own: the default, 1 and 1.
        let origin = Alignment::new(h(0), h(0)).expect("in range");
        assert_eq!(
            world.distance_to_point(&faction_id("free_company"), origin),
            Some(h(10_00))
        );
        assert_eq!(
            world.distance_to_point(&faction_id("nowhere"), origin),
            None
        );
    }

    // Saves (T4)

    /// A world with an accepted theft, a refused advance and a watch.
    fn played() -> World {
        let mut world = riverhold();
        world
            .execute(act("player", "steal", Some("ava"), 1_00))
            .expect("accepted");
        let _ = world.execute(advance(0));
        world
            .execute(Command::Watch {
                subject: id("player"),
            })
            .expect("accepted");
        world
    }

    #[test]
    fn a_save_keeps_each_accepted_commands_event_count() {
        let world = played();
        let saved = world.saved_journal();
        let theft = world
            .events()
            .iter()
            .take_while(|event| !matches!(event.payload, Change::Watched { .. }))
            .count();
        assert_eq!(
            saved,
            [
                SavedCommand {
                    command: act("player", "steal", Some("ava"), 1_00),
                    events: Some(theft),
                },
                SavedCommand {
                    command: advance(0),
                    events: None,
                },
                SavedCommand {
                    command: Command::Watch {
                        subject: id("player")
                    },
                    events: Some(world.events().len() - theft),
                },
            ]
        );
    }

    #[test]
    fn restoring_a_save_rebuilds_the_world_and_its_journal() {
        let world = played();
        let restored = World::restore(
            world.content.clone(),
            &world.saved_journal(),
            world.events(),
        )
        .expect("restores");
        assert_eq!(restored.state, world.state);
        assert_eq!(restored.events(), world.events());
        assert_eq!(
            restored.journal(),
            world.journal(),
            "the refusal's reason, too"
        );
        assert_eq!(restored.saved_journal(), world.saved_journal());
    }

    #[test]
    fn restoring_refuses_a_save_that_doesnt_add_up() {
        let world = played();
        let (content, journal, events) = (
            world.content.clone(),
            world.saved_journal(),
            world.events().to_vec(),
        );
        let total = events.len();
        let error = World::restore(content.clone(), &journal, &events[..total - 1]).unwrap_err();
        assert_eq!(
            error,
            RestoreError::EventCount {
                journal: total,
                events: total - 1
            }
        );
        assert_eq!(
            error.to_string(),
            format!(
                "the journal accounts for {total} events, but the save holds {}",
                total - 1
            )
        );
        let mut shuffled = events.clone();
        shuffled.swap(0, 1);
        assert_eq!(
            World::restore(content.clone(), &journal, &shuffled).unwrap_err(),
            RestoreError::OutOfSequence {
                expected: 1,
                found: 2
            }
        );
        // A refusal the world would accept: the save can't be what this content produced.
        let mut tampered = journal.clone();
        tampered[1].command = advance(3);
        let error = World::restore(content, &tampered, &events).unwrap_err();
        assert_eq!(error, RestoreError::NotRefused { index: 1 });
        assert_eq!(
            error.to_string(),
            "command 2 was refused when it was saved, but is accepted now"
        );
    }

    #[test]
    fn saves_read_as_plain_json_and_check_what_they_read() {
        let change = Change::AlignmentChanged {
            character: id("player"),
            from: Alignment::new(h(0), h(0)).expect("in range"),
            to: Alignment::new(h(-5_00), h(-3_00)).expect("in range"),
        };
        let json = serde_json::to_string(&change).expect("serialises");
        assert_eq!(
            json,
            r#"{"alignment_changed":{"character":"player","from":{"law":"0.00","good":"0.00"},"to":{"law":"-5.00","good":"-3.00"}}}"#
        );
        let bad_id = json.replace("\"player\"", "\"The Player\"");
        assert!(
            serde_json::from_str::<Change>(&bad_id).is_err(),
            "an invalid id"
        );
        let out_of_range = json.replace("\"-5.00\"", "\"-500.00\"");
        let error = serde_json::from_str::<Change>(&out_of_range).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("law -500.00 is outside -100.00..100.00"),
            "{error}"
        );
    }

    #[test]
    fn distance_needs_an_observer_and_a_subject_that_exist() {
        let world = riverhold();
        assert_eq!(
            world.distance(&as_faction("city_wach"), &id("player")),
            None
        );
        assert_eq!(world.distance(&as_character("nobody"), &id("player")), None);
        assert_eq!(
            world.distance(&as_faction("city_watch"), &id("nobody")),
            None
        );
    }

    // Commands, events and time

    fn advance(ticks: u64) -> Command {
        Command::AdvanceTime { ticks }
    }

    fn time_advanced(seq: u64, from: u64, to: u64) -> Event {
        Event {
            seq,
            tick: Tick(from),
            payload: Change::TimeAdvanced {
                from: Tick(from),
                to: Tick(to),
            },
        }
    }

    #[test]
    fn a_new_world_is_at_tick_zero_with_nothing_happened() {
        let world = riverhold();
        assert_eq!(world.now(), Tick(0));
        assert!(world.events().is_empty());
        assert!(world.journal().is_empty());
    }

    #[test]
    fn advancing_time_moves_the_clock_and_emits_one_event() {
        let mut world = riverhold();
        assert_eq!(world.execute(advance(5)), Ok(vec![time_advanced(1, 0, 5)]));
        assert_eq!(world.now(), Tick(5));
        assert_eq!(world.events(), [time_advanced(1, 0, 5)]);
    }

    #[test]
    fn events_are_numbered_and_stamped_across_commands() {
        let mut world = riverhold();
        world.execute(advance(5)).expect("accepted");
        assert_eq!(world.execute(advance(3)), Ok(vec![time_advanced(2, 5, 8)]));
        assert_eq!(
            world.events(),
            [time_advanced(1, 0, 5), time_advanced(2, 5, 8)]
        );
    }

    #[test]
    fn advancing_by_zero_is_refused_and_changes_nothing() {
        let mut world = riverhold();
        world.execute(advance(5)).expect("accepted");
        assert_eq!(world.execute(advance(0)), Err(CommandError::NoTicks));
        assert_eq!(world.now(), Tick(5));
        assert_eq!(world.events().len(), 1);
    }

    #[test]
    fn time_cannot_pass_the_last_tick() {
        let mut world = riverhold();
        world.execute(advance(u64::MAX)).expect("accepted");
        assert_eq!(
            world.execute(advance(1)),
            Err(CommandError::TimeOverflow {
                now: Tick(u64::MAX),
                ticks: 1,
            })
        );
        assert_eq!(world.now(), Tick(u64::MAX));
    }

    #[test]
    fn the_journal_records_every_command_and_its_result() {
        let mut world = riverhold();
        for ticks in [5, 0, 2] {
            let _ = world.execute(advance(ticks));
        }
        assert_eq!(
            world.journal(),
            [
                JournalEntry {
                    command: advance(5),
                    result: Ok(()),
                },
                JournalEntry {
                    command: advance(0),
                    result: Err(CommandError::NoTicks),
                },
                JournalEntry {
                    command: advance(2),
                    result: Ok(()),
                },
            ]
        );
    }

    #[test]
    fn events_since_gives_the_events_after_a_sequence_number() {
        let mut world = riverhold();
        for ticks in [1, 2, 3] {
            world.execute(advance(ticks)).expect("accepted");
        }
        assert_eq!(
            world.events_since(1),
            [time_advanced(2, 1, 3), time_advanced(3, 3, 6)]
        );
        assert_eq!(world.events_since(0).len(), 3);
        assert!(world.events_since(3).is_empty());
        assert!(world.events_since(99).is_empty());
    }

    #[test]
    fn replaying_the_event_log_rebuilds_the_world() {
        let mut world = riverhold();
        for ticks in [4, 6] {
            world.execute(advance(ticks)).expect("accepted");
        }
        let replayed = World::replay(world.content.clone(), world.events()).expect("valid content");
        assert_eq!(replayed.now(), Tick(10));
        assert_eq!(replayed.events(), world.events());
        assert!(replayed.journal().is_empty());
    }

    // Actions (DESIGN.md §5.2)

    fn aligned(law: i64, good: i64) -> Alignment {
        Alignment::new(h(law), h(good)).expect("in range")
    }

    fn act(actor: &str, action: &str, target: Option<&str>, scale: i64) -> Command {
        Command::PerformAction {
            actor: id(actor),
            action: action_id(action),
            target: target.map(id),
            scale: h(scale),
            witnesses: Witnesses::Everyone,
        }
    }

    fn performed(seq: u64, actor: &str, action: &str, target: Option<&str>, scale: i64) -> Event {
        Event {
            seq,
            tick: Tick(0),
            payload: Change::ActionPerformed {
                actor: id(actor),
                action: action_id(action),
                target: target.map(id),
                scale: h(scale),
                witnesses: Witnesses::Everyone,
            },
        }
    }

    fn alignment_changed(seq: u64, character: &str, from: Alignment, to: Alignment) -> Event {
        Event {
            seq,
            tick: Tick(0),
            payload: Change::AlignmentChanged {
                character: id(character),
                from,
                to,
            },
        }
    }

    #[test]
    fn stealing_moves_the_thief_toward_chaotic_evil() {
        let mut world = riverhold();
        let steal = act("player", "steal", Some("ava"), 1_00);
        assert_eq!(
            world.execute(steal),
            Ok(vec![
                performed(1, "player", "steal", Some("ava"), 1_00),
                alignment_changed(2, "player", aligned(0, 0), aligned(-5_00, -3_00)),
                // From M3, the victim thinks less of the thief too.
                standing_changed(3, "player", party_character("ava"), 0, -20_00),
            ])
        );
        assert_eq!(world.alignment(&id("player")), Some(aligned(-5_00, -3_00)));
        assert_eq!(world.alignment(&id("ava")), Some(aligned(20_00, 10_00)));
    }

    #[test]
    fn a_characters_starting_alignment_stays_as_content_gave_it() {
        let mut world = riverhold();
        world
            .execute(act("player", "steal", None, 1_00))
            .expect("accepted");
        let player = world.character(&id("player")).expect("exists");
        assert_eq!(player.alignment, aligned(0, 0));
    }

    #[test]
    fn scale_says_how_big_the_act_was() {
        let mut world = riverhold();
        let events = world
            .execute(act("player", "steal", Some("ava"), 2_00))
            .expect("accepted");
        assert_eq!(
            events[1],
            alignment_changed(2, "player", aligned(0, 0), aligned(-10_00, -6_00))
        );
    }

    #[test]
    fn an_action_needs_no_target() {
        let mut world = riverhold();
        assert_eq!(
            world.execute(act("vex", "help_stranger", None, 1_00)),
            Ok(vec![
                performed(1, "vex", "help_stranger", None, 1_00),
                alignment_changed(2, "vex", aligned(-55_00, -20_00), aligned(-55_00, -16_00)),
            ])
        );
    }

    #[test]
    fn alignment_clamps_at_the_end_of_an_axis() {
        let mut world = world_of([character("Player", -98_00, 0)]);
        let events = world
            .execute(act("player", "steal", None, 1_00))
            .expect("accepted");
        assert_eq!(
            events[1],
            alignment_changed(2, "player", aligned(-98_00, 0), aligned(-100_00, -3_00))
        );
    }

    #[test]
    fn an_act_that_cannot_move_alignment_emits_no_alignment_change() {
        let mut world = world_of([character("Player", -100_00, -100_00)]);
        assert_eq!(
            world.execute(act("player", "steal", None, 1_00)),
            Ok(vec![performed(1, "player", "steal", None, 1_00)])
        );
    }

    #[test]
    fn action_events_are_stamped_with_the_current_tick() {
        let mut world = riverhold();
        world.execute(advance(7)).expect("accepted");
        let events = world
            .execute(act("player", "steal", None, 1_00))
            .expect("accepted");
        let stamps: Vec<(u64, Tick)> = events.iter().map(|e| (e.seq, e.tick)).collect();
        assert_eq!(stamps, [(2, Tick(7)), (3, Tick(7))]);
    }

    #[test]
    fn the_act_records_who_witnessed_it() {
        let mut world = riverhold();
        let witnesses = Witnesses::These([id("vex")].into());
        let command = Command::PerformAction {
            actor: id("player"),
            action: action_id("steal"),
            target: None,
            scale: h(1_00),
            witnesses: witnesses.clone(),
        };
        let events = world.execute(command).expect("accepted");
        let Change::ActionPerformed {
            witnesses: seen, ..
        } = &events[0].payload
        else {
            panic!("expected ActionPerformed first, got {:?}", events[0]);
        };
        assert_eq!(seen, &witnesses);
    }

    /// Runs a command that must be refused and checks that nothing changed but the journal.
    fn refused(world: &mut World, command: Command) -> CommandError {
        let (state, events) = (world.state.clone(), world.events.clone());
        let error = world.execute(command).expect_err("refused");
        assert_eq!(world.state, state);
        assert_eq!(world.events, events);
        assert_eq!(
            world.journal().last().map(|entry| entry.result.clone()),
            Some(Err(error.clone()))
        );
        error
    }

    #[test]
    fn an_unknown_action_is_refused_with_a_suggestion() {
        let mut world = riverhold();
        assert_eq!(
            refused(&mut world, act("player", "stael", None, 1_00)),
            CommandError::UnknownAction {
                action: action_id("stael"),
                suggestion: Some(action_id("steal")),
            }
        );
        assert_eq!(
            refused(&mut world, act("player", "dance", None, 1_00)),
            CommandError::UnknownAction {
                action: action_id("dance"),
                suggestion: None,
            }
        );
    }

    #[test]
    fn unknown_characters_are_refused_by_role() {
        let mut world = riverhold();
        assert_eq!(
            refused(&mut world, act("plyer", "steal", None, 1_00)),
            CommandError::UnknownCharacter {
                role: Role::Actor,
                id: id("plyer"),
                suggestion: Some(id("player")),
            }
        );
        assert_eq!(
            refused(&mut world, act("player", "steal", Some("merchant"), 1_00)),
            CommandError::UnknownCharacter {
                role: Role::Target,
                id: id("merchant"),
                suggestion: None,
            }
        );
        let command = Command::PerformAction {
            actor: id("player"),
            action: action_id("steal"),
            target: None,
            scale: h(1_00),
            witnesses: Witnesses::These([id("vex"), id("vx_ghost"), id("avx")].into()),
        };
        assert_eq!(
            refused(&mut world, command),
            CommandError::UnknownCharacter {
                role: Role::Witness,
                id: id("avx"),
                suggestion: Some(id("ava")),
            },
            "the first unknown witness, in id order"
        );
    }

    #[test]
    fn a_scale_of_zero_or_less_is_refused() {
        let mut world = riverhold();
        for scale in [0, -1_00] {
            assert_eq!(
                refused(&mut world, act("player", "steal", None, scale)),
                CommandError::ScaleNotPositive { scale: h(scale) }
            );
        }
        assert!(world.execute(act("player", "steal", None, 1)).is_ok());
    }

    #[test]
    fn an_actor_cannot_target_themselves() {
        let mut world = riverhold();
        assert_eq!(
            refused(&mut world, act("player", "steal", Some("player"), 1_00)),
            CommandError::TargetIsActor
        );
    }

    #[test]
    fn lists_the_action_catalogue_in_id_order() {
        let world = riverhold();
        let ids: Vec<&str> = world.actions().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["donate_to_temple", "help_stranger", "steal"]);
    }

    #[test]
    fn replaying_rebuilds_alignments() {
        let mut world = riverhold();
        world
            .execute(act("player", "steal", Some("ava"), 3_00))
            .expect("accepted");
        let replayed = World::replay(world.content.clone(), world.events()).expect("valid content");
        assert_eq!(
            replayed.alignment(&id("player")),
            Some(aligned(-15_00, -9_00))
        );
    }

    // Watched subjects and band changes (DESIGN.md §8.3)

    fn watch(subject: &str) -> Command {
        Command::Watch {
            subject: id(subject),
        }
    }

    fn faction_observer(text: &str) -> Observer {
        Observer::Faction(faction_id(text))
    }

    fn character_observer(text: &str) -> Observer {
        Observer::Character(id(text))
    }

    fn band_change(observer: Observer, from: &str, to: &str, score: i64) -> Change {
        Change::DispositionBandChanged {
            observer,
            subject: id("player"),
            from: from.to_owned(),
            to: to.to_owned(),
            score: h(score),
        }
    }

    /// The band changes among `events`.
    fn band_changes(events: &[Event]) -> Vec<Change> {
        events
            .iter()
            .filter(|event| matches!(event.payload, Change::DispositionBandChanged { .. }))
            .map(|event| event.payload.clone())
            .collect()
    }

    fn steal_from_ava(world: &mut World) -> Vec<Event> {
        world
            .execute(act("player", "steal", Some("ava"), 1_00))
            .expect("accepted")
    }

    /// DESIGN.md §8.2's player: two thefts from Ava, leaving them at −10.00 / −6.00, then
    /// watched.
    fn watched_thief(hysteresis: i64) -> World {
        let mut content = outcomes_content();
        content.balance.hysteresis = h(hysteresis);
        let mut world = World::new(content).expect("valid content");
        steal_from_ava(&mut world);
        steal_from_ava(&mut world);
        world.execute(watch("player")).expect("accepted");
        world
    }

    #[test]
    fn watching_records_every_observers_band_factions_first() {
        let world = watched_thief(0);
        let Change::Watched { subject, bands } = &world.events().last().expect("an event").payload
        else {
            panic!("expected Watched");
        };
        // The Free Company, with even weights, is 6.00 away: affinity 45.00, friendly. Ava
        // has lost 40 standing, but 34.00 away her affinity is 21.67: −18.33, neutral.
        let expected = [
            (faction_observer("city_watch"), "neutral"),
            (faction_observer("free_company"), "friendly"),
            (faction_observer("lantern_guild"), "neutral"),
            (faction_observer("temple"), "neutral"),
            (character_observer("ava"), "neutral"),
            (character_observer("captain_hale"), "neutral"),
            (character_observer("vex"), "neutral"),
        ]
        .map(|(observer, band)| (observer, band.to_owned()));
        assert_eq!((subject, &bands[..]), (&id("player"), &expected[..]));
        let watched: Vec<(&CharacterId, usize)> = world
            .watched()
            .map(|(subject, bands)| (subject, bands.len()))
            .collect();
        assert_eq!(watched, [(&id("player"), 7)]);
    }

    #[test]
    fn a_fine_turns_the_watch_and_its_captain_unfriendly() {
        let mut world = watched_thief(0);
        let events = world
            .execute(outcome("fined_by_watch", "player"))
            .expect("accepted");
        // The Watch: affinity −7.24 at 80.26, standing −20.00. Hale: DESIGN.md §8.2.
        assert_eq!(
            band_changes(&events),
            [
                band_change(
                    faction_observer("city_watch"),
                    "neutral",
                    "unfriendly",
                    -27_24
                ),
                band_change(
                    character_observer("captain_hale"),
                    "neutral",
                    "unfriendly",
                    -29_10
                ),
            ]
        );
        // They follow the command's own events, numbered on from them: each theft made 3
        // events and watching 1, so the fine's start at 8. The fine's own are the outcome,
        // the Watch, the Guild and the Temple (spilled), and Hale.
        assert_eq!(
            events.iter().map(|event| event.seq).collect::<Vec<_>>(),
            [8, 9, 10, 11, 12, 13, 14]
        );
        let watched: BTreeMap<Observer, String> = world.watched().next().expect("player").1.clone();
        assert_eq!(watched[&faction_observer("city_watch")], "unfriendly");
    }

    #[test]
    fn hysteresis_holds_a_band_until_the_score_is_well_past_its_edge() {
        let mut world = watched_thief(5_00);
        let fined = world
            .execute(outcome("fined_by_watch", "player"))
            .expect("accepted");
        assert_eq!(
            band_changes(&fined),
            [],
            "−27.24 and −29.10 are within 5 of −25.00"
        );
        // A third theft: Ava at −43.18 (affinity 16.82 at 39.82, standing −60.00), Hale at
        // −30.90 (affinity −10.90 at 90.53, standing −10.00, faction opinion −10.00). The
        // Watch, at −29.04, stays neutral.
        assert_eq!(
            band_changes(&steal_from_ava(&mut world)),
            [
                band_change(character_observer("ava"), "neutral", "unfriendly", -43_18),
                band_change(
                    character_observer("captain_hale"),
                    "neutral",
                    "unfriendly",
                    -30_90
                ),
            ]
        );
    }

    #[test]
    fn only_watched_subjects_have_band_changes() {
        let mut world = with_outcomes();
        steal_from_ava(&mut world);
        steal_from_ava(&mut world);
        let fined = world
            .execute(outcome("fined_by_watch", "player"))
            .expect("accepted");
        assert_eq!(band_changes(&fined), []);
        let mut world = watched_thief(0);
        let events = world
            .execute(Command::Unwatch {
                subject: id("player"),
            })
            .expect("accepted");
        assert_eq!(
            payloads(&events),
            [Change::Unwatched {
                subject: id("player")
            }]
        );
        let fined = world
            .execute(outcome("fined_by_watch", "player"))
            .expect("accepted");
        assert_eq!(band_changes(&fined), []);
        assert_eq!(world.watched().count(), 0);
    }

    #[test]
    fn a_move_within_a_band_changes_nothing() {
        let mut world = watched_thief(0);
        let events = world.execute(advance(5)).expect("accepted");
        assert_eq!(band_changes(&events), []);
        // A small act moves several scores, but none across an edge.
        let events = world
            .execute(act("player", "help_stranger", None, 1_00))
            .expect("accepted");
        assert_eq!(band_changes(&events), []);
    }

    #[test]
    fn watching_needs_a_character_and_says_when_it_is_already_settled() {
        let mut world = with_outcomes();
        assert_eq!(
            world.execute(watch("ghost")),
            Err(CommandError::UnknownCharacter {
                role: Role::Subject,
                id: id("ghost"),
                suggestion: None,
            })
        );
        world.execute(watch("player")).expect("accepted");
        assert_eq!(
            world.execute(watch("player")),
            Err(CommandError::AlreadyWatched {
                subject: id("player")
            })
        );
        assert_eq!(
            world.execute(Command::Unwatch { subject: id("vex") }),
            Err(CommandError::NotWatched { subject: id("vex") })
        );
        assert_eq!(
            [
                CommandError::AlreadyWatched {
                    subject: id("player")
                }
                .to_string(),
                CommandError::NotWatched { subject: id("vex") }.to_string(),
                world.execute(watch("plyer")).unwrap_err().to_string(),
            ],
            [
                "player is already watched",
                "vex isn't watched",
                "unknown character 'plyer' (did you mean 'player'?)",
            ]
        );
    }

    #[test]
    fn replaying_the_events_restores_who_is_watched_and_their_bands() {
        let mut world = watched_thief(0);
        world
            .execute(outcome("fined_by_watch", "player"))
            .expect("accepted");
        let replayed = World::replay(world.content.clone(), world.events()).expect("valid content");
        assert_eq!(replayed.state.watched, world.state.watched);
        assert_eq!(
            replayed.state.watched[&id("player")][&character_observer("captain_hale")],
            "unfriendly"
        );
    }

    #[test]
    fn hysteresis_cannot_be_negative() {
        let mut content = outcomes_content();
        content.balance.hysteresis = h(-1);
        assert_eq!(
            content.problems(),
            [ContentProblem::NegativeHysteresis(h(-1))]
        );
        assert_eq!(
            content.problems()[0].to_string(),
            "-0.01 must be at least 0.00"
        );
        content.balance.hysteresis = Fixed::ZERO;
        assert_eq!(content.problems(), []);
    }

    #[test]
    fn observers_show_as_their_ids() {
        assert_eq!(
            [
                faction_observer("city_watch"),
                character_observer("captain_hale")
            ]
            .map(|observer| observer.to_string()),
            ["city_watch", "captain_hale"]
        );
        assert!(
            faction_observer("temple") < character_observer("ava"),
            "factions first"
        );
    }

    // Inertia (DESIGN.md §5.3)

    fn profile_id(text: &str) -> ProfileId {
        ProfileId::new(text).expect("a valid id")
    }

    fn points(points: &[(i64, i64)]) -> Curve {
        Curve::from_points(points.iter().map(|&(x, y)| (h(x), h(y))).collect()).expect("valid")
    }

    /// DESIGN.md §5.3's `hardening`.
    fn hardening() -> InertiaProfile {
        InertiaProfile {
            curves: [
                (
                    Toward::Good,
                    points(&[(-100_00, 50), (0, 1_00), (100_00, 30)]),
                ),
                (
                    Toward::Evil,
                    points(&[(-100_00, 30), (0, 1_00), (100_00, 50)]),
                ),
            ]
            .into(),
        }
    }

    fn hardened(mut character: Character) -> Character {
        character.inertia = Some(profile_id("hardening"));
        character
    }

    /// Riverhold's factions and actions, `steady` and `hardening` profiles, and
    /// `rescued_merchant` (good +6).
    fn with_inertia(characters: impl IntoIterator<Item = Character>) -> Content {
        let rescued = Outcome {
            id: outcome_id("rescued_merchant"),
            effects: Effects {
                alignment: AlignmentDelta {
                    law: h(0),
                    good: h(6_00),
                },
                standing: StandingEffects::default(),
            },
        };
        let mut inertia = Inertia::default();
        inertia
            .profiles
            .insert(profile_id("hardening"), hardening());
        Content {
            balance: Balance {
                inertia,
                ..Balance::default()
            },
            characters: characters.into_iter().map(|c| (c.id.clone(), c)).collect(),
            factions: factions(),
            actions: actions(),
            relations: relations(),
            outcomes: [(rescued.id.clone(), rescued)].into(),
        }
    }

    fn mira() -> Character {
        hardened(character("Mira", 35_00, 85_00))
    }

    #[test]
    fn a_hardening_priestess_is_moved_less_by_a_good_deed() {
        let mut world = World::new(with_inertia([mira(), character("Ava", 20_00, 10_00)]))
            .expect("valid content");
        let help = AlignmentDelta {
            law: h(0),
            good: h(4_00),
        };
        let shift = world
            .shift(&id("mira"), help, Fixed::ONE)
            .expect("mira exists");
        assert_eq!(shift.profile, profile_id("hardening"));
        // toward_good at 85 is 0.405: 4.00 × 0.405 = 1.62, rounded once.
        assert_eq!(
            shift.axes,
            [AxisShift {
                axis: Axis::Good,
                from: h(85_00),
                base: h(4_00),
                scale: Fixed::ONE,
                by_target: None,
                by_relation: None,
                // 0.405 is 0.81 × 0.50.
                inertia: Some((
                    Toward::Good,
                    Ratio::from_fixed(h(81))
                        .checked_mul(Ratio::from_fixed(h(50)))
                        .expect("small"),
                )),
                shift: h(1_62),
                to: h(86_62),
            }]
        );
        let events = world
            .execute(act("mira", "help_stranger", Some("ava"), 1_00))
            .expect("accepted");
        assert_eq!(
            events[1].payload,
            Change::AlignmentChanged {
                character: id("mira"),
                from: Alignment::new(h(35_00), h(85_00)).expect("in range"),
                to: Alignment::new(h(35_00), h(86_62)).expect("in range"),
            }
        );
        assert_eq!(world.shift(&id("ghost"), help, Fixed::ONE), None);
    }

    #[test]
    fn the_default_profile_applies_to_anyone_without_their_own() {
        let mut steady = character("Ava", 20_00, 60_00);
        steady.inertia = Some(Inertia::steady());
        let mut content = with_inertia([character("Hale", 75_00, 60_00), steady]);
        content.balance.inertia.default_profile = profile_id("hardening");
        let world = World::new(content).expect("valid content");
        let help = AlignmentDelta {
            law: h(0),
            good: h(4_00),
        };
        let shifted = |who: &str| {
            let shift = world.shift(&id(who), help, Fixed::ONE).expect("exists");
            (shift.profile.to_string(), shift.axes[0].shift)
        };
        // toward_good at 60 is 0.58: +2.32 (DESIGN.md §5.3).
        assert_eq!(shifted("hale"), ("hardening".to_owned(), h(232)));
        assert_eq!(shifted("ava"), ("steady".to_owned(), h(4_00)));
    }

    #[test]
    fn outcomes_move_alignment_with_inertia_too() {
        let mut world = World::new(with_inertia([mira()])).expect("valid content");
        let events = world
            .execute(outcome("rescued_merchant", "mira"))
            .expect("accepted");
        // 6.00 × 0.405 = 2.43.
        assert_eq!(
            events[1].payload,
            Change::AlignmentChanged {
                character: id("mira"),
                from: Alignment::new(h(35_00), h(85_00)).expect("in range"),
                to: Alignment::new(h(35_00), h(87_43)).expect("in range"),
            }
        );
    }

    #[test]
    fn inertia_profiles_are_checked_when_content_loads() {
        let mut stedy = character("Ava", 20_00, 10_00);
        stedy.inertia = Some(profile_id("stedy"));
        let mut content = with_inertia([stedy, mira()]);
        content.balance.inertia.default_profile = profile_id("hardenning");
        let profile = content
            .balance
            .inertia
            .profiles
            .get_mut(&profile_id("hardening"))
            .expect("hardening");
        profile
            .curves
            .insert(Toward::Lawful, points(&[(-100_00, 1_00), (100_00, -10)]));
        // A multiplier of exactly 0 is allowed: it stops a shift without reversing it.
        profile
            .curves
            .insert(Toward::Chaotic, Curve::constant(Fixed::ZERO));
        assert_eq!(
            content.problems(),
            [
                ContentProblem::UnknownProfile {
                    user: ProfileUser::Default,
                    profile: profile_id("hardenning"),
                    suggestion: Some(profile_id("hardening")),
                },
                ContentProblem::NegativeInertia {
                    profile: profile_id("hardening"),
                    toward: Toward::Lawful,
                    value: h(-10),
                },
                ContentProblem::UnknownProfile {
                    user: ProfileUser::Character(id("ava")),
                    profile: profile_id("stedy"),
                    suggestion: Some(Inertia::steady()),
                },
            ]
        );
        let messages: Vec<String> = content.problems().iter().map(ToString::to_string).collect();
        assert_eq!(
            messages,
            [
                "unknown inertia profile 'hardenning' (did you mean 'hardening'?)",
                "-0.10 is below 0.00: inertia can damp or amplify a shift, never reverse it",
                "unknown inertia profile 'stedy' (did you mean 'steady'?)",
            ]
        );
    }

    // Target-aware effects (DESIGN.md §5.4)

    /// `murder` with its `by_target` curves from DESIGN.md §5.4.
    fn murder() -> Action {
        let mut murder = action("murder", -10_00, -15_00);
        murder.by_target = [
            (
                TargetCurve::Good,
                points(&[(-100_00, 20), (0, 1_00), (100_00, 1_50)]),
            ),
            (
                TargetCurve::Relation,
                points(&[(-100_00, 50), (-50_00, 80), (0, 1_00)]),
            ),
        ]
        .into();
        murder
    }

    /// Riverhold's people for §5.4's murders: Hale (hardening, the Watch), Vex (the Guild),
    /// Ash and Mira (in no faction here) and the player.
    fn murderers() -> World {
        let mut content = with_inertia([
            hardened(member_of(character("Hale", 75_00, 30_00), &["city_watch"])),
            member_of(character("Vex", -55_00, -20_00), &["lantern_guild"]),
            character("Ash", 25_00, -70_00),
            character("Mira", 35_00, 85_00),
            character("Player", 0, 0),
        ]);
        content.actions.insert(action_id("murder"), murder());
        World::new(content).expect("valid content")
    }

    fn murder_shift(world: &World, actor: &str, target: &str) -> Vec<(Axis, Fixed)> {
        world
            .action_shift(
                &id(actor),
                &action_id("murder"),
                Some(&id(target)),
                Fixed::ONE,
            )
            .expect("all exist")
            .axes
            .iter()
            .map(|axis| (axis.axis, axis.shift))
            .collect()
    }

    #[test]
    fn who_is_murdered_changes_how_evil_it_is() {
        let world = murderers();
        assert_eq!(
            murder_shift(&world, "player", "ash"),
            [(Axis::Law, h(-10_00)), (Axis::Good, h(-6_60))]
        );
        assert_eq!(
            murder_shift(&world, "player", "mira"),
            [(Axis::Law, h(-10_00)), (Axis::Good, h(-21_38))]
        );
        assert_eq!(
            murder_shift(&world, "hale", "vex"),
            [(Axis::Law, h(-6_20)), (Axis::Good, h(-664))]
        );
    }

    #[test]
    fn a_murder_moves_the_killer_by_the_working() {
        let mut world = murderers();
        let events = world
            .execute(act("hale", "murder", Some("vex"), 1_00))
            .expect("accepted");
        assert_eq!(
            events[1].payload,
            Change::AlignmentChanged {
                character: id("hale"),
                from: Alignment::new(h(75_00), h(30_00)).expect("in range"),
                to: Alignment::new(h(68_80), h(23_36)).expect("in range"),
            }
        );
        // Without a target, murder moves the killer in full (after Hale's inertia).
        assert_eq!(
            world
                .action_shift(&id("player"), &action_id("murder"), None, Fixed::ONE)
                .expect("exists")
                .axes
                .iter()
                .map(|axis| axis.shift)
                .collect::<Vec<_>>(),
            [h(-10_00), h(-15_00)]
        );
    }

    #[test]
    fn the_relation_is_the_most_hostile_between_their_factions() {
        let relation_between = |actor: &[&str], target: &[&str]| {
            let mut content = with_inertia([
                member_of(character("Killer", 0, 0), actor),
                member_of(character("Victim", 0, 0), target),
            ]);
            content.actions.insert(action_id("murder"), murder());
            let world = World::new(content).expect("valid content");
            let shift = world
                .action_shift(
                    &id("killer"),
                    &action_id("murder"),
                    Some(&id("victim")),
                    Fixed::ONE,
                )
                .expect("all exist");
            let (relation, _) = shift.axes[0]
                .by_relation
                .clone()
                .expect("murder has the curve");
            (
                relation.value,
                relation.between.map(|(from, to)| format!("{from} → {to}")),
            )
        };
        // The Watch regards the Guild at −80, and the Temple regards it at 0.
        assert_eq!(
            relation_between(&["city_watch", "temple"], &["lantern_guild", "temple"]),
            (h(-80_00), Some("city_watch → lantern_guild".to_owned()))
        );
        // A tie goes to the first pair in id order.
        assert_eq!(
            relation_between(&["temple"], &["lantern_guild", "free_company"]),
            (Fixed::ZERO, Some("temple → free_company".to_owned()))
        );
        // A faction and itself don't count, so fellow members are unrelated: 0.
        assert_eq!(
            relation_between(&["city_watch"], &["city_watch"]),
            (Fixed::ZERO, None)
        );
        assert_eq!(relation_between(&[], &["city_watch"]), (Fixed::ZERO, None));
    }

    #[test]
    fn an_action_shift_needs_everything_it_names_to_exist() {
        let world = murderers();
        let shift = |actor: &str, action: &str, target: Option<&str>| {
            world
                .action_shift(
                    &id(actor),
                    &action_id(action),
                    target.map(id).as_ref(),
                    Fixed::ONE,
                )
                .is_some()
        };
        assert!(shift("hale", "murder", Some("vex")));
        assert!(!shift("ghost", "murder", Some("vex")));
        assert!(!shift("hale", "dance", Some("vex")));
        assert!(!shift("hale", "murder", Some("ghost")));
    }

    #[test]
    fn target_curves_cannot_go_below_zero() {
        let mut content = with_inertia([]);
        let mut cruel = murder();
        cruel
            .by_target
            .insert(TargetCurve::Good, points(&[(-100_00, -10), (100_00, 1_00)]));
        // Exactly 0 is allowed: it stops the act moving that axis.
        cruel
            .by_target
            .insert(TargetCurve::Law, Curve::constant(Fixed::ZERO));
        content.actions.insert(action_id("murder"), cruel);
        assert_eq!(
            content.problems(),
            [ContentProblem::NegativeTargetScaling {
                action: action_id("murder"),
                curve: TargetCurve::Good,
                value: h(-10),
            }]
        );
        assert_eq!(
            content.problems()[0].to_string(),
            "-0.10 is below 0.00: a target can soften or sharpen an act, never reverse it"
        );
    }

    // Standing spillover (DESIGN.md §7.1)

    fn spill(from: &str, change: i64, relation: i64, multiplier: i64, amount: i64) -> Spill {
        Spill {
            from: faction_id(from),
            change: h(change),
            relation: h(relation),
            multiplier: Ratio::from_fixed(h(multiplier)),
            amount: h(amount),
        }
    }

    fn spilled(subject: &str, party: &str, before: i64, after: i64, spills: Vec<Spill>) -> Change {
        Change::StandingChanged {
            subject: id(subject),
            party: Party::Faction(faction_id(party)),
            before: h(before),
            after: h(after),
            spilled: spills,
        }
    }

    fn standing_changes(events: &[Event]) -> Vec<Change> {
        events
            .iter()
            .filter(|event| matches!(event.payload, Change::StandingChanged { .. }))
            .map(|event| event.payload.clone())
            .collect()
    }

    /// The Watch, the Guild, the Temple, the Free Company and the Ashen Circle, with the
    /// default spillover curve; the player, and Vex in the Guild.
    fn spill_world(player: Character) -> World {
        defectors_world([
            player,
            member_of(character("Vex", -55_00, -20_00), &["lantern_guild"]),
        ])
    }

    fn effects_on_player(factions: &[(&str, i64)]) -> Command {
        Command::ApplyEffects {
            source: "test".to_owned(),
            character: id("player"),
            effects: Effects {
                alignment: AlignmentDelta::default(),
                standing: named(factions, &[]),
            },
        }
    }

    #[test]
    fn robbing_a_guild_member_pleases_the_watch() {
        let mut world = spill_world(character("Player", 0, 0));
        let events = world
            .execute(act("player", "steal", Some("vex"), 1_00))
            .expect("accepted");
        // The Watch regards the Guild at −80: −0.18, so −10.00 with the Guild is +1.80. The
        // Free Company (+20) and the others (0) take nothing.
        assert_eq!(
            standing_changes(&events),
            [
                spilled(
                    "player",
                    "city_watch",
                    0,
                    1_80,
                    vec![spill("lantern_guild", -10_00, -80_00, -18, 1_80)]
                ),
                spilled("player", "lantern_guild", 0, -10_00, Vec::new()),
                standing_moved_toward("player", Party::Character(id("vex")), 0, -20_00),
            ]
        );
        // One hop: the Temple, which regards the Watch at +60, gets nothing of the +1.80.
        assert_eq!(
            world.standing(&id("player"), &Party::Faction(faction_id("temple"))),
            Some(Fixed::ZERO)
        );
    }

    #[test]
    fn a_donation_pleases_the_temples_allies_and_annoys_its_enemies() {
        let mut world = spill_world(character("Player", 0, 0));
        let events = world
            .execute(act("player", "donate_to_temple", None, 1_00))
            .expect("accepted");
        // The Watch regards the Temple at +60: 0.10. The Circle at −90: −0.24.
        assert_eq!(
            standing_changes(&events),
            [
                spilled(
                    "player",
                    "ashen_circle",
                    0,
                    -2_40,
                    vec![spill("temple", 10_00, -90_00, -24, -2_40)]
                ),
                spilled(
                    "player",
                    "city_watch",
                    0,
                    1_00,
                    vec![spill("temple", 10_00, 60_00, 10, 1_00)]
                ),
                spilled("player", "temple", 0, 10_00, Vec::new()),
            ]
        );
    }

    #[test]
    fn spills_add_to_direct_changes_and_never_cascade() {
        let mut world = spill_world(character("Player", 0, 0));
        let events = world
            .execute(effects_on_player(&[
                ("lantern_guild", -10_00),
                ("city_watch", 5_00),
            ]))
            .expect("accepted");
        // The Watch: +5.00 direct and +1.80 from the Guild. The Guild: −10.00 direct and
        // −0.90 from the Watch (−0.18 × 5.00). The Temple: 0.10 × 5.00 from the Watch.
        assert_eq!(
            standing_changes(&events),
            [
                spilled(
                    "player",
                    "city_watch",
                    0,
                    6_80,
                    vec![spill("lantern_guild", -10_00, -80_00, -18, 1_80)]
                ),
                spilled(
                    "player",
                    "lantern_guild",
                    0,
                    -10_90,
                    vec![spill("city_watch", 5_00, -80_00, -18, -90)]
                ),
                spilled(
                    "player",
                    "temple",
                    0,
                    50,
                    vec![spill("city_watch", 5_00, 60_00, 10, 50)]
                ),
            ]
        );
    }

    #[test]
    fn a_spill_follows_the_change_as_applied_and_stops_at_the_ends() {
        let player = standing_with(
            standing_with(character("Player", 0, 0), "lantern_guild", -95_00),
            "city_watch",
            99_50,
        );
        let mut world = spill_world(player);
        let events = world
            .execute(act("player", "steal", Some("vex"), 1_00))
            .expect("accepted");
        // Only −5.00 of the −10.00 fits before −100: the Watch gets −0.18 × −5.00 = +0.90,
        // which stops at 100.00.
        assert_eq!(
            standing_changes(&events)[..2],
            [
                spilled(
                    "player",
                    "city_watch",
                    99_50,
                    100_00,
                    vec![spill("lantern_guild", -5_00, -80_00, -18, 90)]
                ),
                spilled("player", "lantern_guild", -95_00, -100_00, Vec::new()),
            ]
        );
    }

    #[test]
    fn a_spill_is_rounded_once_and_one_that_rounds_to_nothing_is_left_out() {
        let mut world = spill_world(character("Player", 0, 0));
        let events = world
            .execute(effects_on_player(&[("temple", 4)]))
            .expect("accepted");
        // The Circle: −0.24 × 0.04 = −0.0096, rounded once to −0.01. The Watch: 0.10 × 0.04
        // = 0.004, which rounds to nothing.
        assert_eq!(
            standing_changes(&events),
            [
                spilled(
                    "player",
                    "ashen_circle",
                    0,
                    -1,
                    vec![spill("temple", 4, -90_00, -24, -1)]
                ),
                spilled("player", "temple", 0, 4, Vec::new()),
            ]
        );
    }

    #[test]
    fn characters_standing_does_not_spill() {
        let mut world = spill_world(character("Player", 0, 0));
        let events = world
            .execute(act("player", "help_stranger", Some("vex"), 1_00))
            .expect("accepted");
        assert_eq!(
            standing_changes(&events),
            [standing_moved_toward(
                "player",
                Party::Character(id("vex")),
                0,
                10_00
            )]
        );
    }

    #[test]
    fn spillover_stays_within_minus_one_to_one() {
        let mut content = defectors_content([]);
        content.balance.spillover = points(&[(-100_00, -1_00), (100_00, 1_50)]);
        assert_eq!(
            content.problems(),
            [ContentProblem::SpilloverOutOfRange(
                CurveError::ValueOutOfRange {
                    value: h(1_50),
                    low: h(-1_00),
                    high: h(1_00),
                }
            )]
        );
        assert_eq!(
            content.problems()[0].to_string(),
            "curve value 1.50 is outside -1.00 to 1.00"
        );
        content.balance.spillover = points(&[(-100_00, -1_00), (100_00, 1_00)]);
        assert_eq!(content.problems(), []);
    }

    // Drift (DESIGN.md §9.3)

    /// The defection world's factions with the example world's drift policies: the Guild
    /// flags (the default), the Circle expels, the Temple demotes, and the Free Company
    /// ignores, with a tolerance of 1.00 so anyone drifts. `report_crime` and `extort` join
    /// the actions.
    fn drift_world(characters: impl IntoIterator<Item = Character>) -> World {
        let mut content = defectors_content(characters);
        let mut set = |faction: &str, policy: DriftPolicy| {
            content
                .factions
                .get_mut(&faction_id(faction))
                .expect("a faction")
                .drift = Some(policy);
        };
        set("ashen_circle", DriftPolicy::Expel);
        set("temple", DriftPolicy::Demote);
        set("free_company", DriftPolicy::Ignore);
        let free = content
            .factions
            .get_mut(&faction_id("free_company"))
            .expect("the Free Company");
        free.tolerances = Tolerances::new(h(1_00), Some(h(1_00))).expect("valid");
        for act in [
            action("report_crime", 4_00, 1_00),
            action("extort", -2_00, -6_00),
        ] {
            content.actions.insert(act.id.clone(), act);
        }
        World::new(content).expect("valid content")
    }

    fn alone(actor: &str, action: &str) -> Command {
        act(actor, action, None, 1_00)
    }

    /// Everything after an act's own two events (the act and the alignment change).
    fn after_the_act(events: &[Event]) -> Vec<Change> {
        payloads(&events[2..])
    }

    fn expelled(character: &str, faction: &str) -> Change {
        Change::LeftFaction {
            character: id(character),
            faction: faction_id(faction),
            reason: LeaveReason::Expelled,
        }
    }

    fn rank_changed(character: &str, faction: &str, from: &str, to: &str) -> Change {
        Change::RankChanged {
            character: id(character),
            faction: faction_id(faction),
            from: rank_id(from),
            to: rank_id(to),
        }
    }

    #[test]
    fn a_flagging_faction_reports_a_member_drifting_out_and_back() {
        let mut world = drift_world([member_of(character("Nell", 0, -10_00), &["lantern_guild"])]);
        // 4 / −9 is 64.00 from the Guild: gaps 64 and 1 × 0.5, past its 60.00.
        let out = world
            .execute(alone("nell", "report_crime"))
            .expect("accepted");
        assert_eq!(
            after_the_act(&out),
            [Change::MemberOutOfTolerance {
                character: id("nell"),
                faction: faction_id("lantern_guild"),
                distance: h(64_00),
                tolerance: h(60_00),
            }]
        );
        // Still out: nothing more to report.
        let still = world
            .execute(alone("nell", "report_crime"))
            .expect("accepted");
        assert_eq!(after_the_act(&still), []);
        // She's at 8 / −8 now. A theft takes her to 3 / −11: gaps 63 and 1 × 0.5, 63.00,
        // still out. A second to −2 / −14: gaps 58 and 4 × 0.5, 58.03, back in.
        let still = world.execute(alone("nell", "steal")).expect("accepted");
        assert_eq!(after_the_act(&still), []);
        let back = world.execute(alone("nell", "steal")).expect("accepted");
        assert_eq!(
            after_the_act(&back),
            [Change::MemberBackInTolerance {
                character: id("nell"),
                faction: faction_id("lantern_guild"),
                distance: h(58_03),
                tolerance: h(60_00),
            }]
        );
        assert_eq!(
            factions_of(&world, "nell").len(),
            1,
            "flagging never removes anyone"
        );
    }

    #[test]
    fn an_expelling_faction_throws_a_drifter_out_at_a_cost_that_spills() {
        let mut world = drift_world([member_of(
            character("Ash", 20_00, -40_00),
            &["ashen_circle"],
        )]);
        // 20 / −36 is 44.00 from the Circle, past its 40.00.
        let events = world
            .execute(alone("ash", "help_stranger"))
            .expect("accepted");
        assert_eq!(
            after_the_act(&events),
            [
                expelled("ash", "ashen_circle"),
                standing_moved("ash", "ashen_circle", 0, -20_00),
                // The Temple regards the Circle at −90: −0.24 × −20.00.
                spilled(
                    "ash",
                    "temple",
                    0,
                    4_80,
                    vec![spill("ashen_circle", -20_00, -90_00, -24, 4_80)]
                ),
            ]
        );
        assert!(factions_of(&world, "ash").is_empty());
    }

    #[test]
    fn a_demoting_faction_moves_a_drifter_down_until_their_rank_allows_them() {
        let ilse = ranked(character("Ilse", 30_00, 60_00), "temple", "high_priest");
        let mut world = drift_world([ilse]);
        // 28 / 54 is 26.02 from the Temple (gaps 2 × 0.5 and 26): past the high priest's
        // 20.00, within the Temple's 45.00, which applies to the ordained.
        let events = world.execute(alone("ilse", "extort")).expect("accepted");
        assert_eq!(
            after_the_act(&events),
            [rank_changed("ilse", "temple", "high_priest", "ordained")]
        );
        assert_eq!(rank_of(&world, "ilse", "temple"), "ordained");
    }

    #[test]
    fn a_demoted_drifter_still_out_on_the_lowest_rung_is_expelled() {
        let oren = member_of(character("Oren", 30_00, 36_00), &["temple"]);
        let fallen = ranked(character("Fallen", 30_00, 36_00), "temple", "high_priest");
        let mut world = drift_world([oren, fallen]);
        // 28 / 30 is 50.01 from the Temple, past even its 45.00.
        let expected_cost = |who: &str| {
            vec![
                expelled(who, "temple"),
                // The Circle regards the Temple at −90 (−0.24); the Watch at +60 (0.10).
                spilled(
                    who,
                    "ashen_circle",
                    0,
                    4_80,
                    vec![spill("temple", -20_00, -90_00, -24, 4_80)],
                ),
                spilled(
                    who,
                    "city_watch",
                    0,
                    -2_00,
                    vec![spill("temple", -20_00, 60_00, 10, -2_00)],
                ),
                standing_moved(who, "temple", 0, -20_00),
            ]
        };
        let events = world.execute(alone("oren", "extort")).expect("accepted");
        assert_eq!(after_the_act(&events), expected_cost("oren"));
        let events = world.execute(alone("fallen", "extort")).expect("accepted");
        let mut expected = vec![
            rank_changed("fallen", "temple", "high_priest", "ordained"),
            rank_changed("fallen", "temple", "ordained", "acolyte"),
        ];
        expected.extend(expected_cost("fallen"));
        assert_eq!(after_the_act(&events), expected);
    }

    #[test]
    fn landing_exactly_on_a_tolerance_is_still_within_it() {
        let mut world = drift_world([
            // Each act lands them exactly on their tolerance.
            member_of(character("Nell", -4_00, -11_00), &["lantern_guild"]),
            ranked(character("Ilse", 32_00, 66_00), "temple", "high_priest"),
            member_of(character("Oren", 32_00, 41_00), &["temple"]),
            member_of(character("Ash", 20_00, -44_00), &["ashen_circle"]),
        ]);
        // Nell to 0 / −10: 60.00 from the Guild. Ilse to 30 / 60: 20.00 from the Temple, the
        // high priest's tolerance. Oren to 30 / 35: 45.00, the Temple's. Ash to 20 / −40:
        // 40.00, the Circle's.
        for (who, action) in [
            ("nell", "report_crime"),
            ("ilse", "extort"),
            ("oren", "extort"),
            ("ash", "help_stranger"),
        ] {
            let events = world.execute(alone(who, action)).expect("accepted");
            assert_eq!(after_the_act(&events), [], "{who}");
        }
        assert_eq!(rank_of(&world, "ilse", "temple"), "high_priest");
    }

    #[test]
    fn an_ignoring_faction_lets_members_drift() {
        let brann = member_of(character("Brann", -10_00, 0), &["free_company"]);
        let mut world = drift_world([brann]);
        let events = world.execute(alone("brann", "steal")).expect("accepted");
        // −15 / −3 is 5.83 from the Company, far past its 1.00.
        assert_eq!(
            events.len(),
            2,
            "the act and the alignment change, nothing more"
        );
        assert_eq!(factions_of(&world, "brann").len(), 1);
    }

    #[test]
    fn drift_is_reviewed_only_when_a_members_alignment_moves() {
        // Vex starts reformed, 95.52 from the Guild: loading only warns.
        let mut world = drift_world([member_of(
            character("Vex", 35_00, 10_00),
            &["lantern_guild"],
        )]);
        assert_eq!(
            payloads(&world.execute(advance(5)).expect("accepted")).len(),
            1
        );
        let events = world
            .execute(alone("vex", "help_stranger"))
            .expect("accepted");
        assert!(matches!(
            after_the_act(&events)[..],
            [Change::MemberOutOfTolerance { .. }]
        ));
    }

    #[test]
    fn expel_standing_change_must_be_within_range() {
        let mut content = defectors_content([]);
        content
            .factions
            .get_mut(&faction_id("temple"))
            .expect("the Temple")
            .expel_standing_change = h(-120_00);
        assert_eq!(
            content.problems(),
            [ContentProblem::ExpelStandingOutOfRange {
                faction: faction_id("temple"),
                value: h(-120_00),
            }]
        );
        assert_eq!(
            content.problems()[0].to_string(),
            "-120.00 is outside -100.00..100.00"
        );
    }

    #[test]
    fn a_factions_drift_policy_is_its_own_or_the_default() {
        let world = drift_world([]);
        let policy = |faction: &str| world.drift_policy(&faction_id(faction));
        assert_eq!(
            [policy("temple"), policy("lantern_guild"), policy("nowhere")],
            [Some(DriftPolicy::Demote), Some(DriftPolicy::Flag), None]
        );
    }

    #[test]
    fn a_members_tolerance_on_each_rung_is_the_stricter_of_the_two() {
        let temple = &factions()[&faction_id("temple")];
        // acolyte and ordained set none; high_priest sets 20.00, stricter than 45.00.
        assert_eq!(
            [0, 1, 2].map(|rung| temple.member_tolerance(rung)),
            [h(45_00), h(45_00), h(20_00)]
        );
    }

    // Probation and runtime faction alignment (DESIGN.md §9.3)

    fn probation(grace_ticks: u64, then: Consequence) -> DriftPolicy {
        DriftPolicy::Probation { grace_ticks, then }
    }

    /// Riverhold's people with Hale as the Watch's captain, and the Watch on probation:
    /// `grace_ticks` and then `then`.
    fn on_watch(then: Consequence) -> World {
        let mut content = outcomes_content();
        let hale = content
            .characters
            .get_mut(&id("captain_hale"))
            .expect("Hale");
        hale.memberships[0].rank = Some(rank_id("captain"));
        content
            .factions
            .get_mut(&faction_id("city_watch"))
            .expect("the Watch")
            .drift = Some(probation(100, then));
        World::new(content).expect("valid content")
    }

    fn shift_faction(faction: &str, law: i64, good: i64) -> Command {
        Command::ShiftFactionAlignment {
            faction: faction_id(faction),
            by: AlignmentDelta {
                law: h(law),
                good: h(good),
            },
        }
    }

    fn faction_moved_to(faction: &str, from: (i64, i64), to: (i64, i64)) -> Change {
        Change::FactionAlignmentChanged {
            faction: faction_id(faction),
            from: aligned(from.0, from.1),
            to: aligned(to.0, to.1),
        }
    }

    fn probation_started(character: &str, faction: &str, until: u64) -> Change {
        Change::ProbationStarted {
            character: id(character),
            faction: faction_id(faction),
            until: Tick(until),
        }
    }

    #[test]
    fn a_member_left_behind_by_their_faction_goes_on_probation() {
        let mut world = on_watch(Consequence::Expel);
        // The Watch to 40 / 20: Hale, at 75 / 30, is 35.09 away (gaps 35 and 10 × 0.25),
        // past a captain's 25.00.
        let events = world
            .execute(shift_faction("city_watch", -30_00, 0))
            .expect("accepted");
        assert_eq!(
            payloads(&events),
            [
                faction_moved_to("city_watch", (70_00, 20_00), (40_00, 20_00)),
                probation_started("captain_hale", "city_watch", 100),
            ]
        );
        assert_eq!(
            world.faction_alignment(&faction_id("city_watch")),
            Some(aligned(40_00, 20_00))
        );
        // Back to 60 / 20 in time: 15.21 away (gaps 15 and 2.5).
        let events = world
            .execute(shift_faction("city_watch", 20_00, 0))
            .expect("accepted");
        assert_eq!(
            payloads(&events),
            [
                faction_moved_to("city_watch", (40_00, 20_00), (60_00, 20_00)),
                Change::ProbationCleared {
                    character: id("captain_hale"),
                    faction: faction_id("city_watch"),
                },
            ]
        );
        assert_eq!(
            payloads(&world.execute(advance(200)).expect("accepted")).len(),
            1
        );
    }

    #[test]
    fn probation_starts_once_past_the_tolerance_and_clears_only_what_started() {
        let mut world = on_watch(Consequence::Expel);
        let set = |law: i64, good: i64| Command::SetFactionAlignment {
            faction: faction_id("city_watch"),
            alignment: aligned(law, good),
        };
        // Nothing to clear while he's within it.
        assert_eq!(
            payloads(
                &world
                    .execute(shift_faction("city_watch", 1_00, 0))
                    .expect("accepted")
            )
            .len(),
            1
        );
        // The Watch at 50 / 30 puts Hale exactly 25.00 away: still within a captain's 25.00.
        assert_eq!(
            payloads(&world.execute(set(50_00, 30_00)).expect("accepted")).len(),
            1
        );
        // At 49.99 / 30 he's 25.01 away.
        let events = world.execute(set(49_99, 30_00)).expect("accepted");
        assert_eq!(
            payloads(&events)[1..],
            [probation_started("captain_hale", "city_watch", 100)]
        );
        // Further away, already on probation: nothing new.
        assert_eq!(
            payloads(
                &world
                    .execute(shift_faction("city_watch", -10_00, 0))
                    .expect("accepted")
            )
            .len(),
            1
        );
    }

    #[test]
    fn a_probation_that_runs_out_expels_at_a_cost_that_spills() {
        let mut world = on_watch(Consequence::Expel);
        world
            .execute(shift_faction("city_watch", -30_00, 0))
            .expect("accepted");
        let early = world.execute(advance(99)).expect("accepted");
        assert_eq!(early.len(), 1, "only the time");
        let events = world.execute(advance(1)).expect("accepted");
        assert_eq!(
            payloads(&events[1..]),
            [
                Change::ProbationExpired {
                    character: id("captain_hale"),
                    faction: faction_id("city_watch"),
                },
                expelled("captain_hale", "city_watch"),
                standing_moved("captain_hale", "city_watch", 75_00, 55_00),
                // The Guild regards the Watch at −80 (−0.18), the Temple at +60 (0.10).
                spilled(
                    "captain_hale",
                    "lantern_guild",
                    0,
                    3_60,
                    vec![spill("city_watch", -20_00, -80_00, -18, 3_60)]
                ),
                spilled(
                    "captain_hale",
                    "temple",
                    0,
                    -2_00,
                    vec![spill("city_watch", -20_00, 60_00, 10, -2_00)]
                ),
            ]
        );
    }

    #[test]
    fn a_probation_that_demotes_steps_down_to_a_rank_that_allows_them() {
        let mut world = on_watch(Consequence::Demote);
        world
            .execute(shift_faction("city_watch", -30_00, 0))
            .expect("accepted");
        let events = world.execute(advance(100)).expect("accepted");
        // 35.09 is past a captain's 25.00 but within the Watch's 50.00 for a sergeant.
        assert_eq!(
            payloads(&events[1..]),
            [
                Change::ProbationExpired {
                    character: id("captain_hale"),
                    faction: faction_id("city_watch"),
                },
                rank_changed("captain_hale", "city_watch", "captain", "sergeant"),
            ]
        );
    }

    #[test]
    fn leaving_ends_a_probation() {
        let mut world = on_watch(Consequence::Expel);
        world
            .execute(shift_faction("city_watch", -30_00, 0))
            .expect("accepted");
        world
            .execute(leave("captain_hale", "city_watch"))
            .expect("accepted");
        assert_eq!(
            payloads(&world.execute(advance(100)).expect("accepted")).len(),
            1
        );
    }

    #[test]
    fn a_faction_moving_away_flags_a_member() {
        let mut world = with_outcomes();
        // The Guild to 10 / −10: Vex, at −55 / −20, is 65.19 away (gaps 65 and 10 × 0.5).
        let events = world
            .execute(shift_faction("lantern_guild", 70_00, 0))
            .expect("accepted");
        assert_eq!(
            payloads(&events),
            [
                faction_moved_to("lantern_guild", (-60_00, -10_00), (10_00, -10_00)),
                Change::MemberOutOfTolerance {
                    character: id("vex"),
                    faction: faction_id("lantern_guild"),
                    distance: h(65_19),
                    tolerance: h(60_00),
                },
            ]
        );
    }

    #[test]
    fn setting_a_factions_alignment_moves_it_and_a_shift_stops_at_the_ends() {
        let mut world = with_outcomes();
        let set = |alignment| Command::SetFactionAlignment {
            faction: faction_id("temple"),
            alignment,
        };
        assert_eq!(
            payloads(&world.execute(set(aligned(10_00, 90_00))).expect("accepted")),
            [faction_moved_to("temple", (30_00, 80_00), (10_00, 90_00))]
        );
        assert_eq!(
            world.execute(set(aligned(10_00, 90_00))),
            Ok(vec![]),
            "no move"
        );
        assert_eq!(
            payloads(
                &world
                    .execute(shift_faction("temple", -150_00, 50_00))
                    .expect("accepted")
            ),
            [faction_moved_to(
                "temple",
                (10_00, 90_00),
                (-100_00, 100_00)
            )]
        );
        assert_eq!(
            world.execute(shift_faction("tempel", 1_00, 0)),
            Err(CommandError::UnknownFaction {
                faction: faction_id("tempel"),
                suggestion: Some(faction_id("temple")),
            })
        );
        // Distance measures against where the faction is now.
        let distance = world
            .distance(&Observer::Faction(faction_id("temple")), &id("player"))
            .expect("both exist");
        assert_eq!(distance.observer, aligned(-100_00, 100_00));
        assert_eq!(world.faction_alignment(&faction_id("nowhere")), None);
    }

    // War between your own factions (DESIGN.md §9.4)

    /// Vex, a fence (rung 2) of the Guild with standing 30, and a sellsword (rung 1) of the
    /// Free Company, which costs 10.00 to leave; Kit, a cutpurse and a sellsword with
    /// `kit_standing` with the Free Company; and `rule` for settling wars.
    fn two_sides(rule: ConflictRule) -> World {
        let mut vex = standing_with(
            ranked(character("Vex", -55_00, -20_00), "lantern_guild", "fence"),
            "lantern_guild",
            30_00,
        );
        vex.memberships.push(StartingMembership {
            faction: faction_id("free_company"),
            rank: None,
        });
        let mut content = Content {
            characters: [vex].into_iter().map(|c| (c.id.clone(), c)).collect(),
            factions: factions(),
            actions: actions(),
            relations: relations(),
            ..Content::default()
        };
        content.balance.conflict = rule;
        content
            .factions
            .get_mut(&faction_id("free_company"))
            .expect("the Company")
            .leave_standing_change = h(-10_00);
        World::new(content).expect("valid content")
    }

    fn war() -> Command {
        set("lantern_guild", "free_company", -60_00, true)
    }

    fn conflict_opened(character: &str) -> Change {
        Change::MembershipConflict {
            character: id(character),
            factions: (faction_id("free_company"), faction_id("lantern_guild")),
        }
    }

    fn resolved(character: &str, faction: &str) -> Change {
        Change::LeftFaction {
            character: id(character),
            faction: faction_id(faction),
            reason: LeaveReason::ConflictResolved,
        }
    }

    fn resolve(character: &str, keep: &str) -> Command {
        Command::ResolveConflict {
            character: id(character),
            keep: faction_id(keep),
        }
    }

    /// Leaving the Company costs 10.00 there; the Guild, now at −60 with it, takes −0.06 of
    /// that the other way: +0.60.
    fn vex_leaves_the_company() -> Vec<Change> {
        vec![
            resolved("vex", "free_company"),
            standing_moved("vex", "free_company", 0, -10_00),
            spilled(
                "vex",
                "lantern_guild",
                30_00,
                30_60,
                vec![spill("free_company", -10_00, -60_00, -6, 60)],
            ),
        ]
    }

    #[test]
    fn a_war_between_two_of_a_characters_factions_opens_a_conflict() {
        let mut world = two_sides(ConflictRule::default());
        let events = world.execute(war()).expect("accepted, no longer refused");
        assert_eq!(payloads(&events)[2..], [conflict_opened("vex")]);
        assert_eq!(
            world.conflicts(&id("vex")),
            Some(vec![(
                faction_id("free_company"),
                faction_id("lantern_guild")
            )])
        );
        assert_eq!(
            factions_of(&world, "vex").len(),
            2,
            "both memberships stand"
        );
        assert_eq!(world.conflicts(&id("ghost")), None);
    }

    #[test]
    fn a_conflict_is_settled_by_hand_at_the_cost_of_leaving() {
        let mut world = two_sides(ConflictRule::default());
        world.execute(war()).expect("accepted");
        let events = world
            .execute(resolve("vex", "lantern_guild"))
            .expect("accepted");
        assert_eq!(payloads(&events), vex_leaves_the_company());
        assert_eq!(world.conflicts(&id("vex")), Some(vec![]));
        let none = CommandError::NoConflict {
            character: id("vex"),
            faction: faction_id("lantern_guild"),
        };
        assert_eq!(
            world.execute(resolve("vex", "lantern_guild")),
            Err(none.clone())
        );
        assert_eq!(
            none.to_string(),
            "vex has no open conflict involving lantern_guild"
        );
        assert_eq!(
            world.execute(resolve("vex", "tempel")),
            Err(CommandError::UnknownFaction {
                faction: faction_id("tempel"),
                suggestion: Some(faction_id("temple")),
            })
        );
    }

    #[test]
    fn a_conflict_ends_if_the_factions_make_peace_first() {
        let mut world = two_sides(ConflictRule::default());
        world.execute(war()).expect("accepted");
        let events = world
            .execute(set("lantern_guild", "free_company", -40_00, true))
            .expect("accepted");
        assert_eq!(
            payloads(&events)[2..],
            [Change::MembershipConflictEnded {
                character: id("vex"),
                factions: (faction_id("free_company"), faction_id("lantern_guild")),
            }]
        );
        assert_eq!(factions_of(&world, "vex").len(), 2);
    }

    #[test]
    fn the_automatic_rule_keeps_the_higher_rank_first() {
        let mut world = two_sides(ConflictRule::Auto);
        let events = world.execute(war()).expect("accepted");
        // A fence (rung 2) outranks a sellsword (rung 1): the Guild stays.
        let mut expected = vec![conflict_opened("vex")];
        expected.extend(vex_leaves_the_company());
        assert_eq!(payloads(&events)[2..], expected);
    }

    /// Kit, a cutpurse and a sellsword: equal rungs. `standing` with the Free Company, and
    /// whether Kit joins the Company at tick 5 rather than starting in it.
    fn kit_at_war(standing: i64, joins_later: bool) -> Vec<Change> {
        let mut kit = standing_with(
            member_of(character("Kit", -30_00, -10_00), &["lantern_guild"]),
            "free_company",
            standing,
        );
        if !joins_later {
            kit.memberships.push(StartingMembership {
                faction: faction_id("free_company"),
                rank: None,
            });
        }
        let mut content = Content {
            characters: [(kit.id.clone(), kit)].into(),
            factions: factions(),
            actions: actions(),
            relations: relations(),
            ..Content::default()
        };
        content.balance.conflict = ConflictRule::Auto;
        let mut world = World::new(content).expect("valid content");
        if joins_later {
            world.execute(advance(5)).expect("accepted");
            world
                .execute(join("kit", "free_company"))
                .expect("22.36 away, within 60");
        }
        let events = world.execute(war()).expect("accepted");
        payloads(&events)[3..].to_vec()
    }

    #[test]
    fn the_automatic_rule_breaks_ties_by_standing_then_service_then_id() {
        // Higher standing: the Free Company (30) over the Guild (0).
        assert_eq!(
            kit_at_war(30_00, false)[0],
            resolved("kit", "lantern_guild")
        );
        // Longer service: the Guild (since tick 0) over the Company (since tick 5).
        assert_eq!(kit_at_war(0, true)[0], resolved("kit", "free_company"));
        // All equal: the lower id, free_company.
        assert_eq!(kit_at_war(0, false)[0], resolved("kit", "lantern_guild"));
    }

    #[test]
    fn asking_can_fall_back_to_the_rule_after_a_while() {
        let rule = ConflictRule::Ask {
            auto_after_ticks: Some(10),
        };
        let mut world = two_sides(rule);
        world.execute(war()).expect("accepted");
        assert_eq!(world.execute(advance(9)).expect("accepted").len(), 1);
        let events = world.execute(advance(1)).expect("accepted");
        assert_eq!(payloads(&events)[1..], vex_leaves_the_company());
        // Settled by hand first, nothing happens at tick 10.
        let mut world = two_sides(rule);
        world.execute(war()).expect("accepted");
        world.execute(advance(3)).expect("accepted");
        world
            .execute(resolve("vex", "free_company"))
            .expect("accepted");
        assert_eq!(world.execute(advance(7)).expect("accepted").len(), 1);
        assert_eq!(factions_of(&world, "vex")[0].0, "free_company");
    }

    #[test]
    fn leaving_one_side_ends_the_conflict() {
        let mut world = two_sides(ConflictRule::Ask {
            auto_after_ticks: Some(1),
        });
        world.execute(war()).expect("accepted");
        world
            .execute(leave("vex", "free_company"))
            .expect("accepted");
        assert_eq!(world.conflicts(&id("vex")), Some(vec![]));
        assert_eq!(world.execute(advance(5)).expect("accepted").len(), 1);
    }

    #[test]
    fn leaving_ends_only_the_leavers_conflict() {
        let mut kit = member_of(character("Kit", -30_00, -10_00), &["lantern_guild"]);
        kit.memberships.push(StartingMembership {
            faction: faction_id("free_company"),
            rank: None,
        });
        let mut world = World::new(Content {
            characters: [
                kit,
                member_of(
                    character("Rook", -30_00, -10_00),
                    &["lantern_guild", "free_company"],
                ),
            ]
            .into_iter()
            .map(|c| (c.id.clone(), c))
            .collect(),
            factions: factions(),
            relations: relations(),
            ..Content::default()
        })
        .expect("valid content");
        world.execute(war()).expect("accepted");
        let events = world
            .execute(leave("kit", "free_company"))
            .expect("accepted");
        // Rook's war goes on untouched: it isn't closed and opened again.
        assert_eq!(
            payloads(&events),
            [Change::LeftFaction {
                character: id("kit"),
                faction: faction_id("free_company"),
                reason: LeaveReason::Voluntary,
            }]
        );
        assert_eq!(world.conflicts(&id("kit")), Some(vec![]));
        assert_eq!(
            world.conflicts(&id("rook")),
            Some(vec![(
                faction_id("free_company"),
                faction_id("lantern_guild")
            )])
        );
    }

    #[test]
    fn a_faction_is_never_at_war_with_itself_even_with_a_positive_threshold() {
        let mut content = two_sides(ConflictRule::default()).content.clone();
        content.balance.conflict_threshold = h(10_00);
        let mut world = World::new(content).expect("valid content");
        // +5 is within 10 of nothing: the Guild and the Company are now in conflict, and
        // that's the only pair, though each faction regards itself at 0.
        let events = world
            .execute(set("lantern_guild", "free_company", 5_00, true))
            .expect("accepted");
        assert_eq!(payloads(&events)[2..], [conflict_opened("vex")]);
    }

    #[test]
    fn the_conflict_rule_says_when_it_settles() {
        assert_eq!(ConflictRule::default().auto_after(), None);
        assert_eq!(ConflictRule::Auto.auto_after(), Some(0));
        assert_eq!(
            ConflictRule::Ask {
                auto_after_ticks: Some(10)
            }
            .auto_after(),
            Some(10)
        );
    }

    // Disposition modifiers (DESIGN.md §8.1)

    fn modifier_id(text: &str) -> ModifierId {
        ModifierId::new(text).expect("a valid id")
    }

    fn modify(
        name: &str,
        observer: ModifierObserver,
        amount: i64,
        expires_at: Option<u64>,
    ) -> Command {
        Command::AddModifier {
            id: modifier_id(name),
            observer,
            subject: id("player"),
            amount: h(amount),
            expires_at: expires_at.map(Tick),
        }
    }

    fn by_character(text: &str) -> ModifierObserver {
        ModifierObserver::Character(id(text))
    }

    /// DESIGN.md §8.2's thief: two thefts from Ava and a fine, so Hale regards them at
    /// −29.10.
    fn fined_thief() -> World {
        let mut world = with_outcomes();
        steal_from_ava(&mut world);
        steal_from_ava(&mut world);
        world
            .execute(outcome("fined_by_watch", "player"))
            .expect("accepted");
        world
    }

    fn score(world: &World, observer: &Observer) -> (Fixed, String) {
        let regard = world
            .disposition(observer, &id("player"))
            .expect("both exist");
        (regard.score, regard.band)
    }

    #[test]
    fn a_modifier_changes_how_one_observer_sees_the_subject() {
        let mut world = fined_thief();
        let hale = as_character("captain_hale");
        assert_eq!(score(&world, &hale), scored(-29_10, "unfriendly"));
        let events = world
            .execute(modify("bribed", by_character("captain_hale"), 20_00, None))
            .expect("accepted");
        assert_eq!(
            payloads(&events),
            [Change::ModifierAdded {
                subject: id("player"),
                id: modifier_id("bribed"),
                observer: by_character("captain_hale"),
                amount: h(20_00),
                expires_at: None,
            }]
        );
        // The modifiers component is 20.00 × 1.00: −29.10 + 20.00 = −9.10.
        assert_eq!(score(&world, &hale), scored(-9_10, "neutral"));
        let modifiers = component(&world, &hale, "player", ComponentKind::Modifiers);
        assert_eq!((modifiers.value, modifiers.weighted), (h(20_00), h(20_00)));
        assert_eq!(
            modifiers.modifiers,
            [AppliedModifier {
                id: modifier_id("bribed"),
                observer: by_character("captain_hale"),
                amount: h(20_00),
            }]
        );
        // No one else's view changed.
        assert_eq!(score(&world, &as_character("vex")).0, {
            let mut plain = fined_thief();
            plain.execute(advance(1)).expect("accepted");
            score(&plain, &as_character("vex")).0
        });
    }

    #[test]
    fn modifiers_for_everyone_and_for_a_faction_add_up() {
        let mut world = fined_thief();
        world
            .execute(modify("bribed", by_character("captain_hale"), 20_00, None))
            .expect("accepted");
        world
            .execute(modify(
                "hero_of_riverhold",
                ModifierObserver::Everyone,
                10_00,
                None,
            ))
            .expect("accepted");
        // 20.00 + 10.00: −29.10 + 30.00 = 0.90.
        assert_eq!(
            score(&world, &as_character("captain_hale")),
            scored(90, "neutral")
        );
        world
            .execute(modify(
                "watch_favour",
                ModifierObserver::Faction(faction_id("city_watch")),
                15_00,
                None,
            ))
            .expect("accepted");
        // The Watch's favour counts for Hale, a member: 0.90 + 15.00.
        assert_eq!(score(&world, &as_character("captain_hale")).0, h(15_90));
        // And for the Watch itself: −27.24 + 10.00 + 15.00.
        assert_eq!(score(&world, &as_faction("city_watch")).0, h(-2_24));
        // Not for the Guild, which only gets everyone's 10.00.
        let guild = component(
            &world,
            &as_faction("lantern_guild"),
            "player",
            ComponentKind::Modifiers,
        );
        assert_eq!(guild.value, h(10_00));
    }

    #[test]
    fn modifiers_add_up_to_at_most_100() {
        let mut world = with_outcomes();
        for name in ["a", "b", "c"] {
            world
                .execute(modify(name, ModifierObserver::Everyone, 50_00, None))
                .expect("accepted");
        }
        let modifiers = component(
            &world,
            &as_character("ava"),
            "player",
            ComponentKind::Modifiers,
        );
        assert_eq!((modifiers.value, modifiers.modifiers.len()), (h(100_00), 3));
    }

    #[test]
    fn a_modifier_expires_when_time_reaches_it() {
        let mut world = fined_thief();
        world
            .execute(modify(
                "bribed",
                by_character("captain_hale"),
                20_00,
                Some(10),
            ))
            .expect("accepted");
        assert_eq!(world.execute(advance(9)).expect("accepted").len(), 1);
        let events = world.execute(advance(1)).expect("accepted");
        assert_eq!(
            payloads(&events)[1..],
            [Change::ModifierExpired {
                subject: id("player"),
                id: modifier_id("bribed"),
            }]
        );
        assert_eq!(
            score(&world, &as_character("captain_hale")),
            scored(-29_10, "unfriendly")
        );
    }

    #[test]
    fn the_modifiers_on_a_subject_are_listed_in_id_order() {
        let mut world = with_outcomes();
        world
            .execute(modify("zeal", ModifierObserver::Everyone, 5_00, Some(9)))
            .expect("accepted");
        world
            .execute(modify("bribed", by_character("captain_hale"), 20_00, None))
            .expect("accepted");
        let mut on_vex = modify("other", ModifierObserver::Everyone, 1_00, None);
        if let Command::AddModifier { subject, .. } = &mut on_vex {
            *subject = id("vex");
        }
        world.execute(on_vex).expect("accepted");
        assert_eq!(
            world.modifiers(&id("player")),
            Some(vec![
                (
                    AppliedModifier {
                        id: modifier_id("bribed"),
                        observer: by_character("captain_hale"),
                        amount: h(20_00),
                    },
                    None
                ),
                (
                    AppliedModifier {
                        id: modifier_id("zeal"),
                        observer: ModifierObserver::Everyone,
                        amount: h(5_00),
                    },
                    Some(Tick(9))
                ),
            ])
        );
        assert_eq!(world.modifiers(&id("ghost")), None);
    }

    #[test]
    fn a_modifier_can_be_removed() {
        let mut world = fined_thief();
        world
            .execute(modify("bribed", by_character("captain_hale"), 20_00, None))
            .expect("accepted");
        let remove = Command::RemoveModifier {
            subject: id("player"),
            id: modifier_id("bribed"),
        };
        assert_eq!(
            payloads(&world.execute(remove.clone()).expect("accepted")),
            [Change::ModifierRemoved {
                subject: id("player"),
                id: modifier_id("bribed"),
            }]
        );
        assert_eq!(
            world.execute(remove),
            Err(CommandError::NoSuchModifier {
                subject: id("player"),
                id: modifier_id("bribed"),
            })
        );
    }

    #[test]
    fn a_watched_subject_hears_when_a_modifier_moves_a_band() {
        let mut world = fined_thief();
        world.execute(watch("player")).expect("accepted");
        let events = world
            .execute(modify("bribed", by_character("captain_hale"), 20_00, None))
            .expect("accepted");
        assert_eq!(
            payloads(&events)[1..],
            [band_change(
                character_observer("captain_hale"),
                "unfriendly",
                "neutral",
                -9_10
            )]
        );
    }

    #[test]
    fn a_modifier_needs_things_that_exist_a_range_a_future_and_a_fresh_id() {
        let mut world = with_outcomes();
        world.execute(advance(5)).expect("accepted");
        let mut ghost = modify("x", ModifierObserver::Everyone, 1_00, None);
        if let Command::AddModifier { subject, .. } = &mut ghost {
            *subject = id("ghost");
        }
        assert!(matches!(
            world.execute(ghost),
            Err(CommandError::UnknownCharacter {
                role: Role::Subject,
                ..
            })
        ));
        assert!(matches!(
            world.execute(modify("x", by_character("ghost"), 1_00, None)),
            Err(CommandError::UnknownCharacter {
                role: Role::Observer,
                ..
            })
        ));
        assert!(matches!(
            world.execute(modify(
                "x",
                ModifierObserver::Faction(faction_id("nowhere")),
                1_00,
                None
            )),
            Err(CommandError::UnknownFaction { .. })
        ));
        assert_eq!(
            world.execute(modify("x", ModifierObserver::Everyone, 100_01, None)),
            Err(CommandError::ValueOutOfRange { value: h(100_01) })
        );
        assert_eq!(
            world.execute(modify("x", ModifierObserver::Everyone, 1_00, Some(5))),
            Err(CommandError::ExpiryNotAfterNow {
                expires_at: Tick(5),
                now: Tick(5),
            })
        );
        world
            .execute(modify("x", ModifierObserver::Everyone, -100_00, Some(6)))
            .expect("the ends of the range, and the next tick, are fine");
        assert_eq!(
            world.execute(modify("x", ModifierObserver::Everyone, 1_00, None)),
            Err(CommandError::AlreadyModified {
                subject: id("player"),
                id: modifier_id("x"),
            })
        );
        assert_eq!(
            [
                CommandError::AlreadyModified {
                    subject: id("player"),
                    id: modifier_id("x")
                }
                .to_string(),
                CommandError::NoSuchModifier {
                    subject: id("player"),
                    id: modifier_id("x")
                }
                .to_string(),
                CommandError::ExpiryNotAfterNow {
                    expires_at: Tick(5),
                    now: Tick(5)
                }
                .to_string(),
                ModifierObserver::Everyone.to_string(),
            ],
            [
                "player already has a modifier 'x'",
                "player has no modifier 'x'",
                "a modifier must expire after now, tick 5, not at tick 5",
                "everyone",
            ]
        );
    }

    // Joining an enemy: defectors and deserters (DESIGN.md §9.2)

    fn accept(standing_change: i64) -> Verdict {
        Verdict::Allow {
            standing_change: h(standing_change),
        }
    }

    fn refuse(reason: &str) -> Verdict {
        Verdict::Refuse {
            reason: Some(reason.to_owned()),
        }
    }

    fn when(conditions: Vec<Condition>, then: Verdict) -> Rule {
        Rule {
            when: conditions,
            then,
        }
    }

    /// The sample `membership.defectors` table (DESIGN.md §9.2).
    fn sample_defectors() -> Vec<Rule> {
        vec![
            when(
                vec![Condition::StandingWithTargetAtLeast(h(50_00))],
                accept(0),
            ),
            when(vec![Condition::CloserToTarget(true)], accept(-10_00)),
            when(Vec::new(), refuse("You serve our enemies.")),
        ]
    }

    /// The sample `membership.deserters` table.
    fn sample_deserters() -> Vec<Rule> {
        vec![
            when(
                vec![Condition::RankAtLeast(RankRef::Rung(3))],
                refuse("Officers don't walk away."),
            ),
            when(vec![Condition::OutsideMemberTolerance(true)], accept(0)),
            when(Vec::new(), accept(-40_00)),
        ]
    }

    /// The Ashen Circle, whose own deserters table lets no one go.
    fn ashen_circle() -> Faction {
        let mut circle = faction("ashen_circle", 20_00, -80_00, Some((25, 1_00)));
        circle.name = "The Ashen Circle".to_owned();
        circle.tolerances = Tolerances::new(h(30_00), Some(h(40_00))).expect("valid");
        circle.ranks = vec![
            rung("initiate", None, None),
            rung("adept", Some(40_00), None),
        ];
        circle.rule_tables = [(
            TableKind::Deserters,
            vec![when(Vec::new(), refuse("No one leaves the Circle."))],
        )]
        .into();
        circle
    }

    /// Riverhold with the sample tables and the Ashen Circle, the Temple's enemy.
    fn defectors_content(characters: impl IntoIterator<Item = Character>) -> Content {
        let mut factions = factions();
        factions.insert(faction_id("ashen_circle"), ashen_circle());
        let mut relations = relations();
        relations.push(between("temple", "ashen_circle", -90_00));
        Content {
            balance: Balance {
                rule_tables: [
                    (TableKind::Defectors, sample_defectors()),
                    (TableKind::Deserters, sample_deserters()),
                ]
                .into(),
                ..Balance::default()
            },
            characters: characters.into_iter().map(|c| (c.id.clone(), c)).collect(),
            factions,
            actions: actions(),
            relations,
            ..Content::default()
        }
    }

    fn defectors_world(characters: impl IntoIterator<Item = Character>) -> World {
        World::new(defectors_content(characters)).expect("valid content")
    }

    fn standing_with(mut character: Character, faction: &str, value: i64) -> Character {
        character
            .standing
            .factions
            .insert(faction_id(faction), h(value));
        character
    }

    /// Vex, reformed to 35 / 10 but still in the Lantern Guild, at `rank` (DESIGN.md §9.2).
    fn reformed_vex(rank: &str) -> Character {
        standing_with(
            ranked(character("Vex", 35_00, 10_00), "lantern_guild", rank),
            "lantern_guild",
            30_00,
        )
    }

    fn payloads(events: &[Event]) -> Vec<Change> {
        events.iter().map(|event| event.payload.clone()).collect()
    }

    fn left(character: &str, faction: &str) -> Change {
        Change::LeftFaction {
            character: id(character),
            faction: faction_id(faction),
            reason: LeaveReason::Defected,
        }
    }

    fn joined(character: &str, faction: &str, rank: &str) -> Change {
        Change::JoinedFaction {
            character: id(character),
            faction: faction_id(faction),
            rank: rank_id(rank),
        }
    }

    fn standing_moved(character: &str, faction: &str, before: i64, after: i64) -> Change {
        Change::StandingChanged {
            subject: id(character),
            party: Party::Faction(faction_id(faction)),
            before: h(before),
            after: h(after),
            spilled: Vec::new(),
        }
    }

    fn standing_moved_toward(character: &str, party: Party, before: i64, after: i64) -> Change {
        Change::StandingChanged {
            subject: id(character),
            party,
            before: h(before),
            after: h(after),
            spilled: Vec::new(),
        }
    }

    fn refusal(world: &mut World, character: &str, faction: &str) -> Vec<String> {
        match world.execute(join(character, faction)) {
            Err(CommandError::JoinRefused(assessment)) => assessment.reasons(),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_reformed_fence_defects_from_the_guild_to_the_watch() {
        let mut world = defectors_world([reformed_vex("fence")]);
        let assessment = world
            .assess_join(&id("vex"), &faction_id("city_watch"))
            .expect("both exist");
        // 35.09 from the Watch, within its 40; 95.52 from the Guild, past its 60.
        assert_eq!(assessment.distance.value, h(35_09));
        assert!(assessment.allowed(), "{:?}", assessment.reasons());
        let [defection] = &assessment.defections[..] else {
            panic!("one enemy membership: {:?}", assessment.defections);
        };
        // The Guild releases him on outside_member_tolerance (rule 2)...
        let deserters = &defection.deserters;
        assert_eq!(
            (deserters.source, deserters.fired(), &deserters.verdict),
            (TableSource::World, 1, &accept(0))
        );
        assert_eq!(
            deserters.tried[1].checks[0].observed,
            Observed::Tolerance {
                distance: h(95_52),
                tolerance: h(60_00)
            }
        );
        // ...and the Watch takes him on closer_to_target (rule 2), at a cost of 10.
        let defectors = &defection.defectors;
        assert_eq!(
            (&defectors.faction, defectors.fired(), &defectors.verdict),
            (&faction_id("city_watch"), 1, &accept(-10_00))
        );
        assert_eq!(
            defectors.tried[1].checks[0].observed,
            Observed::Distances {
                target: h(35_09),
                current: h(95_52)
            }
        );
        let events = world
            .execute(join("vex", "city_watch"))
            .expect("he defects");
        assert_eq!(
            payloads(&events),
            [
                left("vex", "lantern_guild"),
                joined("vex", "city_watch", "recruit"),
                standing_moved("vex", "city_watch", 0, -10_00),
                // The −10.00 with the Watch spills +1.80 to the Guild and −1.00 to the Temple.
                spilled(
                    "vex",
                    "lantern_guild",
                    30_00,
                    31_80,
                    vec![spill("city_watch", -10_00, -80_00, -18, 1_80)]
                ),
                spilled(
                    "vex",
                    "temple",
                    0,
                    -1_00,
                    vec![spill("city_watch", -10_00, 60_00, 10, -1_00)]
                ),
            ]
        );
        assert_eq!(
            factions_of(&world, "vex"),
            [("city_watch".to_owned(), Tick(0))]
        );
    }

    #[test]
    fn a_shadow_of_the_guild_is_refused_because_officers_dont_walk_away() {
        let mut world = defectors_world([reformed_vex("shadow")]);
        assert_eq!(
            refusal(&mut world, "vex", "city_watch"),
            [
                "vex belongs to The Lantern Guild, in conflict with The City Watch (-80.00): refused by The Lantern Guild's deserters rule 1, \"Officers don't walk away.\""
            ]
        );
        assert!(world.events().is_empty());
        assert_eq!(
            factions_of(&world, "vex"),
            [("lantern_guild".to_owned(), Tick(0))]
        );
    }

    #[test]
    fn a_factions_own_table_replaces_the_worlds() {
        // Ash has turned toward the light: 20.00 from the Temple, within its 35.
        let ash = member_of(character("Ash", 30_00, 60_00), &["ashen_circle"]);
        let mut world = defectors_world([ash]);
        let assessment = world
            .assess_join(&id("ash"), &faction_id("temple"))
            .expect("both exist");
        assert_eq!(
            assessment.defections[0].deserters.source,
            TableSource::Faction
        );
        assert_eq!(
            refusal(&mut world, "ash", "temple"),
            [
                "ash belongs to The Ashen Circle, in conflict with Temple of the Dawn (-90.00): refused by The Ashen Circle's deserters rule 1, \"No one leaves the Circle.\""
            ]
        );
    }

    #[test]
    fn standing_with_the_target_is_tried_first_and_costs_nothing() {
        let nell = standing_with(
            member_of(character("Nell", 35_00, 10_00), &["lantern_guild"]),
            "city_watch",
            50_00,
        );
        let mut world = defectors_world([nell]);
        let assessment = world
            .assess_join(&id("nell"), &faction_id("city_watch"))
            .expect("both exist");
        assert_eq!(assessment.defections[0].defectors.fired(), 0);
        let events = world
            .execute(join("nell", "city_watch"))
            .expect("she defects");
        assert_eq!(
            payloads(&events),
            [
                left("nell", "lantern_guild"),
                joined("nell", "city_watch", "recruit"),
            ]
        );
    }

    #[test]
    fn a_released_deserter_pays_the_tables_standing_change_after_leaving() {
        // A Watch that would take almost anyone, so a loyal-looking thief can apply.
        let mut content = defectors_content([standing_with(
            member_of(character("Mole", -60_00, -10_00), &["lantern_guild"]),
            "city_watch",
            50_00,
        )]);
        let watch = content
            .factions
            .get_mut(&faction_id("city_watch"))
            .expect("the Watch");
        // The mole is 130.22 away: gaps 130 and 30, weighted 130 and 7.5.
        watch.tolerances = Tolerances::new(h(150_00), Some(h(150_00))).expect("valid");
        let mut world = World::new(content).expect("valid content");
        let events = world
            .execute(join("mole", "city_watch"))
            .expect("the Guild lets the mole go, at a price");
        assert_eq!(
            payloads(&events),
            [
                left("mole", "lantern_guild"),
                // The Guild's −40.00 spills −0.18 to the Watch: +7.20.
                spilled(
                    "mole",
                    "city_watch",
                    50_00,
                    57_20,
                    vec![spill("lantern_guild", -40_00, -80_00, -18, 7_20)]
                ),
                standing_moved("mole", "lantern_guild", 0, -40_00),
                joined("mole", "city_watch", "recruit"),
            ]
        );
    }

    #[test]
    fn defecting_leaves_every_enemy_faction_and_adds_up_the_defectors_costs() {
        let mut content = defectors_content([member_of(
            character("Vex", 35_00, 10_00),
            &["lantern_guild", "free_company"],
        )]);
        content.relations = vec![
            between("city_watch", "lantern_guild", -80_00),
            between("city_watch", "free_company", -60_00),
            between("lantern_guild", "free_company", 20_00),
        ];
        content.balance.rule_tables =
            [(TableKind::Defectors, vec![when(Vec::new(), accept(-10_00))])].into();
        let mut world = World::new(content).expect("valid content");
        let events = world
            .execute(join("vex", "city_watch"))
            .expect("both let him go");
        assert_eq!(
            payloads(&events),
            [
                left("vex", "free_company"),
                left("vex", "lantern_guild"),
                joined("vex", "city_watch", "recruit"),
                standing_moved("vex", "city_watch", 0, -20_00),
                // The −20.00 with the Watch spills: −0.06 at −60, −0.18 at −80.
                spilled(
                    "vex",
                    "free_company",
                    0,
                    1_20,
                    vec![spill("city_watch", -20_00, -60_00, -6, 1_20)]
                ),
                spilled(
                    "vex",
                    "lantern_guild",
                    0,
                    3_60,
                    vec![spill("city_watch", -20_00, -80_00, -18, 3_60)]
                ),
            ]
        );
    }

    #[test]
    fn a_members_tolerance_is_the_stricter_of_their_ranks_and_their_factions() {
        let mut content = defectors_content([
            // 30.00 from the Watch: past a captain's 25, within the Watch's 50.
            ranked(character("Hale", 40_00, 20_00), "city_watch", "captain"),
            ranked(character("Sarge", 40_00, 20_00), "city_watch", "sergeant"),
        ]);
        content.balance.rule_tables = [(
            TableKind::Deserters,
            vec![
                when(vec![Condition::OutsideMemberTolerance(true)], accept(0)),
                when(Vec::new(), refuse("Stay.")),
            ],
        )]
        .into();
        // A sergeant's tolerance looser than the Watch's changes nothing.
        let watch = content
            .factions
            .get_mut(&faction_id("city_watch"))
            .expect("the Watch");
        watch.ranks[1].tolerance = Some(h(70_00));
        let world = World::new(content).expect("valid content");
        let observed = |who: &str| {
            world
                .assess_join(&id(who), &faction_id("lantern_guild"))
                .expect("both exist")
                .defections[0]
                .deserters
                .tried[0]
                .checks[0]
                .clone()
        };
        let tolerance = |tolerance: i64, held: bool| ConditionCheck {
            condition: Condition::OutsideMemberTolerance(true),
            observed: Observed::Tolerance {
                distance: h(30_00),
                tolerance: h(tolerance),
            },
            held,
        };
        assert_eq!(observed("hale"), tolerance(25_00, true));
        assert_eq!(observed("sarge"), tolerance(50_00, false));
    }

    #[test]
    fn rule_tables_are_checked_when_content_loads() {
        let mut content = defectors_content([]);
        content.balance.rule_tables = [
            (
                TableKind::Defectors,
                vec![
                    when(vec![Condition::RankAtLeast(RankRef::Rung(0))], accept(0)),
                    when(
                        vec![Condition::RankBelow(RankRef::Id(rank_id("shadow")))],
                        accept(0),
                    ),
                    when(
                        vec![Condition::StandingWithTargetAtLeast(h(120_00))],
                        accept(-150_00),
                    ),
                    when(vec![Condition::CloserToTarget(true)], refuse("No.")),
                ],
            ),
            (TableKind::Deserters, Vec::new()),
        ]
        .into();
        let guild = content
            .factions
            .get_mut(&faction_id("lantern_guild"))
            .expect("the Guild");
        guild.rule_tables = [(
            TableKind::Deserters,
            vec![
                when(
                    vec![Condition::RankAtLeast(RankRef::Id(rank_id("captain")))],
                    refuse("No."),
                ),
                when(
                    vec![Condition::RankAtLeast(RankRef::Id(rank_id("shadw")))],
                    refuse("No."),
                ),
                when(
                    vec![Condition::RankAtLeast(RankRef::Id(rank_id("shadow")))],
                    refuse("No."),
                ),
                when(Vec::new(), accept(0)),
            ],
        )]
        .into();
        let problem =
            |owner: TableOwner, kind: TableKind, problem: TableProblem| ContentProblem::RuleTable {
                owner,
                kind,
                problem,
            };
        let world = |found: TableProblem| problem(TableOwner::World, TableKind::Defectors, found);
        let guild = TableOwner::Faction(faction_id("lantern_guild"));
        assert_eq!(
            content.problems(),
            [
                world(TableProblem::RungBelowOne {
                    rule: 0,
                    condition: "rank_at_least",
                    rung: 0
                }),
                world(TableProblem::RankInWorldTable {
                    rule: 1,
                    condition: "rank_below",
                    rank: rank_id("shadow")
                }),
                world(TableProblem::ValueOutOfRange {
                    rule: 2,
                    key: "standing_with_target_at_least",
                    value: h(120_00)
                }),
                world(TableProblem::ValueOutOfRange {
                    rule: 2,
                    key: "standing_change",
                    value: h(-150_00)
                }),
                world(TableProblem::MightNotDecide { rules: 4 }),
                problem(
                    TableOwner::World,
                    TableKind::Deserters,
                    TableProblem::MightNotDecide { rules: 0 }
                ),
                problem(
                    guild.clone(),
                    TableKind::Deserters,
                    TableProblem::UnknownRank {
                        rule: 0,
                        condition: "rank_at_least",
                        rank: rank_id("captain"),
                        suggestion: None
                    }
                ),
                problem(
                    guild,
                    TableKind::Deserters,
                    TableProblem::UnknownRank {
                        rule: 1,
                        condition: "rank_at_least",
                        rank: rank_id("shadw"),
                        suggestion: Some(rank_id("shadow"))
                    }
                ),
            ]
        );
        let messages: Vec<String> = content.problems().iter().map(ToString::to_string).collect();
        assert_eq!(
            messages,
            [
                "0 isn't a rung: rungs count from 1, the lowest",
                "'shadow' is a rank id, but the world's tables can't name ranks: use a rung number, 1 for the lowest",
                "120.00 is outside -100.00..100.00",
                "-150.00 is outside -100.00..100.00",
                "the last rule must have no conditions, so the table always decides",
                "a table needs at least one rule, and its last must have no conditions",
                "unknown rank 'captain' for lantern_guild",
                "unknown rank 'shadw' for lantern_guild (did you mean 'shadow'?)",
            ]
        );
    }

    #[test]
    fn every_kind_of_condition_is_range_checked_where_it_has_a_number() {
        let mut content = defectors_content([]);
        let out = h(100_01);
        content.balance.rule_tables = [(
            TableKind::Deserters,
            vec![
                when(
                    vec![
                        // Rung 1, the lowest, is fine.
                        Condition::RankAtLeast(RankRef::Rung(1)),
                        Condition::RankBelow(RankRef::Rung(-1)),
                        Condition::StandingWithCurrentAtLeast(out),
                        Condition::StandingWithCurrentBelow(-out),
                        Condition::StandingWithTargetBelow(out),
                        Condition::StandingWithTargetAtLeast(h(100_00)),
                    ],
                    refuse("No."),
                ),
                when(Vec::new(), accept(100_00)),
            ],
        )]
        .into();
        let keys: Vec<String> = content
            .problems()
            .into_iter()
            .map(|problem| match problem {
                ContentProblem::RuleTable {
                    problem: TableProblem::RungBelowOne { condition, .. },
                    ..
                }
                | ContentProblem::RuleTable {
                    problem: TableProblem::ValueOutOfRange { key: condition, .. },
                    ..
                } => condition.to_owned(),
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(
            keys,
            [
                "rank_below",
                "standing_with_current_at_least",
                "standing_with_current_below",
                "standing_with_target_below",
            ]
        );
    }

    // Properties (DESIGN.md §14, invariants 1–4)

    use proptest::prelude::*;

    /// Some commands, refused ones included: zero ticks, unknown characters or actions,
    /// self-targets and scales of 0.
    fn commands() -> impl Strategy<Value = Vec<Command>> {
        let who = || {
            prop_oneof![
                Just("player"),
                Just("vex"),
                Just("ava"),
                Just("nell"),
                Just("rook"),
                Just("ghost")
            ]
        };
        let action = (
            who(),
            prop_oneof![Just("steal"), Just("help_stranger"), Just("dance")],
            proptest::option::of(who()),
            prop_oneof![0_i64..=500, Just(i64::MAX)],
        )
            .prop_map(|(actor, action, target, scale)| act(actor, action, target, scale));
        let faction = || {
            prop_oneof![
                Just("lantern_guild"),
                Just("free_company"),
                Just("temple"),
                Just("city_watch"),
                Just("nowhere")
            ]
        };
        let membership = (any::<bool>(), who(), faction()).prop_map(|(joining, who, faction)| {
            if joining {
                join(who, faction)
            } else {
                leave(who, faction)
            }
        });
        let relate = (
            faction(),
            faction(),
            -120_00_i64..=120_00,
            any::<bool>(),
            any::<bool>(),
        )
            .prop_map(|(from, to, value, mutual, shifting)| {
                if shifting {
                    shift(from, to, value, mutual)
                } else {
                    set(from, to, value, mutual)
                }
            });
        let effects = (
            who(),
            prop_oneof![Just("temple"), Just("nowhere")],
            -150_00_i64..=150_00,
        )
            .prop_map(|(who, faction, value)| Command::ApplyEffects {
                source: "test".to_owned(),
                character: id(who),
                effects: Effects {
                    alignment: AlignmentDelta::default(),
                    standing: named(&[(faction, value)], &[("vex", value)]),
                },
            });
        let watching = (any::<bool>(), who()).prop_map(|(watching, who)| {
            if watching {
                watch(who)
            } else {
                Command::Unwatch { subject: id(who) }
            }
        });
        let faction_shift = (faction(), -60_00_i64..=60_00, -60_00_i64..=60_00)
            .prop_map(|(faction, law, good)| shift_faction(faction, law, good));
        let modifying = (
            any::<bool>(),
            prop_oneof![Just("bribed"), Just("hero")],
            prop_oneof![
                Just(ModifierObserver::Everyone),
                Just(ModifierObserver::Faction(faction_id("lantern_guild"))),
                Just(ModifierObserver::Character(id("vex"))),
            ],
            who(),
            -150_00_i64..=150_00,
            proptest::option::of(0_u64..=2_000),
        )
            .prop_map(|(adding, name, observer, subject, amount, expires_at)| {
                if adding {
                    Command::AddModifier {
                        id: modifier_id(name),
                        observer,
                        subject: id(subject),
                        amount: h(amount),
                        expires_at: expires_at.map(Tick),
                    }
                } else {
                    Command::RemoveModifier {
                        subject: id(subject),
                        id: modifier_id(name),
                    }
                }
            });
        let command = prop_oneof![
            (0_u64..=1_000).prop_map(advance),
            action,
            membership,
            relate,
            effects,
            watching,
            faction_shift,
            modifying
        ];
        proptest::collection::vec(command, 0..30)
    }

    /// Riverhold with the sample rule tables. Nell is a reformed Guild member who can defect
    /// to the Watch, and Rook is in both the Guild and the Free Company, so random relations
    /// can put him in a war. The Watch puts drifters on probation for 50 ticks, then demotes
    /// them; the Guild expels them; wars are settled automatically after 20 ticks.
    fn run(commands: &[Command]) -> World {
        let mut content = defectors_content([
            character("Vex", -55_00, -20_00),
            character("Ava", 20_00, 10_00),
            character("Player", 0, 0),
            member_of(character("Nell", 35_00, 10_00), &["lantern_guild"]),
            member_of(
                character("Rook", -40_00, -10_00),
                &["lantern_guild", "free_company"],
            ),
        ]);
        content.balance.conflict = ConflictRule::Ask {
            auto_after_ticks: Some(20),
        };
        let mut drift = |faction: &str, policy: DriftPolicy| {
            content
                .factions
                .get_mut(&faction_id(faction))
                .expect("a faction")
                .drift = Some(policy);
        };
        drift("city_watch", probation(50, Consequence::Demote));
        drift("lantern_guild", DriftPolicy::Expel);
        let mut world = World::new(content).expect("valid content");
        for command in commands {
            let _ = world.execute(command.clone());
        }
        world
    }

    proptest! {
        #[test]
        fn replaying_events_reproduces_the_state(commands in commands()) {
            let world = run(&commands);
            let replayed = World::replay(world.content.clone(), world.events()).expect("valid content");
            prop_assert_eq!(&replayed.state, &world.state);
            prop_assert_eq!(replayed.events(), world.events());
        }

        #[test]
        fn re_executing_the_journal_reproduces_the_events_exactly(commands in commands()) {
            let world = run(&commands);
            let journal: Vec<Command> =
                world.journal().iter().map(|entry| entry.command.clone()).collect();
            let rerun = run(&journal);
            prop_assert_eq!(rerun.events(), world.events());
            prop_assert_eq!(rerun.journal(), world.journal());
        }

        #[test]
        fn a_refused_command_leaves_the_state_exactly_as_it_was(commands in commands()) {
            let mut world = run(&commands);
            let (state, events) = (world.state.clone(), world.events.clone());
            prop_assert!(world.execute(advance(0)).is_err());
            prop_assert_eq!(&world.state, &state);
            prop_assert_eq!(&world.events, &events);
        }

        /// DESIGN.md §14, invariant 11: restoring a save gives the same world.
        #[test]
        fn restoring_a_save_gives_the_same_world(commands in commands()) {
            let world = run(&commands);
            let restored = World::restore(world.content.clone(), &world.saved_journal(), world.events())
                .expect("restores");
            prop_assert_eq!(&restored.state, &world.state);
            prop_assert_eq!(restored.events(), world.events());
            prop_assert_eq!(restored.journal(), world.journal());
        }

        #[test]
        fn every_saved_command_and_event_reads_back_exactly(commands in commands()) {
            let world = run(&commands);
            let saved = (world.saved_journal(), world.events().to_vec());
            let json = serde_json::to_string(&saved).expect("serialises");
            let read: (Vec<SavedCommand>, Vec<Event>) =
                serde_json::from_str(&json).expect("reads back");
            prop_assert_eq!(read, saved);
        }

        #[test]
        fn the_same_commands_always_give_the_same_events(commands in commands()) {
            let (first, second) = (run(&commands), run(&commands));
            prop_assert_eq!(first.events(), second.events());
        }

        #[test]
        fn time_only_moves_forward_and_events_count_up_from_one(commands in commands()) {
            let world = run(&commands);
            for (index, event) in world.events().iter().enumerate() {
                prop_assert_eq!(event.seq, index as u64 + 1);
            }
            let total: u64 = commands
                .iter()
                .filter_map(|command| match command {
                    Command::AdvanceTime { ticks } => Some(*ticks),
                    _ => None,
                })
                .sum();
            prop_assert_eq!(world.now(), Tick(total));
        }

        /// DESIGN.md §14, invariant 6: no one is in two factions in conflict, except while a
        /// `MembershipConflict` for that pair is open.
        #[test]
        fn no_one_is_in_two_factions_in_conflict_without_an_open_conflict(
            commands in commands()
        ) {
            let world = run(&commands);
            for character in world.characters() {
                let factions: Vec<&FactionId> = world
                    .memberships(&character.id)
                    .expect("the character exists")
                    .map(|(faction, _)| faction)
                    .collect();
                let open = world.conflicts(&character.id).expect("the character exists");
                for (i, a) in factions.iter().enumerate() {
                    for b in &factions[i + 1..] {
                        if world.in_conflict(a, b).expect("both exist") {
                            prop_assert!(open.contains(&((*a).clone(), (*b).clone())));
                        }
                    }
                }
            }
        }

        /// DESIGN.md §8.3: with no hysteresis, the band remembered for a watched subject is
        /// always the band their disposition is in now.
        #[test]
        fn with_no_hysteresis_a_remembered_band_is_always_the_current_one(
            commands in commands()
        ) {
            let world = run(&commands);
            for (subject, bands) in world.watched() {
                for (observer, band) in bands {
                    let now = world.disposition(observer, subject).expect("both exist").band;
                    prop_assert_eq!(band, &now);
                }
            }
        }

        /// DESIGN.md §9.3: a flagged member is still a member, and past their tolerance as
        /// of the last time their alignment moved, which is their alignment now.
        #[test]
        fn a_flagged_member_is_a_member_out_of_tolerance(commands in commands()) {
            let world = run(&commands);
            for (character, faction) in &world.state.out_of_tolerance {
                let membership = &world.state.memberships[character][faction];
                let found = &world.content.factions[faction];
                let rung = found.rank_position(&membership.rank).expect("on the ladder");
                let distance = world
                    .distance(&Observer::Faction(faction.clone()), character)
                    .expect("both exist")
                    .value;
                prop_assert!(distance > found.member_tolerance(rung));
            }
        }

        /// DESIGN.md §9.3: anyone on probation is a member past their tolerance, and their
        /// probation hasn't run out yet.
        #[test]
        fn a_probation_is_for_a_member_out_of_tolerance_until_it_runs_out(
            commands in commands()
        ) {
            let world = run(&commands);
            for ((character, faction), until) in &world.state.probation {
                prop_assert!(*until > world.now());
                let membership = &world.state.memberships[character][faction];
                let found = &world.content.factions[faction];
                let rung = found.rank_position(&membership.rank).expect("on the ladder");
                let distance = world
                    .distance(&Observer::Faction(faction.clone()), character)
                    .expect("both exist")
                    .value;
                prop_assert!(distance > found.member_tolerance(rung));
            }
        }

        /// DESIGN.md §8.1: no modifier outlives its expiry, and the modifiers component
        /// stays within ±100 however many there are.
        #[test]
        fn modifiers_expire_on_time_and_stay_within_range(commands in commands()) {
            let world = run(&commands);
            for modifier in world.state.modifiers.values() {
                prop_assert!(modifier.expires_at.is_none_or(|until| until > world.now()));
            }
            for character in world.characters() {
                let regard = world
                    .disposition(&Observer::Faction(faction_id("lantern_guild")), &character.id)
                    .expect("both exist");
                let value = regard.component(ComponentKind::Modifiers).value;
                prop_assert!((-AXIS_LIMIT..=AXIS_LIMIT).contains(&value));
            }
        }

        /// DESIGN.md §14, invariant 1, for relations.
        #[test]
        fn standings_always_stay_within_range(commands in commands()) {
            let world = run(&commands);
            for character in world.characters() {
                for (_, value) in world.standings(&character.id).expect("the character exists") {
                    prop_assert!((-AXIS_LIMIT..=AXIS_LIMIT).contains(&value));
                }
            }
        }

        #[test]
        fn relations_always_stay_within_range(commands in commands()) {
            let world = run(&commands);
            for regard in world.relations() {
                prop_assert!((-AXIS_LIMIT..=AXIS_LIMIT).contains(&regard.value));
            }
        }

        #[test]
        fn alignments_always_stay_within_the_axes(commands in commands()) {
            let world = run(&commands);
            for character in world.characters() {
                let alignment = world.alignment(&character.id).expect("every character has one");
                for value in [alignment.law(), alignment.good()] {
                    prop_assert!((-AXIS_LIMIT..=AXIS_LIMIT).contains(&value));
                }
            }
        }

        #[test]
        fn every_alignment_change_follows_the_act_that_caused_it(commands in commands()) {
            let world = run(&commands);
            for pair in world.events().windows(2) {
                if let Change::AlignmentChanged { character, from, to } = &pair[1].payload {
                    prop_assert_ne!(from, to);
                    let Change::ActionPerformed { actor, .. } = &pair[0].payload else {
                        return Err(TestCaseError::fail("an alignment change without an act"));
                    };
                    prop_assert_eq!(actor, character);
                }
            }
        }
    }
}
