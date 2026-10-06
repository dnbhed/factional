use std::fmt;

use factional_core::{Fixed, suggest};

use crate::{AXIS_LIMIT, Faction, FactionId, RankId};

/// The two rule tables that settle joining an enemy of a faction you're in (DESIGN.md §9.2,
/// P-10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TableKind {
    /// The target's: will it take someone from its enemy?
    Defectors,
    /// The current faction's: will it let them go?
    Deserters,
}

impl TableKind {
    pub const ALL: [TableKind; 2] = [TableKind::Defectors, TableKind::Deserters];

    /// Its key in content: `defectors` or `deserters`.
    pub fn key(self) -> &'static str {
        match self {
            TableKind::Defectors => "defectors",
            TableKind::Deserters => "deserters",
        }
    }

    /// The outcome that lets someone through: `accept` for defectors, `release` for
    /// deserters.
    pub fn allow_key(self) -> &'static str {
        match self {
            TableKind::Defectors => "accept",
            TableKind::Deserters => "release",
        }
    }

    /// The table every world starts with: defectors refuses everyone and deserters
    /// releases everyone, which is exactly D-4.
    pub fn built_in(self) -> Vec<Rule> {
        let then = match self {
            TableKind::Defectors => Verdict::Refuse { reason: None },
            TableKind::Deserters => Verdict::Allow {
                standing_change: Fixed::ZERO,
            },
        };
        vec![Rule {
            when: Vec::new(),
            then,
        }]
    }
}

impl fmt::Display for TableKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key())
    }
}

/// A rung named in a rank condition: a number, 1 for the lowest, or in a faction's own
/// tables one of its rank ids, which stands for that rank's rung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RankRef {
    Rung(i64),
    Id(RankId),
}

/// One condition of a rule, from a fixed vocabulary (DESIGN.md §9.2). "Current" is the
/// faction being left and "target" the one being joined, in either table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Condition {
    /// The member's rung in the current faction is at least this.
    RankAtLeast(RankRef),
    /// The member's rung in the current faction is below this.
    RankBelow(RankRef),
    StandingWithCurrentAtLeast(Fixed),
    StandingWithCurrentBelow(Fixed),
    StandingWithTargetAtLeast(Fixed),
    StandingWithTargetBelow(Fixed),
    /// `true`: nearer the target than the current faction, each measured with its own
    /// weights. `false`: not nearer.
    CloserToTarget(bool),
    /// `true`: further from the current faction than the member's tolerance there. `false`:
    /// within it.
    OutsideMemberTolerance(bool),
}

impl Condition {
    /// Every condition's key, in the order content lists them.
    pub const KEYS: [&'static str; 8] = [
        "rank_at_least",
        "rank_below",
        "standing_with_current_at_least",
        "standing_with_current_below",
        "standing_with_target_at_least",
        "standing_with_target_below",
        "closer_to_target",
        "outside_member_tolerance",
    ];

    /// Its key in content, such as `rank_at_least`.
    pub fn key(&self) -> &'static str {
        let index = match self {
            Condition::RankAtLeast(_) => 0,
            Condition::RankBelow(_) => 1,
            Condition::StandingWithCurrentAtLeast(_) => 2,
            Condition::StandingWithCurrentBelow(_) => 3,
            Condition::StandingWithTargetAtLeast(_) => 4,
            Condition::StandingWithTargetBelow(_) => 5,
            Condition::CloserToTarget(_) => 6,
            Condition::OutsideMemberTolerance(_) => 7,
        };
        Condition::KEYS[index]
    }
}

/// What a rule decides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// `accept` or `release`, changing the character's standing with the table's faction by
    /// `standing_change` (0 if content leaves it out).
    Allow { standing_change: Fixed },
    /// `refuse`; content always gives a reason, the built-in table none.
    Refuse { reason: Option<String> },
}

impl Verdict {
    pub fn allows(&self) -> bool {
        matches!(self, Verdict::Allow { .. })
    }
}

/// One rule: if every condition holds (`when` may be empty), `then` decides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub when: Vec<Condition>,
    pub then: Verdict,
}

/// Where the table a faction used came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableSource {
    /// The faction's own, in `factions.toml`.
    Faction,
    /// The world's, in `balance.toml`.
    World,
    BuiltIn,
}

/// What a condition was checked against, for explaining it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observed {
    /// The member's rank in the current faction and its rung, against the rung the
    /// condition names.
    Rank {
        rank: RankId,
        rung: usize,
        named: usize,
    },
    /// The standing of the faction the condition is about.
    Standing(Fixed),
    /// Distance to the target and to the current faction.
    Distances { target: Fixed, current: Fixed },
    /// Distance to the current faction and the member's tolerance there.
    Tolerance { distance: Fixed, tolerance: Fixed },
}

/// One condition, what it was checked against, and whether it held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConditionCheck {
    pub condition: Condition,
    pub observed: Observed,
    pub held: bool,
}

/// One rule tried: its place in the table, from 0, and each of its conditions checked. It
/// fired if every one held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleTried {
    pub index: usize,
    pub checks: Vec<ConditionCheck>,
}

/// How one faction's table decided: every rule tried, the last being the one that fired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableDecision {
    pub kind: TableKind,
    /// The faction whose table it is: the target's for defectors, the current one's for
    /// deserters.
    pub faction: FactionId,
    pub faction_name: String,
    pub source: TableSource,
    pub tried: Vec<RuleTried>,
    pub verdict: Verdict,
}

impl TableDecision {
    pub fn allows(&self) -> bool {
        self.verdict.allows()
    }

    /// The rule that fired, from 0.
    pub fn fired(&self) -> usize {
        self.tried.last().map_or(0, |rule| rule.index)
    }

    /// The rule that fired, in words: `The Lantern Guild's deserters rule 1`, or `the
    /// built-in defectors rule`.
    pub fn rule_name(&self) -> String {
        match self.source {
            TableSource::BuiltIn => format!("the built-in {} rule", self.kind),
            TableSource::Faction | TableSource::World => format!(
                "{}'s {} rule {}",
                self.faction_name,
                self.kind,
                self.fired() + 1
            ),
        }
    }
}

/// Leaving one current faction, in conflict with the target, to join the target: both
/// tables' decisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Defection {
    pub from: FactionId,
    pub from_name: String,
    /// The more hostile of the two directions between the factions.
    pub relation: Fixed,
    pub deserters: TableDecision,
    pub defectors: TableDecision,
}

impl Defection {
    pub fn allowed(&self) -> bool {
        self.deserters.allows() && self.defectors.allows()
    }
}

/// Everything a table's conditions are checked against, for one member leaving one faction
/// for another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Situation {
    /// Their rank in the current faction and its rung, 1 for the lowest.
    pub rank: RankId,
    pub rung: usize,
    pub standing_with_current: Fixed,
    pub standing_with_target: Fixed,
    pub distance_to_target: Fixed,
    pub distance_to_current: Fixed,
    /// Their tolerance in the current faction: the stricter of their rank's and the
    /// faction's member tolerance.
    pub member_tolerance: Fixed,
}

/// Tries `rules` top to bottom for `owner`, the faction whose table it is, until one whose
/// conditions all hold decides. Content checks that every table's last rule always
/// decides, and that rank ids name `owner`'s ranks (P-32).
pub(crate) fn decide(
    kind: TableKind,
    rules: &[Rule],
    source: TableSource,
    owner: &Faction,
    situation: &Situation,
) -> TableDecision {
    let mut tried = Vec::new();
    let mut verdict = Verdict::Refuse { reason: None };
    for (index, rule) in rules.iter().enumerate() {
        let checks: Vec<ConditionCheck> = rule
            .when
            .iter()
            .map(|condition| check(condition, owner, situation))
            .collect();
        let fired = checks.iter().all(|check| check.held);
        tried.push(RuleTried { index, checks });
        if fired {
            verdict = rule.then.clone();
            break;
        }
    }
    TableDecision {
        kind,
        faction: owner.id.clone(),
        faction_name: owner.name.clone(),
        source,
        tried,
        verdict,
    }
}

/// Checks one condition against `situation`; a rank id names a rung on `owner`'s ladder.
fn check(condition: &Condition, owner: &Faction, situation: &Situation) -> ConditionCheck {
    let rung = |named: &RankRef| match named {
        RankRef::Rung(rung) => usize::try_from(*rung).unwrap_or(0),
        RankRef::Id(id) => {
            owner
                .rank_position(id)
                .expect("content checks a table's rank ids are its faction's")
                + 1
        }
    };
    let rank = |named: &RankRef| Observed::Rank {
        rank: situation.rank.clone(),
        rung: situation.rung,
        named: rung(named),
    };
    let distances = Observed::Distances {
        target: situation.distance_to_target,
        current: situation.distance_to_current,
    };
    let (observed, held) = match condition {
        Condition::RankAtLeast(named) => (rank(named), situation.rung >= rung(named)),
        Condition::RankBelow(named) => (rank(named), situation.rung < rung(named)),
        Condition::StandingWithCurrentAtLeast(value) => (
            Observed::Standing(situation.standing_with_current),
            situation.standing_with_current >= *value,
        ),
        Condition::StandingWithCurrentBelow(value) => (
            Observed::Standing(situation.standing_with_current),
            situation.standing_with_current < *value,
        ),
        Condition::StandingWithTargetAtLeast(value) => (
            Observed::Standing(situation.standing_with_target),
            situation.standing_with_target >= *value,
        ),
        Condition::StandingWithTargetBelow(value) => (
            Observed::Standing(situation.standing_with_target),
            situation.standing_with_target < *value,
        ),
        Condition::CloserToTarget(closer) => (
            distances,
            (situation.distance_to_target < situation.distance_to_current) == *closer,
        ),
        Condition::OutsideMemberTolerance(outside) => (
            Observed::Tolerance {
                distance: situation.distance_to_current,
                tolerance: situation.member_tolerance,
            },
            (situation.distance_to_current > situation.member_tolerance) == *outside,
        ),
    };
    ConditionCheck {
        condition: condition.clone(),
        observed,
        held,
    }
}

/// Whose rule table a content problem is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableOwner {
    /// `balance.toml`'s `membership` tables.
    World,
    /// A faction's own, in `factions.toml`.
    Faction(FactionId),
}

/// Something wrong with a rule table (DESIGN.md §12.2). `rule` is the rule's place, from 0;
/// `condition` its key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableProblem {
    /// A rung number below 1.
    RungBelowOne {
        rule: usize,
        condition: &'static str,
        rung: i64,
    },
    /// A rank id in the world's table, which belongs to no faction's ladder.
    RankInWorldTable {
        rule: usize,
        condition: &'static str,
        rank: RankId,
    },
    /// A rank id that isn't on the table's own faction's ladder.
    UnknownRank {
        rule: usize,
        condition: &'static str,
        rank: RankId,
        suggestion: Option<RankId>,
    },
    /// A standing threshold or `standing_change` outside −100…100; `key` is the condition's
    /// key or `standing_change`.
    ValueOutOfRange {
        rule: usize,
        key: &'static str,
        value: Fixed,
    },
    /// The last rule has conditions, or there are no rules, so the table might not decide.
    MightNotDecide { rules: usize },
}

/// Every problem with one table, in rule order (DESIGN.md §12.2). `owner` is the faction
/// whose own table it is, whose rank ids it may name; `None` for the world's.
pub(crate) fn problems(rules: &[Rule], owner: Option<&Faction>) -> Vec<TableProblem> {
    let mut problems = Vec::new();
    for (index, rule) in rules.iter().enumerate() {
        for condition in &rule.when {
            let key = condition.key();
            match condition {
                Condition::RankAtLeast(named) | Condition::RankBelow(named) => match named {
                    RankRef::Rung(rung) if *rung < 1 => {
                        problems.push(TableProblem::RungBelowOne {
                            rule: index,
                            condition: key,
                            rung: *rung,
                        });
                    }
                    RankRef::Rung(_) => {}
                    RankRef::Id(rank) => match owner {
                        None => problems.push(TableProblem::RankInWorldTable {
                            rule: index,
                            condition: key,
                            rank: rank.clone(),
                        }),
                        Some(faction) if faction.rank_position(rank).is_none() => {
                            let ladder = faction.ranks.iter().map(|rung| rung.id.as_str());
                            let suggestion = suggest(rank.as_str(), ladder)
                                .map(|close| RankId::new(close).expect("a rank id"));
                            problems.push(TableProblem::UnknownRank {
                                rule: index,
                                condition: key,
                                rank: rank.clone(),
                                suggestion,
                            });
                        }
                        Some(_) => {}
                    },
                },
                Condition::StandingWithCurrentAtLeast(value)
                | Condition::StandingWithCurrentBelow(value)
                | Condition::StandingWithTargetAtLeast(value)
                | Condition::StandingWithTargetBelow(value) => {
                    if !(-AXIS_LIMIT..=AXIS_LIMIT).contains(value) {
                        problems.push(TableProblem::ValueOutOfRange {
                            rule: index,
                            key,
                            value: *value,
                        });
                    }
                }
                Condition::CloserToTarget(_) | Condition::OutsideMemberTolerance(_) => {}
            }
        }
        if let Verdict::Allow { standing_change } = rule.then
            && !(-AXIS_LIMIT..=AXIS_LIMIT).contains(&standing_change)
        {
            problems.push(TableProblem::ValueOutOfRange {
                rule: index,
                key: "standing_change",
                value: standing_change,
            });
        }
    }
    if rules.last().is_none_or(|last| !last.when.is_empty()) {
        problems.push(TableProblem::MightNotDecide { rules: rules.len() });
    }
    problems
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::{Alignment, Rank, Tolerances};

    const fn h(hundredths: i64) -> Fixed {
        Fixed::from_hundredths(hundredths)
    }

    fn rank(id: &str) -> RankId {
        RankId::new(id).expect("valid")
    }

    fn guild() -> Faction {
        let rung = |id: &str| Rank {
            id: rank(id),
            requires_standing: None,
            tolerance: None,
        };
        Faction {
            id: FactionId::new("lantern_guild").expect("valid"),
            name: "The Lantern Guild".to_owned(),
            alignment: Alignment::new(h(-60_00), h(-10_00)).expect("in range"),
            weights: None,
            tolerances: Tolerances::new(h(45_00), Some(h(60_00))).expect("valid"),
            leave_standing_change: Fixed::ZERO,
            ranks: vec![rung("cutpurse"), rung("fence"), rung("shadow")],
            rule_tables: BTreeMap::new(),
            drift: None,
            expel_standing_change: Faction::DEFAULT_EXPEL_STANDING_CHANGE,
        }
    }

    /// Vex, reformed to 35 / 10, a fence of the Lantern Guild (DESIGN.md §9.2).
    fn vex() -> Situation {
        Situation {
            rank: rank("fence"),
            rung: 2,
            standing_with_current: h(30_00),
            standing_with_target: Fixed::ZERO,
            distance_to_target: h(35_09),
            distance_to_current: h(95_52),
            member_tolerance: h(60_00),
        }
    }

    fn rule(when: Vec<Condition>, then: Verdict) -> Rule {
        Rule { when, then }
    }

    fn release(standing_change: i64) -> Verdict {
        Verdict::Allow {
            standing_change: h(standing_change),
        }
    }

    fn refuse(reason: &str) -> Verdict {
        Verdict::Refuse {
            reason: Some(reason.to_owned()),
        }
    }

    /// The sample `deserters` table (DESIGN.md §9.2).
    fn deserters() -> Vec<Rule> {
        vec![
            rule(
                vec![Condition::RankAtLeast(RankRef::Rung(3))],
                refuse("Officers don't walk away."),
            ),
            rule(vec![Condition::OutsideMemberTolerance(true)], release(0)),
            rule(Vec::new(), release(-40_00)),
        ]
    }

    fn decided(rules: &[Rule], situation: &Situation) -> TableDecision {
        decide(
            TableKind::Deserters,
            rules,
            TableSource::World,
            &guild(),
            situation,
        )
    }

    /// Whether the table's only rule, `when = { <condition> }`, held for `situation`.
    fn holds(condition: Condition, situation: &Situation) -> bool {
        let rules = [
            rule(vec![condition], release(0)),
            rule(Vec::new(), refuse("no")),
        ];
        decided(&rules, situation).fired() == 0
    }

    #[test]
    fn the_first_rule_whose_conditions_all_hold_decides() {
        let decision = decided(&deserters(), &vex());
        assert_eq!(
            decision,
            TableDecision {
                kind: TableKind::Deserters,
                faction: guild().id,
                faction_name: "The Lantern Guild".to_owned(),
                source: TableSource::World,
                tried: vec![
                    RuleTried {
                        index: 0,
                        checks: vec![ConditionCheck {
                            condition: Condition::RankAtLeast(RankRef::Rung(3)),
                            observed: Observed::Rank {
                                rank: rank("fence"),
                                rung: 2,
                                named: 3
                            },
                            held: false,
                        }],
                    },
                    RuleTried {
                        index: 1,
                        checks: vec![ConditionCheck {
                            condition: Condition::OutsideMemberTolerance(true),
                            observed: Observed::Tolerance {
                                distance: h(95_52),
                                tolerance: h(60_00)
                            },
                            held: true,
                        }],
                    },
                ],
                verdict: release(0),
            }
        );
        assert_eq!(decision.rule_name(), "The Lantern Guild's deserters rule 2");
    }

    #[test]
    fn a_shadow_is_refused_by_the_first_rule() {
        let shadow = Situation {
            rank: rank("shadow"),
            rung: 3,
            ..vex()
        };
        let decision = decided(&deserters(), &shadow);
        assert_eq!(
            (decision.fired(), decision.verdict),
            (0, refuse("Officers don't walk away."))
        );
    }

    #[test]
    fn every_condition_in_a_rule_must_hold() {
        let rules = [
            rule(
                vec![
                    Condition::OutsideMemberTolerance(true),
                    Condition::StandingWithCurrentAtLeast(h(50_00)),
                ],
                release(0),
            ),
            rule(Vec::new(), release(-40_00)),
        ];
        let decision = decided(&rules, &vex());
        assert_eq!(decision.fired(), 1);
        let held: Vec<bool> = decision.tried[0].checks.iter().map(|c| c.held).collect();
        assert_eq!(held, [true, false]);
    }

    #[test]
    fn rank_conditions_compare_rungs_at_least_inclusively_and_below_strictly() {
        let at = |rung: usize| Situation { rung, ..vex() };
        assert!(holds(Condition::RankAtLeast(RankRef::Rung(2)), &at(2)));
        assert!(!holds(Condition::RankAtLeast(RankRef::Rung(3)), &at(2)));
        assert!(holds(Condition::RankBelow(RankRef::Rung(3)), &at(2)));
        assert!(!holds(Condition::RankBelow(RankRef::Rung(2)), &at(2)));
    }

    #[test]
    fn a_rank_id_stands_for_its_rung_on_the_tables_faction() {
        let rules = [
            rule(
                vec![Condition::RankAtLeast(RankRef::Id(rank("shadow")))],
                refuse("no"),
            ),
            rule(Vec::new(), release(0)),
        ];
        let decision = decided(&rules, &vex());
        assert_eq!(
            decision.tried[0].checks[0].observed,
            Observed::Rank {
                rank: rank("fence"),
                rung: 2,
                named: 3
            }
        );
        assert!(holds(
            Condition::RankBelow(RankRef::Id(rank("shadow"))),
            &vex()
        ));
        assert!(holds(
            Condition::RankAtLeast(RankRef::Id(rank("fence"))),
            &vex()
        ));
    }

    #[test]
    fn standing_conditions_are_at_least_inclusively_and_below_strictly() {
        let with = |current: i64, target: i64| Situation {
            standing_with_current: h(current),
            standing_with_target: h(target),
            ..vex()
        };
        let fifty = h(50_00);
        assert!(holds(
            Condition::StandingWithTargetAtLeast(fifty),
            &with(0, 50_00)
        ));
        assert!(!holds(
            Condition::StandingWithTargetAtLeast(fifty),
            &with(0, 49_99)
        ));
        assert!(holds(
            Condition::StandingWithTargetBelow(fifty),
            &with(0, 49_99)
        ));
        assert!(!holds(
            Condition::StandingWithTargetBelow(fifty),
            &with(0, 50_00)
        ));
        assert!(holds(
            Condition::StandingWithCurrentAtLeast(fifty),
            &with(50_00, 0)
        ));
        assert!(!holds(
            Condition::StandingWithCurrentAtLeast(fifty),
            &with(49_99, 0)
        ));
        assert!(holds(
            Condition::StandingWithCurrentBelow(fifty),
            &with(49_99, 0)
        ));
        assert!(!holds(
            Condition::StandingWithCurrentBelow(fifty),
            &with(50_00, 0)
        ));
        let rules = [
            rule(
                vec![Condition::StandingWithTargetAtLeast(fifty)],
                release(0),
            ),
            rule(vec![Condition::StandingWithCurrentBelow(fifty)], release(0)),
            rule(Vec::new(), release(0)),
        ];
        let decision = decided(&rules, &with(30_00, 10_00));
        let observed: Vec<&Observed> = decision
            .tried
            .iter()
            .map(|r| &r.checks[0].observed)
            .collect();
        assert_eq!(
            observed,
            [&Observed::Standing(h(10_00)), &Observed::Standing(h(30_00))]
        );
    }

    #[test]
    fn closer_to_target_means_strictly_nearer_the_target() {
        let apart = |target: i64, current: i64| Situation {
            distance_to_target: h(target),
            distance_to_current: h(current),
            ..vex()
        };
        assert!(holds(Condition::CloserToTarget(true), &apart(35_09, 95_52)));
        assert!(!holds(
            Condition::CloserToTarget(true),
            &apart(40_00, 40_00)
        ));
        assert!(holds(
            Condition::CloserToTarget(false),
            &apart(40_00, 40_00)
        ));
        assert!(!holds(
            Condition::CloserToTarget(false),
            &apart(35_09, 95_52)
        ));
        let rules = [
            rule(vec![Condition::CloserToTarget(true)], release(0)),
            rule(Vec::new(), release(0)),
        ];
        assert_eq!(
            decided(&rules, &vex()).tried[0].checks[0].observed,
            Observed::Distances {
                target: h(35_09),
                current: h(95_52)
            }
        );
    }

    #[test]
    fn exactly_at_the_member_tolerance_is_still_within_it() {
        let at = |distance: i64| Situation {
            distance_to_current: h(distance),
            ..vex()
        };
        assert!(!holds(Condition::OutsideMemberTolerance(true), &at(60_00)));
        assert!(holds(Condition::OutsideMemberTolerance(true), &at(60_01)));
        assert!(holds(Condition::OutsideMemberTolerance(false), &at(60_00)));
        assert!(!holds(Condition::OutsideMemberTolerance(false), &at(60_01)));
    }

    #[test]
    fn the_built_in_tables_refuse_every_defector_and_release_every_deserter() {
        let built_in = |kind: TableKind| {
            decide(
                kind,
                &kind.built_in(),
                TableSource::BuiltIn,
                &guild(),
                &vex(),
            )
        };
        let defectors = built_in(TableKind::Defectors);
        assert_eq!(defectors.verdict, Verdict::Refuse { reason: None });
        assert_eq!(defectors.rule_name(), "the built-in defectors rule");
        assert_eq!(
            built_in(TableKind::Deserters).verdict,
            Verdict::Allow {
                standing_change: Fixed::ZERO
            }
        );
    }

    #[test]
    fn condition_keys_are_content_keys() {
        let keys: Vec<&str> = [
            Condition::RankAtLeast(RankRef::Rung(1)),
            Condition::RankBelow(RankRef::Rung(1)),
            Condition::StandingWithCurrentAtLeast(Fixed::ZERO),
            Condition::StandingWithCurrentBelow(Fixed::ZERO),
            Condition::StandingWithTargetAtLeast(Fixed::ZERO),
            Condition::StandingWithTargetBelow(Fixed::ZERO),
            Condition::CloserToTarget(true),
            Condition::OutsideMemberTolerance(true),
        ]
        .iter()
        .map(Condition::key)
        .collect();
        assert_eq!(keys, Condition::KEYS);
        assert_eq!(
            TableKind::ALL.map(|kind| (kind.key(), kind.allow_key())),
            [("defectors", "accept"), ("deserters", "release")]
        );
    }
}
