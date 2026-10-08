//! `quests <dir> [<quest>]`: a directory's quests and questlines, read and checked without
//! loading a world, since a world with quests doesn't load until they can be reconciled (Q4).

use std::path::Path;

use factional_quests::{
    ChoiceEffects, Leftovers, Next, Quest, QuestId, Quests, Requirements, Step,
};
use factional_reputation::AlignmentDelta;

use crate::check::count;
use crate::session::{Outcome, hint, lines, named_effects, named_shifts};

/// `quests <dir> [<quest>]`: every quest and questline in `dir`, or one quest's stages and
/// choices, then any warnings.
pub(crate) fn quests(base: &Path, args: &str) -> Outcome {
    let (dir, quest) = match args.split_whitespace().collect::<Vec<_>>()[..] {
        [dir] => (dir, None),
        [dir, quest] => (dir, Some(quest)),
        _ => return Outcome::Error("quests needs the form: quests <dir> [<quest>]".to_owned()),
    };
    let quests = match factional_content::load_quests(&base.join(dir)) {
        Ok((_, quests)) => quests,
        Err(error) => return Outcome::Error(error.to_string()),
    };
    if quests.is_empty() {
        return Outcome::Output(format!("no quests in {dir}"));
    }
    let shown = match quest {
        None => list(&quests),
        Some(id) => match quests.quests.keys().find(|known| known.as_str() == id) {
            Some(id) => show(&quests, id),
            None => {
                let ids: Vec<&str> = quests.quests.keys().map(QuestId::as_str).collect();
                return Outcome::Error(format!("unknown quest '{id}'{}", hint(id, ids)));
            }
        },
    };
    let warnings = factional_content::quest_warnings(&quests)
        .into_iter()
        .map(|warning| format!("warning: {warning}"));
    Outcome::Output(lines(shown.into_iter().chain(warnings)))
}

/// Every quest, one per line, then every questline with its steps.
fn list(quests: &Quests) -> Vec<String> {
    let mut shown = vec!["quests:".to_owned()];
    for quest in quests.quests.values() {
        let mut line = format!(
            "  {} — {}",
            heading(quests, quest, false),
            count(quest.stages.len(), "stage")
        );
        line += &needs(&quest.requires);
        shown.push(line);
    }
    if !quests.questlines.is_empty() {
        shown.push("questlines:".to_owned());
    }
    for line in quests.questlines.values() {
        let giver = line.giver.as_ref().map_or_else(
            || "the world's own".to_owned(),
            |giver| format!("from {giver}"),
        );
        shown.push(format!(
            "  {} — {} — {giver} — {}",
            line.id,
            line.name,
            count(line.steps.len(), "step")
        ));
        for (index, step) in line.steps.iter().enumerate() {
            shown.push(format!(
                "    {}. {}{}",
                index + 1,
                describe_step(step),
                needs(&step.requires)
            ));
        }
    }
    shown
}

/// One quest: its heading and gate, then each stage with its requirements and choices.
fn show(quests: &Quests, id: &QuestId) -> Vec<String> {
    let quest = &quests.quests[id];
    let mut shown = vec![heading(quests, quest, true) + &needs(&quest.requires)];
    for (index, stage) in quest.stages.iter().enumerate() {
        shown.push(format!(
            "{}. {}{}",
            index + 1,
            stage.id,
            needs(&stage.requires)
        ));
        for choice in &stage.choices {
            let mut line = format!("   {}", choice.id);
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
            line += &match &choice.next {
                Next::Stage(stage) => format!(" — then {stage}"),
                Next::End => " — then the end".to_owned(),
            };
            shown.push(line);
        }
    }
    shown
}

/// `watch_oath — The Watch's Oath — from city_watch, in watch_career`, with the step when
/// `at_step`.
fn heading(quests: &Quests, quest: &Quest, at_step: bool) -> String {
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

/// A step's quests and how many it needs: `2 of a, b, c, in any order; the rest close`.
fn describe_step(step: &Step) -> String {
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

/// ` — needs standing 10.00 with city_watch, and …`, or nothing when nothing is required.
fn needs(requires: &Requirements) -> String {
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

#[cfg(test)]
mod tests {
    use factional_quests::{Leftovers, QuestId, Requirements, Step};

    use super::describe_step;
    use crate::session::{Outcome, ScriptError, Session};

    fn run(line: &str) -> Result<Outcome, ScriptError> {
        Session::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).execute(line)
    }

    fn output(text: &str) -> Result<Outcome, ScriptError> {
        Ok(Outcome::Output(text.into()))
    }

    fn command_error(message: &str) -> Result<Outcome, ScriptError> {
        Ok(Outcome::Error(message.into()))
    }

    #[test]
    fn quests_lists_every_quest_then_every_questline_with_its_steps() {
        assert_eq!(
            run("quests docs/examples/riverhold"),
            output(
                "quests:\n\
                 \x20 dock_inspection — Inspecting the Docks — from city_watch, in watch_career — 1 stage\n\
                 \x20 harbour_errands — Harbour Errands — from city_watch, in watch_career — 1 stage\n\
                 \x20 lost_dog — The Captain's Dog — from captain_hale, in watch_career — 1 stage\n\
                 \x20 lost_ring — Ava's Lost Ring — from merchant_ava — 1 stage\n\
                 \x20 night_patrol — Night Patrol — from city_watch, in watch_career — 1 stage\n\
                 \x20 smugglers_cove — The Smugglers' Cove — from city_watch, in watch_career — 2 stages — needs watch_oath.patrol.report done\n\
                 \x20 the_long_winter — The Long Winter — the world's own — 1 stage\n\
                 \x20 watch_captain — Captain of the Watch — from city_watch, in watch_career — 1 stage\n\
                 \x20 watch_oath — The Watch's Oath — from city_watch, in watch_career — 2 stages — needs not to be in lantern_guild\n\
                 questlines:\n\
                 \x20 watch_career — A Life in the Watch — from city_watch — 4 steps\n\
                 \x20   1. watch_oath\n\
                 \x20   2. 2 of night_patrol, dock_inspection, smugglers_cove, in any order; the rest close\n\
                 \x20   3. any of harbour_errands, lost_dog, or none\n\
                 \x20   4. watch_captain — needs standing 40.00 with city_watch, and rank sergeant or higher in city_watch"
            )
        );
    }

    #[test]
    fn quests_shows_one_quests_stages_and_choices() {
        assert_eq!(
            run("quests docs/examples/riverhold watch_oath"),
            output(
                "watch_oath — The Watch's Oath — from city_watch, in watch_career at step 1 — needs not to be in lantern_guild\n\
                 1. patrol\n\
                 \x20  report — outcome turned_in_vex — then oath\n\
                 \x20  look_away — outcome took_a_bribe — then the end\n\
                 2. oath — needs standing 10.00 with city_watch\n\
                 \x20  swear — alignment: law 5.00, good 0.00 — then the end"
            )
        );
        assert_eq!(
            run("quests docs/examples/riverhold dock_inspection"),
            output(
                "dock_inspection — Inspecting the Docks — from city_watch, in watch_career at step 2\n\
                 1. inspect\n\
                 \x20  seize — alignment: law 2.00, good 0.00 — standing: city_watch 5.00, lantern_guild -5.00 — then the end\n\
                 \x20  wave_through — standing: lantern_guild 5.00 — then the end"
            )
        );
        assert_eq!(
            run("quests docs/examples/riverhold watch_captain"),
            output(
                "watch_captain — Captain of the Watch — from city_watch, in watch_career at step 4\n\
                 1. command — needs to be in city_watch, and to be within city_watch's member tolerance\n\
                 \x20  accept — alignment: law 5.00, good 0.00 — standing: city_watch 10.00 — then the end"
            )
        );
        assert_eq!(
            run("quests docs/examples/riverhold the_long_winter"),
            output(
                "the_long_winter — The Long Winter — the world's own\n\
                 1. stores\n\
                 \x20  share — alignment: law 0.00, good 6.00 — standing: temple 10.00 — relations: temple ↔ city_watch 5.00 — then the end\n\
                 \x20  hoard — alignment: law 0.00, good -6.00 — then the end"
            )
        );
        assert_eq!(
            run("quests docs/examples/riverhold smugglers_cove"),
            output(
                "smugglers_cove — The Smugglers' Cove — from city_watch, in watch_career at step 2 — needs watch_oath.patrol.report done\n\
                 1. find\n\
                 \x20  follow_the_lights — then raid\n\
                 2. raid\n\
                 \x20  arrest_them — standing: city_watch 10.00, lantern_guild -10.00 — then the end"
            )
        );
    }

    #[test]
    fn quests_lists_warnings_after_the_quests() {
        assert_eq!(
            run("quests crates/cli/tests/fixtures/worlds/odd_jobs"),
            output(
                "quests:\n\
                 \x20 inspect — Inspect — from watch, in jobs — 1 stage\n\
                 \x20 patrol — Patrol — from watch, in jobs — 1 stage\n\
                 questlines:\n\
                 \x20 jobs — Odd Jobs — from watch — 1 step\n\
                 \x20   1. all of patrol, inspect, in any order\n\
                 warning: questlines.toml: jobs.steps[0].leftovers: leftovers = \"close\" has no effect: the step needs all its quests, so none are left over"
            )
        );
    }

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

    #[test]
    fn quests_names_an_unknown_quest_with_a_suggestion() {
        assert_eq!(
            run("quests docs/examples/riverhold watch_oth"),
            command_error("unknown quest 'watch_oth' (did you mean 'watch_oath'?)")
        );
    }

    #[test]
    fn quests_says_when_a_directory_has_none() {
        assert_eq!(
            run("quests content/sample"),
            output("no quests in content/sample")
        );
    }

    #[test]
    fn quests_reports_every_problem_instead_of_listing() {
        assert_eq!(
            run("quests crates/cli/tests/fixtures/worlds/broken"),
            run("load crates/cli/tests/fixtures/worlds/broken")
        );
        assert_eq!(run("quests no/such/world"), run("load no/such/world"));
    }

    #[test]
    fn quests_needs_a_directory_and_at_most_one_quest() {
        let usage = command_error("quests needs the form: quests <dir> [<quest>]");
        assert_eq!(run("quests"), usage);
        assert_eq!(
            run("quests docs/examples/riverhold watch_oath lost_ring"),
            usage
        );
    }
}
