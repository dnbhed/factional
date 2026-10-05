use std::fmt;

use factional_core::{Fixed, Tick};

use crate::{CharacterId, Distance, FactionId};

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Membership {
    /// When they joined; tick 0 for a starting member.
    pub since: Tick,
}

/// Why a character left a faction. Defection, expulsion and conflict resolution arrive with
/// M7, M8 and M9.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaveReason {
    Voluntary,
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
    /// A member of a faction in conflict with this one; `relation` is the more hostile of the
    /// two directions.
    EnemyMembership {
        faction: FactionId,
        faction_name: String,
        relation: Fixed,
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
    /// Every failing check, in a fixed order; empty if they may join.
    pub blocks: Vec<JoinBlock>,
}

impl JoinAssessment {
    pub fn allowed(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Each failing check in words, such as `60.21 from The Lantern Guild, tolerance is 45.00`.
    pub fn reasons(&self) -> Vec<String> {
        self.blocks
            .iter()
            .map(|block| match block {
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
                JoinBlock::EnemyMembership {
                    faction_name,
                    relation,
                    ..
                } => format!(
                    "{} belongs to {faction_name}, in conflict with {} ({relation})",
                    self.character, self.faction_name
                ),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn h(hundredths: i64) -> Fixed {
        Fixed::from_hundredths(hundredths)
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
