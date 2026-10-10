//! Quests in words (U4): their headings, steps, requirements and choices, as the CLI's
//! `quests` and `graph` and the editor's Quests tab show them, written in one place.

use factional_quests::{
    Choice, ChoiceEffects, Leftovers, Next, Quest, QuestId, Quests, Requirements, Step,
};
use factional_reputation::{AlignmentDelta, RelationEnds, RelationShift, StandingEffects};

/// `watch_oath — The Watch's Oath — from city_watch, in watch_career`, with `at step 1` too
/// when `at_step`.
pub fn quest_heading(quests: &Quests, quest: &Quest, at_step: bool) -> String {
    let mut whose = quests.giver_of(&quest.id).map_or_else(
        || "the world's own".to_owned(),
        |giver| format!("from {giver}"),
    );
    if let Some((line, step)) = quests.place_of(&quest.id) {
        whose += &format!(", in {line}");
        if at_step {
            whose += &format!(" at step {}", step + 1);
        }
    }
    format!("{} — {} — {whose}", quest.id, quest.name)
}

/// What a step needs of its quests: `2 of night_patrol, dock_inspection, smugglers_cove, in
/// any order; the rest close`.
pub fn describe_step(step: &Step) -> String {
    let quests: Vec<&str> = step.quests.iter().map(QuestId::as_str).collect();
    let listed = quests.join(", ");
    let mut described = match step.needed() {
        _ if quests.len() == 1 && step.needed() == 1 => listed,
        0 => format!("any of {listed}, or none"),
        needed if needed == quests.len() => format!("all of {listed}, in any order"),
        needed => format!("{needed} of {listed}, in any order"),
    };
    if step.leftovers == Leftovers::Close && step.needed() < quests.len() {
        described += "; the rest close";
    }
    described
}

/// ` — needs standing 10.00 with city_watch, and …`, or nothing if nothing is required.
pub fn needs(requires: &Requirements) -> String {
    let mut needs: Vec<String> = Vec::new();
    needs.extend(
        requires
            .standing
            .iter()
            .map(|(party, value)| format!("standing {value} with {party}")),
    );
    needs.extend(requires.member.iter().map(|f| format!("to be in {f}")));
    needs.extend(
        requires
            .not_member
            .iter()
            .map(|f| format!("not to be in {f}")),
    );
    needs.extend(
        requires
            .rank_at_least
            .iter()
            .map(|(faction, rank)| format!("rank {rank} or higher in {faction}")),
    );
    needs.extend(
        requires
            .within_tolerance
            .iter()
            .map(|f| format!("to be within {f}'s member tolerance")),
    );
    needs.extend(requires.done.iter().map(|done| format!("{done} done")));
    if needs.is_empty() {
        String::new()
    } else {
        format!(" — needs {}", needs.join(", and "))
    }
}

/// `report — outcome turned_in_vex — then oath`: a choice, what it does, what it locks and
/// where it leads.
pub fn describe_choice(choice: &Choice) -> String {
    let mut line = choice.id.to_string();
    match &choice.effects {
        ChoiceEffects::None => {}
        ChoiceEffects::Outcome(outcome) => line += &format!(" — outcome {outcome}"),
        ChoiceEffects::Inline(effects) => {
            if effects.alignment != AlignmentDelta::default() {
                line += &format!(
                    " — alignment: law {}, good {}",
                    effects.alignment.law, effects.alignment.good
                );
            }
            let standing = named_effects(&effects.standing);
            if !standing.is_empty() {
                line += &format!(" — standing: {}", standing.join(", "));
            }
            let relations = named_shifts(&effects.relations);
            if !relations.is_empty() {
                line += &format!(" — relations: {}", relations.join(", "));
            }
        }
    }
    if !choice.locks.is_empty() {
        let locks: Vec<String> = choice.locks.iter().map(ToString::to_string).collect();
        line += &format!(" — locks {}", locks.join(", "));
    }
    line += &match &choice.next {
        Next::Stage(stage) => format!(" — then {stage}"),
        Next::End => " — then the end".to_owned(),
    };
    line
}

/// Each relation shift: `city_watch ↔ temple -40.00`, or `city_watch → ashen_circle -20.00`
/// for one way.
pub fn named_shifts(shifts: &[RelationShift]) -> Vec<String> {
    shifts
        .iter()
        .map(|shift| match &shift.ends {
            RelationEnds::Between(a, b) => format!("{a} ↔ {b} {}", shift.by),
            RelationEnds::Directed { from, to } => format!("{from} → {to} {}", shift.by),
        })
        .collect()
}

/// Named standing effects as `city_watch -20.00, captain_hale -10.00`: factions first.
pub fn named_effects(effects: &StandingEffects) -> Vec<String> {
    effects
        .parties()
        .into_iter()
        .map(|(party, value)| format!("{party} {value}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use factional_quests::{Leftovers, QuestId, Requirements, Step};

    use super::describe_step;

    #[test]
    fn a_step_says_how_many_of_its_quests_it_needs() {
        let step = |quests: &[&str], need: Option<usize>, leftovers: Leftovers| Step {
            quests: quests
                .iter()
                .map(|id| QuestId::new(id).expect("valid id"))
                .collect(),
            need,
            requires: Requirements::default(),
            leftovers,
        };
        let described: Vec<String> = [
            step(&["oath"], None, Leftovers::Open),
            step(&["oath"], Some(0), Leftovers::Open),
            step(&["oath", "errand"], Some(1), Leftovers::Open),
            step(&["oath", "errand"], None, Leftovers::Close),
            step(&["oath", "errand", "dog"], Some(1), Leftovers::Close),
            step(&["oath", "errand"], Some(0), Leftovers::Close),
        ]
        .iter()
        .map(describe_step)
        .collect();
        assert_eq!(
            described,
            [
                "oath",
                "any of oath, or none",
                "1 of oath, errand, in any order",
                "all of oath, errand, in any order",
                "1 of oath, errand, dog, in any order; the rest close",
                "any of oath, errand, or none; the rest close",
            ]
        );
    }
}
