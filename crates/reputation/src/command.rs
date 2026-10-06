use std::fmt;

use factional_core::{Envelope, Fixed, Tick, article};

use crate::{
    AXIS_LIMIT, ActionId, Alignment, CharacterId, Effects, FactionId, JoinAssessment, LeaveReason,
    Observer, OutcomeId, Party, PromotionAssessment, RankId, Spill, Witnesses,
};

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
    /// `character` asks to join `faction`; refused unless they may (DESIGN.md §9.1). If
    /// they're in an enemy of `faction`, joining means defecting: they leave it, if the
    /// defectors and deserters tables allow (§9.2).
    JoinFaction {
        character: CharacterId,
        faction: FactionId,
    },
    /// Moves `character` up one rung in `faction`, if the next rank's requirements hold. Only
    /// ever on request: meeting them never promotes anyone by itself (D-17).
    Promote {
        character: CharacterId,
        faction: FactionId,
    },
    /// Moves `character` down one rung in `faction`.
    Demote {
        character: CharacterId,
        faction: FactionId,
    },
    /// `character` leaves `faction` of their own accord.
    LeaveFaction {
        character: CharacterId,
        faction: FactionId,
    },
    /// Sets how `from` regards `to`, and with `mutual`, how `to` regards `from` too.
    SetRelation {
        from: FactionId,
        to: FactionId,
        value: Fixed,
        mutual: bool,
    },
    /// Moves how `from` regards `to` by `by`, clamped to ±100; with `mutual`, both ways.
    ShiftRelation {
        from: FactionId,
        to: FactionId,
        by: Fixed,
        mutual: bool,
    },
    /// Applies a named outcome from content to `character`, such as a quest's result (P-26).
    ApplyOutcome {
        outcome: OutcomeId,
        character: CharacterId,
    },
    /// Applies effects sent directly by another module; `source` says which, for the record.
    ApplyEffects {
        source: String,
        character: CharacterId,
        effects: Effects,
    },
    /// Starts reporting when anyone's disposition toward `subject` changes band (DESIGN.md
    /// §8.3, P-23).
    Watch { subject: CharacterId },
    /// Stops reporting band changes for `subject`.
    Unwatch { subject: CharacterId },
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
        /// The rung they start on: the faction's lowest.
        rank: RankId,
    },
    RankChanged {
        character: CharacterId,
        faction: FactionId,
        from: RankId,
        to: RankId,
    },
    LeftFaction {
        character: CharacterId,
        faction: FactionId,
        reason: LeaveReason,
    },
    /// How `party` regards `subject` changed.
    StandingChanged {
        subject: CharacterId,
        party: Party,
        before: Fixed,
        after: Fixed,
        /// What spilled over from changes with other factions, if any, in faction id order
        /// (DESIGN.md §7.1).
        spilled: Vec<Spill>,
    },
    /// An outcome was applied; its changes follow as their own events.
    OutcomeApplied {
        outcome: OutcomeId,
        character: CharacterId,
    },
    /// Effects from another module were applied; their changes follow.
    EffectsApplied {
        source: String,
        character: CharacterId,
    },
    /// How `from` regards `to` changed.
    RelationChanged {
        from: FactionId,
        to: FactionId,
        before: Fixed,
        after: Fixed,
    },
    /// `subject` is now watched; `bands` is every observer's band toward them at that moment,
    /// factions first, then characters, each in id order (DESIGN.md §8.3).
    Watched {
        subject: CharacterId,
        bands: Vec<(Observer, String)>,
    },
    Unwatched {
        subject: CharacterId,
    },
    /// How `observer` regards a watched `subject` moved into another band, at `score`.
    DispositionBandChanged {
        observer: Observer,
        subject: CharacterId,
        from: String,
        to: String,
        score: Fixed,
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
    /// The character being watched.
    Subject,
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
    /// `Promote` for a member already on the top rung.
    AtTopRank {
        character: CharacterId,
        faction: FactionId,
        rank: RankId,
    },
    /// `Demote` for a member already on the bottom rung.
    AtBottomRank {
        character: CharacterId,
        faction: FactionId,
        rank: RankId,
    },
    /// `Promote` when the next rank's requirements don't hold: the assessment says which.
    PromotionRefused(Box<PromotionAssessment>),
    /// A relation between a faction and itself.
    SelfRelation,
    /// `ApplyOutcome` named an outcome that isn't in content.
    UnknownOutcome {
        outcome: OutcomeId,
        suggestion: Option<OutcomeId>,
    },
    /// A relation or effect outside −100…100.
    ValueOutOfRange { value: Fixed },
    /// The change would put two of `character`'s factions in conflict. Until M9 can resolve
    /// that, it's refused, so no one is ever in two factions at war (invariant 6).
    WouldPutInConflict {
        character: CharacterId,
        factions: (FactionId, FactionId),
    },
    /// `Watch` for a subject already watched.
    AlreadyWatched { subject: CharacterId },
    /// `Unwatch` for a subject not watched.
    NotWatched { subject: CharacterId },
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
            Role::Member | Role::Subject => "character",
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
            CommandError::UnknownOutcome {
                outcome,
                suggestion,
            } => write!(
                f,
                "unknown outcome '{outcome}'{}",
                hint(suggestion.as_ref().map(OutcomeId::as_str))
            ),
            CommandError::AtTopRank {
                character,
                faction,
                rank,
            } => write!(
                f,
                "{character} is already {} {rank}, the highest rank of {faction}",
                article(rank.as_str())
            ),
            CommandError::AtBottomRank {
                character,
                faction,
                rank,
            } => write!(
                f,
                "{character} is already {} {rank}, the lowest rank of {faction}",
                article(rank.as_str())
            ),
            CommandError::PromotionRefused(assessment) => write!(
                f,
                "{} can't be promoted in {}: {}",
                assessment.character,
                assessment.faction,
                assessment.reasons().join("; ")
            ),
            CommandError::SelfRelation => {
                f.write_str("a faction can't have a relation with itself")
            }
            CommandError::ValueOutOfRange { value } => {
                write!(f, "{value} is outside {}..{}", -AXIS_LIMIT, AXIS_LIMIT)
            }
            CommandError::WouldPutInConflict {
                character,
                factions: (a, b),
            } => write!(
                f,
                "that would put two of {character}'s factions in conflict: {a} and {b}"
            ),
            CommandError::AlreadyWatched { subject } => {
                write!(f, "{subject} is already watched")
            }
            CommandError::NotWatched { subject } => write!(f, "{subject} isn't watched"),
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
