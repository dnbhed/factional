use std::collections::BTreeMap;
use std::fmt;

use factional_core::{Curve, CurveError, Fixed, Tick, article, suggest};

use crate::defection::{self, Situation};
use crate::distance::gap;
use crate::inertia::shifts;
use crate::{
    AXIS_LIMIT, Action, ActionId, Alignment, AlignmentDelta, Axis, Bands, Change, Character,
    CharacterId, Command, CommandError, Component, ComponentKind, Defection, Disposition,
    DispositionWeights, Effects, Event, Faction, FactionId, Inertia, InertiaProfile,
    JoinAssessment, JoinBlock, JournalEntry, LeaveReason, Membership, Metric, Outcome, OutcomeId,
    Part, Party, ProfileId, PromotionAssessment, RankCheck, RankId, Regard, Relation, RelationSide,
    Role, Rule, Shift, StandingEffects, StandingKey, StandingOwner, TableKind, TableOwner,
    TableProblem, TableSource, Toward, Verdict, Weights, Witnesses, measure,
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
}

impl Balance {
    /// `disposition.same_faction`'s default: 50.00.
    pub const DEFAULT_SAME_FACTION: Fixed = Fixed::from_hundredths(50_00);

    /// `relations.conflict_threshold`'s default: −50.00.
    pub const DEFAULT_CONFLICT_THRESHOLD: Fixed = Fixed::from_hundredths(-50_00);

    /// `alignment.label_threshold`'s default: 33.00.
    pub const DEFAULT_LABEL_THRESHOLD: Fixed = Fixed::from_hundredths(33_00);

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
    /// A `disposition.weights` entry below 0.
    NegativeDispositionWeight {
        component: ComponentKind,
        value: Fixed,
    },
    /// `disposition.same_faction` outside −100…100.
    SameFactionOutOfRange(Fixed),
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
            .chain(threshold)
            .chain(self.inertia_problems())
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

    /// `disposition.weights` must each be at least 0, and `same_faction` within ±100.
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
        problems
    }

    /// Problems with `leave_standing_change` and every `standing` block: each party must
    /// exist, and each value must be within ±100. Factions, then characters, actions and
    /// outcomes, each in id order.
    fn standing_problems(&self) -> Vec<ContentProblem> {
        let mut problems: Vec<ContentProblem> = self
            .factions
            .values()
            .filter(|faction| !within_range(faction.leave_standing_change))
            .map(|faction| ContentProblem::LeaveStandingOutOfRange {
                faction: faction.id.clone(),
                value: faction.leave_standing_change,
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
    /// memberships, then each faction's ranks.
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
            ContentProblem::AffinityOutOfRange(error) => error.fmt(f),
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
            ContentProblem::NegativeDispositionWeight { value, .. } => {
                write!(f, "{value} must be at least {}", Fixed::ZERO)
            }
            ContentProblem::StandingOutOfRange { value, .. }
            | ContentProblem::LeaveStandingOutOfRange { value, .. }
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
        }
    }
}

/// Who is doing the judging: a faction, or a character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observer {
    Faction(FactionId),
    Character(CharacterId),
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

    /// Runs a command. Refused: nothing changes, and the error says why. Accepted: the events
    /// it produced, already applied. Either way the journal records it.
    pub fn execute(&mut self, command: Command) -> Result<Vec<Event>, CommandError> {
        let decided = self.decide(&command);
        self.journal.push(JournalEntry {
            command,
            result: decided.as_ref().map(|_| ()).map_err(Clone::clone),
        });
        let mut emitted = Vec::new();
        for change in decided? {
            let event = Event {
                seq: self.events.len() as u64 + 1,
                tick: self.state.now,
                payload: change,
            };
            self.apply(event.clone());
            emitted.push(event);
        }
        Ok(emitted)
    }

    /// Works out what a command would change, without changing anything: the rules live
    /// here, and only here.
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
                let to = from.shifted(catalogued.alignment, *scale, self.inertia_of(actor).1);
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
        deltas
            .into_iter()
            .filter_map(|(party, delta)| {
                let change = delta.saturating_mul(self.awareness(&party));
                let before = self.standing_now(subject, &party);
                // A sum too big to hold means a change that reaches the end by itself.
                let after = before
                    .checked_add(change)
                    .unwrap_or(change)
                    .clamp(-AXIS_LIMIT, AXIS_LIMIT);
                (after != before).then(|| Change::StandingChanged {
                    subject: subject.clone(),
                    party,
                    before,
                    after,
                })
            })
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
    /// works out from what's there now. Refused if it would put two of anyone's factions in
    /// conflict: until M9 can resolve that, invariant 6 holds by refusal.
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
        let threshold = self.content.balance.conflict_threshold;
        for (character, factions) in &self.state.memberships {
            let factions: Vec<&FactionId> = factions.keys().collect();
            for (i, a) in factions.iter().enumerate() {
                for b in &factions[i + 1..] {
                    if hostility(&relations, a, b) <= threshold {
                        return Err(CommandError::WouldPutInConflict {
                            character: character.clone(),
                            factions: ((*a).clone(), (*b).clone()),
                        });
                    }
                }
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
            axes: shifts(from, delta, scale, inertia),
        })
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
        let rank_tolerance = current.ranks[position].tolerance;
        let member_tolerance = current.tolerances.member();
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
            member_tolerance: rank_tolerance
                .map_or(member_tolerance, |rank| rank.min(member_tolerance)),
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
        let components: Vec<Component> = ComponentKind::ALL
            .into_iter()
            .map(|kind| {
                let (value, parts) = match kind {
                    ComponentKind::Affinity => (affinity, Vec::new()),
                    ComponentKind::Standing => (standing, Vec::new()),
                    ComponentKind::Kinship => (sum(&kinship), kinship.clone()),
                    ComponentKind::FactionOpinion => (sum(&opinion), opinion.clone()),
                    ComponentKind::Modifiers => (Fixed::ZERO, Vec::new()),
                };
                let weight = balance.disposition_weights.get(kind);
                Component {
                    kind,
                    value,
                    weight,
                    weighted: weight * value,
                    parts,
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

    /// How far `subject` is from `observer`, as the observer sees it: measured with the
    /// observer's weights and the world's metric (DESIGN.md §6). `None` if either is unknown.
    pub fn distance(&self, observer: &Observer, subject: &CharacterId) -> Option<Distance> {
        let (from, own_weights) = match observer {
            Observer::Faction(id) => {
                let faction = self.faction(id)?;
                (faction.alignment, faction.weights)
            }
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
        AXIS_LIMIT, ActionStanding, AlignmentDelta, AxisShift, Band, Condition, ConditionCheck,
        Effects, JoinBlock, LeaveReason, Observed, Part, Rank, RankCheck, RankRef, RelationEnds,
        Role, StandingEffects, StartingMembership, Tolerances, Verdict, Witnesses,
    };
    use factional_core::Ratio;

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
        // A reformed Vex at 35 / 10 is 95.52 from the guild, beyond its 60.00.
        let reformed = content(35_00, 10_00);
        let warnings = reformed.warnings();
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
        assert_eq!(content(-55_00, -20_00).warnings(), [], "7.07 away");
        // Exactly 60.00 away on the law axis is still within.
        assert_eq!(content(0, -10_00).warnings(), []);
        assert_eq!(content(1, -10_00).warnings().len(), 1);
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
    fn a_relation_change_cannot_put_two_of_a_characters_factions_at_war() {
        let mut world = riverhold();
        steal_times(&mut world, 4);
        world
            .execute(join("player", "lantern_guild"))
            .expect("accepted");
        world
            .execute(join("player", "free_company"))
            .expect("accepted");
        let refusal = refused(
            &mut world,
            set("lantern_guild", "free_company", -60_00, false),
        );
        assert_eq!(
            refusal,
            CommandError::WouldPutInConflict {
                character: id("player"),
                factions: (faction_id("free_company"), faction_id("lantern_guild")),
            }
        );
        assert_eq!(
            refusal.to_string(),
            "that would put two of player's factions in conflict: free_company and lantern_guild"
        );
        assert_eq!(
            refused(
                &mut world,
                shift("free_company", "lantern_guild", -70_00, false)
            ),
            refusal
        );
        // Short of conflict is fine.
        assert!(
            world
                .execute(set("lantern_guild", "free_company", -49_99, true))
                .is_ok()
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

    fn standing_changed(seq: u64, subject: &str, party: Party, before: i64, after: i64) -> Event {
        Event {
            seq,
            tick: Tick(0),
            payload: Change::StandingChanged {
                subject: id(subject),
                party,
                before: h(before),
                after: h(after),
            },
        }
    }

    /// Riverhold's people, with Hale's starting standing, and its two outcomes.
    fn with_outcomes() -> World {
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
        World::new(Content {
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
        })
        .expect("valid content")
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
                standing_changed(3, "player", party_faction("lantern_guild"), 0, -10_00),
                standing_changed(4, "player", party_character("vex"), 0, -20_00),
            ]
        );
    }

    #[test]
    fn an_action_without_a_target_changes_only_named_standing() {
        let mut world = with_outcomes();
        let events = world
            .execute(act("player", "donate_to_temple", None, 1_00))
            .expect("accepted");
        assert_eq!(
            events[2..],
            [standing_changed(
                3,
                "player",
                party_faction("temple"),
                0,
                10_00
            )]
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
                standing_changed(3, "player", party_character("captain_hale"), 0, -10_00),
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
                standing_changed(6, "player", party_faction("city_watch"), -20_00, -10_00),
                standing_changed(7, "player", party_character("ava"), 0, 30_00),
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
            [standing_changed(
                17,
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
            Some(vec![
                (party_faction("city_watch"), h(10_00)),
                (party_faction("lantern_guild"), h(-10_00)),
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
                standing_changed(2, "player", party_faction("temple"), 0, 25_00),
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
                seq: 3,
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
        let command = prop_oneof![
            (0_u64..=1_000).prop_map(advance),
            action,
            membership,
            relate,
            effects
        ];
        proptest::collection::vec(command, 0..30)
    }

    /// Riverhold with the sample rule tables, and Nell, a reformed Guild member who can defect
    /// to the Watch.
    fn run(commands: &[Command]) -> World {
        let mut world = defectors_world([
            character("Vex", -55_00, -20_00),
            character("Ava", 20_00, 10_00),
            character("Player", 0, 0),
            member_of(character("Nell", 35_00, 10_00), &["lantern_guild"]),
        ]);
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

        /// DESIGN.md §14, invariant 6 (until M9 adds `MembershipConflict`).
        #[test]
        fn no_one_is_ever_in_two_factions_in_conflict(commands in commands()) {
            let world = run(&commands);
            for character in world.characters() {
                let factions: Vec<&FactionId> = world
                    .memberships(&character.id)
                    .expect("the character exists")
                    .map(|(faction, _)| faction)
                    .collect();
                for (i, a) in factions.iter().enumerate() {
                    for b in &factions[i + 1..] {
                        prop_assert!(!world.in_conflict(a, b).expect("both exist"));
                    }
                }
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
