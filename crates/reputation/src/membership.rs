use std::fmt;

use factional_core::{Fixed, Tick};

use crate::{CharacterId, Defection, Distance, FactionId, RankId, Verdict};

/// How close to a faction's alignment someone must be to join it, and to stay in it
/// (DESIGN.md §9.1). Staying is never harder than joining: easier to stay than to get in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tolerances {
    tolerance: Fixed,
    member: Fixed,
}

/// Why two numbers can't be a faction's tolerances.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToleranceProblem {
    /// `tolerance` is below 0.
    Negative(Fixed),
    /// `member_tolerance` is below `tolerance`.
    MemberBelowTolerance { member: Fixed, tolerance: Fixed },
}

impl Tolerances {
    /// Tolerances, if 0 ≤ `tolerance` ≤ `member`; `member` defaults to `tolerance`.
    pub fn new(tolerance: Fixed, member: Option<Fixed>) -> Result<Tolerances, ToleranceProblem> {
        let member = member.unwrap_or(tolerance);
        if tolerance < Fixed::ZERO {
            Err(ToleranceProblem::Negative(tolerance))
        } else if member < tolerance {
            Err(ToleranceProblem::MemberBelowTolerance { member, tolerance })
        } else {
            Ok(Tolerances { tolerance, member })
        }
    }

    /// The furthest a character may be from the faction and still join.
    pub fn tolerance(self) -> Fixed {
        self.tolerance
    }

    /// The furthest a member may drift before the faction's drift policy applies (M8).
    pub fn member(self) -> Fixed {
        self.member
    }
}

impl fmt::Display for ToleranceProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToleranceProblem::Negative(value) => {
                write!(f, "{value} must be at least {}", Fixed::ZERO)
            }
            ToleranceProblem::MemberBelowTolerance { member, tolerance } => write!(
                f,
                "{member} must be at least the faction's tolerance, {tolerance}"
            ),
        }
    }
}

/// A character's place in a faction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Membership {
    /// When they joined; tick 0 for a starting member.
    pub since: Tick,
    /// Their rung on the faction's ladder.
    pub rank: RankId,
}

/// A faction a character starts in, as `characters.toml` lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartingMembership {
    pub faction: FactionId,
    /// Their starting rank; `None` means the lowest rung.
    pub rank: Option<RankId>,
}

/// Why a character left a faction. Conflict resolution arrives with M9.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaveReason {
    Voluntary,
    /// To join an enemy of the faction (DESIGN.md §9.2).
    Defected,
    /// Thrown out for drifting from the faction's ideals (DESIGN.md §9.3).
    Expelled,
}

/// What a faction does about a member who drifts past their tolerance (DESIGN.md §9.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriftPolicy {
    Ignore,
    /// Report it, and report when they're back.
    Flag,
    /// Down a rung at a time until they're within the rank's tolerance; expelled if they're
    /// still out on the lowest rung.
    Demote,
    Expel,
}

impl DriftPolicy {
    pub const ALL: [DriftPolicy; 4] = [
        DriftPolicy::Ignore,
        DriftPolicy::Flag,
        DriftPolicy::Demote,
        DriftPolicy::Expel,
    ];

    /// Its name in content, such as `flag`.
    pub fn key(self) -> &'static str {
        match self {
            DriftPolicy::Ignore => "ignore",
            DriftPolicy::Flag => "flag",
            DriftPolicy::Demote => "demote",
            DriftPolicy::Expel => "expel",
        }
    }

    pub fn from_key(key: &str) -> Option<DriftPolicy> {
        DriftPolicy::ALL
            .into_iter()
            .find(|policy| policy.key() == key)
    }
}

/// One requirement of the next rank (DESIGN.md §7.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RankCheck {
    /// Standing with the faction at or above `required`.
    Standing { required: Fixed, has: Fixed },
    /// Distance from the faction within the rank's own `limit`.
    Tolerance { limit: Fixed, distance: Fixed },
}

impl RankCheck {
    pub fn met(self) -> bool {
        match self {
            RankCheck::Standing { required, has } => has >= required,
            RankCheck::Tolerance { limit, distance } => distance <= limit,
        }
    }
}

/// Whether a member may be promoted now, and every requirement of the next rank (P-24).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionAssessment {
    pub character: CharacterId,
    pub faction: FactionId,
    pub faction_name: String,
    pub current: RankId,
    /// `None` on the top rung.
    pub next: Option<RankId>,
    /// The next rank's requirements, standing first; empty on the top rung.
    pub checks: Vec<RankCheck>,
}

impl PromotionAssessment {
    pub fn allowed(&self) -> bool {
        self.next.is_some() && self.checks.iter().all(|check| check.met())
    }

    /// Each failing requirement in words, such as `shadow needs standing 60.00, vex has 30.00`.
    pub fn reasons(&self) -> Vec<String> {
        let Some(next) = &self.next else {
            return Vec::new();
        };
        self.checks
            .iter()
            .filter(|check| !check.met())
            .map(|check| match check {
                RankCheck::Standing { required, has } => {
                    format!(
                        "{next} needs standing {required}, {} has {has}",
                        self.character
                    )
                }
                RankCheck::Tolerance { limit, distance } => format!(
                    "{next} needs to be within {limit} of {}, {} is {distance} away",
                    self.faction_name, self.character
                ),
            })
            .collect()
    }
}

/// One reason a character can't join a faction now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinBlock {
    AlreadyMember,
    /// Further from the faction than its tolerance.
    OutsideTolerance {
        distance: Fixed,
        tolerance: Fixed,
    },
}

/// Whether a character may join a faction, with every reason they can't (DESIGN.md §9.1, P-24).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinAssessment {
    pub character: CharacterId,
    pub faction: FactionId,
    /// The faction's name, so reasons can read naturally.
    pub faction_name: String,
    /// How far the character is from the faction, measured with the faction's weights.
    pub distance: Distance,
    pub tolerance: Fixed,
    /// Every failing check, in a fixed order.
    pub blocks: Vec<JoinBlock>,
    /// For each faction they're in that's in conflict with this one, in id order, whether
    /// it lets them go and this one takes them (DESIGN.md §9.2).
    pub defections: Vec<Defection>,
}

impl JoinAssessment {
    pub fn allowed(&self) -> bool {
        self.blocks.is_empty() && self.defections.iter().all(Defection::allowed)
    }

    /// Each failing check in words, such as `60.21 from The Lantern Guild, tolerance is 45.00`,
    /// then each table that refused.
    pub fn reasons(&self) -> Vec<String> {
        let blocks = self.blocks.iter().map(|block| match block {
            JoinBlock::AlreadyMember => {
                format!("{} is already a member of {}", self.character, self.faction)
            }
            JoinBlock::OutsideTolerance {
                distance,
                tolerance,
            } => format!(
                "{distance} from {}, tolerance is {tolerance}",
                self.faction_name
            ),
        });
        let refusals = self.defections.iter().flat_map(|defection| {
            [&defection.deserters, &defection.defectors]
                .into_iter()
                .filter_map(move |table| {
                    let Verdict::Refuse { reason } = &table.verdict else {
                        return None;
                    };
                    let quoted = reason
                        .as_ref()
                        .map(|reason| format!(", \"{reason}\""))
                        .unwrap_or_default();
                    Some(format!(
                        "{} belongs to {}, in conflict with {} ({}): refused by {}{quoted}",
                        self.character,
                        defection.from_name,
                        self.faction_name,
                        defection.relation,
                        table.rule_name()
                    ))
                })
        });
        blocks.chain(refusals).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn h(hundredths: i64) -> Fixed {
        Fixed::from_hundredths(hundredths)
    }

    #[test]
    fn drift_policies_are_named_as_content_writes_them() {
        assert_eq!(
            DriftPolicy::ALL.map(DriftPolicy::key),
            ["ignore", "flag", "demote", "expel"]
        );
        for policy in DriftPolicy::ALL {
            assert_eq!(DriftPolicy::from_key(policy.key()), Some(policy));
        }
        assert_eq!(DriftPolicy::from_key("probation"), None);
    }

    #[test]
    fn member_tolerance_defaults_to_the_tolerance() {
        let guild = Tolerances::new(h(45_00), None).expect("valid");
        assert_eq!((guild.tolerance(), guild.member()), (h(45_00), h(45_00)));
        let watch = Tolerances::new(h(40_00), Some(h(50_00))).expect("valid");
        assert_eq!((watch.tolerance(), watch.member()), (h(40_00), h(50_00)));
    }

    #[test]
    fn tolerances_run_from_zero_and_staying_is_never_harder_than_joining() {
        assert!(Tolerances::new(h(0), Some(h(0))).is_ok());
        assert!(Tolerances::new(h(30_00), Some(h(30_00))).is_ok());
        assert_eq!(
            Tolerances::new(h(-1), None),
            Err(ToleranceProblem::Negative(h(-1)))
        );
        assert_eq!(
            Tolerances::new(h(60_00), Some(h(50_00))),
            Err(ToleranceProblem::MemberBelowTolerance {
                member: h(50_00),
                tolerance: h(60_00)
            })
        );
    }

    #[test]
    fn describes_tolerance_problems() {
        assert_eq!(
            ToleranceProblem::Negative(h(-5_00)).to_string(),
            "-5.00 must be at least 0.00"
        );
        assert_eq!(
            ToleranceProblem::MemberBelowTolerance {
                member: h(50_00),
                tolerance: h(60_00)
            }
            .to_string(),
            "50.00 must be at least the faction's tolerance, 60.00"
        );
    }
}
