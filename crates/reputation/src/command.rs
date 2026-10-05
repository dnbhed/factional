use std::fmt;

use factional_core::{Envelope, Fixed, Tick};

use crate::{ActionId, Alignment, CharacterId, FactionId, JoinAssessment, LeaveReason, Witnesses};

/// A request to change the world: the only way in (DESIGN.md §2, §11.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Moves time forward. The host's game loop sends it; the engine never reads a clock.
    AdvanceTime { ticks: u64 },
    /// `actor` does `action`, optionally to `target`. `scale` (more than 0; 1.00 normally)
    /// says how big this instance was, such as stealing a crown rather than a loaf
    /// (DESIGN.md §5.2).
    PerformAction {
        actor: CharacterId,
        action: ActionId,
        target: Option<CharacterId>,
        scale: Fixed,
        witnesses: Witnesses,
    },
    /// `character` asks to join `faction`; refused unless they may (DESIGN.md §9.1).
    JoinFaction {
        character: CharacterId,
        faction: FactionId,
    },
    /// `character` leaves `faction` of their own accord.
    LeaveFaction {
        character: CharacterId,
        faction: FactionId,
    },
}

/// What changed, carried in an [`Event`]. Events hold absolute before-and-after values, so
/// they read without context and replay without rules (P-15).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    TimeAdvanced {
        from: Tick,
        to: Tick,
    },
    /// An act, as it was done. Its effects follow as their own events.
    ActionPerformed {
        actor: CharacterId,
        action: ActionId,
        target: Option<CharacterId>,
        scale: Fixed,
        witnesses: Witnesses,
    },
    AlignmentChanged {
        character: CharacterId,
        from: Alignment,
        to: Alignment,
    },
    JoinedFaction {
        character: CharacterId,
        faction: FactionId,
    },
    LeftFaction {
        character: CharacterId,
        faction: FactionId,
        reason: LeaveReason,
    },
}

/// Something that happened in the world, numbered and stamped with when it happened.
pub type Event = Envelope<Change>;

/// The part a character plays in a command, so a refusal can say which one was wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Actor,
    Target,
    Witness,
    /// The character joining or leaving a faction.
    Member,
}

/// Why a command was refused. A refused command changes nothing and emits nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    /// `AdvanceTime` by zero ticks.
    NoTicks,
    /// `AdvanceTime` past the last tick there is.
    TimeOverflow { now: Tick, ticks: u64 },
    /// A command named a character that doesn't exist; `suggestion` is a close id, if any.
    UnknownCharacter {
        role: Role,
        id: CharacterId,
        suggestion: Option<CharacterId>,
    },
    /// `PerformAction` named an action that isn't in the catalogue.
    UnknownAction {
        action: ActionId,
        suggestion: Option<ActionId>,
    },
    /// `PerformAction` with a scale of 0 or less.
    ScaleNotPositive { scale: Fixed },
    /// `PerformAction` whose target is its actor.
    TargetIsActor,
    /// A command named a faction that doesn't exist.
    UnknownFaction {
        faction: FactionId,
        suggestion: Option<FactionId>,
    },
    /// `JoinFaction` for a character who may not join: the assessment says why.
    JoinRefused(Box<JoinAssessment>),
    /// `LeaveFaction` for a faction the character isn't in.
    NotAMember {
        character: CharacterId,
        faction: FactionId,
    },
}

/// One command as it was issued, and whether the world accepted it (P-16).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalEntry {
    pub command: Command,
    pub result: Result<(), CommandError>,
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Role::Actor => "actor",
            Role::Target => "target",
            Role::Witness => "witness",
            Role::Member => "character",
        })
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hint = |suggestion: Option<&str>| {
            suggestion
                .map(|close| format!(" (did you mean '{close}'?)"))
                .unwrap_or_default()
        };
        match self {
            CommandError::NoTicks => f.write_str("ticks must be at least 1"),
            CommandError::TimeOverflow { now, ticks } => write!(
                f,
                "time can't advance {ticks} ticks from tick {now}: it would pass the last tick"
            ),
            CommandError::UnknownCharacter {
                role,
                id,
                suggestion,
            } => write!(
                f,
                "unknown {role} '{id}'{}",
                hint(suggestion.as_ref().map(CharacterId::as_str))
            ),
            CommandError::UnknownAction { action, suggestion } => write!(
                f,
                "unknown action '{action}'{}",
                hint(suggestion.as_ref().map(ActionId::as_str))
            ),
            CommandError::ScaleNotPositive { .. } => {
                write!(f, "scale must be greater than {}", Fixed::ZERO)
            }
            CommandError::TargetIsActor => {
                f.write_str("an action's target must be another character")
            }
            CommandError::UnknownFaction {
                faction,
                suggestion,
            } => write!(
                f,
                "unknown faction '{faction}'{}",
                hint(suggestion.as_ref().map(FactionId::as_str))
            ),
            CommandError::JoinRefused(assessment) => write!(
                f,
                "{} can't join {}: {}",
                assessment.character,
                assessment.faction,
                assessment.reasons().join("; ")
            ),
            CommandError::NotAMember { character, faction } => {
                write!(f, "{character} isn't a member of {faction}")
            }
        }
    }
}

impl std::error::Error for CommandError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn character(id: &str) -> CharacterId {
        CharacterId::new(id).expect("a valid id")
    }

    #[test]
    fn describes_refusals_in_plain_words() {
        assert_eq!(
            CommandError::NoTicks.to_string(),
            "ticks must be at least 1"
        );
        assert_eq!(
            CommandError::TimeOverflow {
                now: Tick(10),
                ticks: u64::MAX,
            }
            .to_string(),
            format!(
                "time can't advance {} ticks from tick 10: it would pass the last tick",
                u64::MAX
            )
        );
        assert_eq!(
            CommandError::ScaleNotPositive { scale: Fixed::ZERO }.to_string(),
            "scale must be greater than 0.00"
        );
        assert_eq!(
            CommandError::TargetIsActor.to_string(),
            "an action's target must be another character"
        );
    }

    #[test]
    fn names_an_unknown_character_by_its_role_with_any_close_id() {
        let unknown = |role, id, suggestion: Option<&str>| {
            CommandError::UnknownCharacter {
                role,
                id: character(id),
                suggestion: suggestion.map(character),
            }
            .to_string()
        };
        assert_eq!(
            unknown(Role::Actor, "plyer", Some("player")),
            "unknown actor 'plyer' (did you mean 'player'?)"
        );
        assert_eq!(
            unknown(Role::Target, "nobody", None),
            "unknown target 'nobody'"
        );
        assert_eq!(
            unknown(Role::Witness, "ghost", None),
            "unknown witness 'ghost'"
        );
    }

    #[test]
    fn names_an_unknown_action_with_any_close_id() {
        let action = |id| ActionId::new(id).expect("a valid id");
        assert_eq!(
            CommandError::UnknownAction {
                action: action("stael"),
                suggestion: Some(action("steal")),
            }
            .to_string(),
            "unknown action 'stael' (did you mean 'steal'?)"
        );
        assert_eq!(
            CommandError::UnknownAction {
                action: action("dance"),
                suggestion: None,
            }
            .to_string(),
            "unknown action 'dance'"
        );
    }
}
