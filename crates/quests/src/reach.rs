//! Whether quest content can be reached (Q3, P-69). First the structure within each quest:
//! stages no choice leads to, gates that can never hold, and `done` on a quest's own
//! progress where it can't have happened. Then, once the structure is sound, a generous run
//! of what the `done` requirements and the questlines' order allow: every other requirement
//! is taken as one the character can meet (P-51, P-66), and timing is ignored, so anything
//! the run can't reach can never be reached.

use std::collections::BTreeSet;

use crate::check::{Blocker, Gate, Needs, QuestProblem};
use crate::quest::{Next, Progress, Quest, QuestId, QuestlineId, Quests, Requirements, Step};

impl Quests {
    /// Content that can never be reached, once every reference resolves: the structure's
    /// problems if it has any, or else what the runs find.
    pub(crate) fn reach_problems(&self) -> Vec<QuestProblem> {
        let structure = self.structure_problems();
        if !structure.is_empty() {
            return structure;
        }
        self.dependency_problems()
    }

    /// Each quest in id order: its start gate, then each stage; then each questline's
    /// steps' gates.
    fn structure_problems(&self) -> Vec<QuestProblem> {
        let mut problems = Vec::new();
        for quest in self.quests.values() {
            let gate = Gate::Quest(quest.id.clone());
            let step = self.step_of(&quest.id);
            let mut alongside = vec![&quest.requires];
            alongside.extend(step.map(|(_, _, step)| &step.requires));
            contradictions(&gate, &quest.requires, &alongside, &mut problems);
            for (index, progress) in quest.requires.done.iter().enumerate() {
                if progress.quest() == &quest.id {
                    problems.push(QuestProblem::OwnProgress {
                        gate: gate.clone(),
                        index,
                        progress: progress.clone(),
                    });
                }
            }
            let leads = leads(quest);
            for (index, stage) in quest.stages.iter().enumerate() {
                if index > 0 && !leads[0].contains(&index) {
                    problems.push(QuestProblem::UnreachableStage {
                        quest: quest.id.clone(),
                        stage: index,
                    });
                }
                let gate = Gate::Stage {
                    quest: quest.id.clone(),
                    stage: index,
                };
                contradictions(&gate, &stage.requires, &[&stage.requires], &mut problems);
                for (place, progress) in stage.requires.done.iter().enumerate() {
                    if progress.quest() == &quest.id && !leads_to(quest, &leads, progress, index) {
                        problems.push(QuestProblem::OwnProgress {
                            gate: gate.clone(),
                            index: place,
                            progress: progress.clone(),
                        });
                    }
                }
            }
        }
        for line in self.questlines.values() {
            for (index, step) in line.steps.iter().enumerate() {
                let gate = Gate::Step {
                    questline: line.id.clone(),
                    step: index,
                };
                let mut alongside = vec![&step.requires];
                alongside.extend(
                    step.quests
                        .iter()
                        .filter_map(|quest| self.quests.get(quest))
                        .map(|quest| &quest.requires),
                );
                contradictions(&gate, &step.requires, &alongside, &mut problems);
                for (place, progress) in step.requires.done.iter().enumerate() {
                    if step.quests.contains(progress.quest()) {
                        problems.push(QuestProblem::OwnProgress {
                            gate: gate.clone(),
                            index: place,
                            progress: progress.clone(),
                        });
                    }
                }
            }
        }
        problems
    }

    /// What the runs find: each quest in id order that can never start, or each of its
    /// stages whose `done` can never happen; then quests their own step's leftovers close
    /// before they can start; then stages that need their quest over first.
    fn dependency_problems(&self) -> Vec<QuestProblem> {
        let full = self.run(&Limits::default());
        let mut problems = Vec::new();
        for quest in self.quests.values() {
            if !full.started.contains(&quest.id) {
                problems.push(QuestProblem::NeverStarts {
                    quest: quest.id.clone(),
                    blocker: self.blocker(quest, &full),
                });
                continue;
            }
            for (index, stage) in quest.stages.iter().enumerate() {
                let led_to = index == 0
                    || full.choices.iter().any(|(id, at, choice)| {
                        *id == &quest.id
                            && quest.stages[*at].choices[*choice].next
                                == Next::Stage(stage.id.clone())
                    });
                if !led_to || full.stages.contains(&(&quest.id, index)) {
                    continue;
                }
                if let Some((place, progress)) = self.unmet(&stage.requires, &full) {
                    problems.push(QuestProblem::StageNeverReached {
                        quest: quest.id.clone(),
                        stage: index,
                        index: place,
                        progress: progress.clone(),
                    });
                }
            }
        }
        for line in self.questlines.values() {
            for (index, step) in line.steps.iter().enumerate() {
                // A step that needs every quest has none left over, and the last step has no
                // later one to shut; the run then finds nothing, so neither needs leaving out.
                if step.leftovers != crate::Leftovers::Close {
                    continue;
                }
                let shut = self.run(&Limits {
                    shut: Some((&line.id, index)),
                    hold: None,
                });
                for quest in &step.quests {
                    if full.started.contains(quest) && !shut.started.contains(quest) {
                        problems.push(QuestProblem::NeverStarts {
                            quest: quest.clone(),
                            blocker: Blocker::Closed {
                                questline: line.id.clone(),
                                step: index,
                            },
                        });
                    }
                }
            }
        }
        for quest in self.quests.values() {
            let leads = leads(quest);
            for (index, stage) in quest.stages.iter().enumerate() {
                // A stage the generous run never reached is reported above already.
                if !full.stages.contains(&(&quest.id, index)) {
                    continue;
                }
                let foreign: Vec<(usize, &Progress)> = stage
                    .requires
                    .done
                    .iter()
                    .enumerate()
                    .filter(|(_, progress)| progress.quest() != &quest.id)
                    .collect();
                if foreign.is_empty() {
                    continue;
                }
                let before: BTreeSet<usize> = (0..index)
                    .filter(|earlier| leads[*earlier].contains(&index))
                    .collect();
                let held = self.run(&Limits {
                    shut: None,
                    hold: Some(Hold {
                        quest: &quest.id,
                        stages: &before,
                        to: index,
                    }),
                });
                for (place, progress) in foreign {
                    if !held.achieved(self, progress) {
                        problems.push(QuestProblem::OnlyAfter {
                            quest: quest.id.clone(),
                            stage: index,
                            index: place,
                            progress: progress.clone(),
                        });
                    }
                }
            }
        }
        problems
    }

    /// Why a quest the run never started can't start: the first step before its own that
    /// can never be complete, or else the first `done` in its gate that can never happen.
    fn blocker(&self, quest: &Quest, run: &Run<'_>) -> Blocker {
        if let Some((line, at, _)) = self.step_of(&quest.id)
            && let Some(step) = self.first_incomplete(line, at, run)
        {
            return Blocker::Step {
                questline: line.clone(),
                step,
            };
        }
        let step = self.step_of(&quest.id).map(|(_, _, step)| &step.requires);
        let unmet = self
            .unmet(&quest.requires, run)
            .or_else(|| step.and_then(|requires| self.unmet(requires, run)))
            .map(|(_, progress)| progress.clone())
            .expect("a quest the run never started has something it never met");
        Blocker::Done(unmet)
    }

    /// The first `done` the run never achieved, with its place.
    fn unmet<'r>(
        &self,
        requires: &'r Requirements,
        run: &Run<'_>,
    ) -> Option<(usize, &'r Progress)> {
        requires
            .done
            .iter()
            .enumerate()
            .find(|(_, progress)| !run.achieved(self, progress))
    }

    /// The questline, the step's place and the step a quest is in, if any.
    fn step_of(&self, quest: &QuestId) -> Option<(&QuestlineId, usize, &Step)> {
        let (line, at) = self.place_of(quest)?;
        Some((line, at, &self.questlines[line].steps[at]))
    }

    /// The first step before `at` in `line` that the run never completed: fewer of its
    /// quests finished than it needs.
    fn first_incomplete(&self, line: &QuestlineId, at: usize, run: &Run<'_>) -> Option<usize> {
        self.questlines[line].steps[..at].iter().position(|step| {
            step.quests
                .iter()
                .filter(|quest| run.finished.contains(quest))
                .count()
                < step.needed()
        })
    }

    /// Everything reachable within `limits`, growing until nothing more is.
    fn run<'q>(&'q self, limits: &Limits<'_>) -> Run<'q> {
        let mut run = Run::default();
        // Until a pass adds nothing; the first pass always runs, as `None` is never a run.
        let mut before = None;
        while before.as_ref() != Some(&run) {
            before = Some(run.clone());
            for quest in self.quests.values() {
                let place = self.step_of(&quest.id);
                if let (Some((line, at, _)), Some((shut, after))) = (place, limits.shut)
                    && line == shut
                    && at > after
                {
                    continue;
                }
                if !run.started.contains(&quest.id) {
                    let in_order = place.is_none_or(|(line, at, _)| {
                        self.first_incomplete(line, at, &run).is_none()
                    });
                    let gate_met = self.unmet(&quest.requires, &run).is_none()
                        && place
                            .is_none_or(|(_, _, step)| self.unmet(&step.requires, &run).is_none());
                    if in_order && gate_met {
                        run.started.insert(&quest.id);
                    }
                }
                if run.started.contains(&quest.id) {
                    self.walk(quest, limits.hold.as_ref(), &mut run);
                }
            }
        }
        run
    }

    /// The stages and choices of a started quest that the run can reach now, and whether it
    /// can be finished; a held quest goes no further than the stages before its stage.
    fn walk<'q>(&'q self, quest: &'q Quest, hold: Option<&Hold<'_>>, run: &mut Run<'q>) {
        let held = hold.filter(|hold| hold.quest == &quest.id);
        let allowed = |stage: usize| held.is_none_or(|hold| hold.stages.contains(&stage));
        if allowed(0) && self.unmet(&quest.stages[0].requires, run).is_none() {
            run.stages.insert((&quest.id, 0));
        }
        for (index, stage) in quest.stages.iter().enumerate() {
            if !run.stages.contains(&(&quest.id, index)) {
                continue;
            }
            for (place, choice) in stage.choices.iter().enumerate() {
                match &choice.next {
                    Next::End => {
                        if held.is_some() {
                            continue;
                        }
                        run.finished.insert(&quest.id);
                    }
                    Next::Stage(next) => {
                        let to = quest.stage_index(next);
                        if held.is_some_and(|hold| hold.to != to && !allowed(to)) {
                            continue;
                        }
                        if allowed(to) && self.unmet(&quest.stages[to].requires, run).is_none() {
                            run.stages.insert((&quest.id, to));
                        }
                    }
                }
                run.choices.insert((&quest.id, index, place));
            }
        }
    }
}

/// What a run may not do.
#[derive(Default)]
struct Limits<'l> {
    /// A questline and a step whose leftovers close: no quest of a later step starts.
    shut: Option<(&'l QuestlineId, usize)>,
    /// A quest held under way before one of its stages.
    hold: Option<Hold<'l>>,
}

/// A quest held under way before stage `to`: only the stages that lead to it, and the
/// choices that keep it on the way there, and it never finishes.
struct Hold<'l> {
    quest: &'l QuestId,
    stages: &'l BTreeSet<usize>,
    to: usize,
}

/// What a run reached.
#[derive(Clone, Default, PartialEq, Eq)]
struct Run<'q> {
    started: BTreeSet<&'q QuestId>,
    stages: BTreeSet<(&'q QuestId, usize)>,
    choices: BTreeSet<(&'q QuestId, usize, usize)>,
    finished: BTreeSet<&'q QuestId>,
}

impl Run<'_> {
    /// Whether the run achieved `progress`.
    fn achieved(&self, quests: &Quests, progress: &Progress) -> bool {
        let quest = progress.quest();
        let found = &quests.quests[quest];
        match progress {
            Progress::Quest(_) => self.finished.contains(quest),
            Progress::Stage(_, stage) => self.stages.contains(&(quest, found.stage_index(stage))),
            Progress::Choice(_, stage, choice) => {
                let at = found.stage_index(stage);
                let place = found.stages[at]
                    .choices
                    .iter()
                    .position(|each| &each.id == choice)
                    .expect("references resolve before reachability");
                self.choices.contains(&(quest, at, place))
            }
        }
    }
}

impl Quest {
    /// A stage's place in the quest; references resolve before reachability is checked.
    fn stage_index(&self, stage: &crate::StageId) -> usize {
        self.stages
            .iter()
            .position(|each| &each.id == stage)
            .expect("references resolve before reachability")
    }
}

/// For each stage, every later stage a path of choices leads to from it.
fn leads(quest: &Quest) -> Vec<BTreeSet<usize>> {
    let mut leads = vec![BTreeSet::new(); quest.stages.len()];
    for index in (0..quest.stages.len()).rev() {
        let mut reach = BTreeSet::new();
        for choice in &quest.stages[index].choices {
            if let Next::Stage(next) = &choice.next {
                let to = quest.stage_index(next);
                reach.insert(to);
                reach.extend(leads[to].iter().copied());
            }
        }
        leads[index] = reach;
    }
    leads
}

/// Whether a quest's own `progress` can have happened by the time it reaches stage `at`:
/// an earlier stage that leads there, or a choice that does.
fn leads_to(quest: &Quest, leads: &[BTreeSet<usize>], progress: &Progress, at: usize) -> bool {
    match progress {
        Progress::Quest(_) => false,
        Progress::Stage(_, stage) => leads[quest.stage_index(stage)].contains(&at),
        Progress::Choice(_, stage, choice) => {
            let from = quest.stage_index(stage);
            let next = quest.stages[from]
                .choices
                .iter()
                .find(|each| &each.id == choice)
                .map(|each| &each.next)
                .expect("references resolve before reachability");
            match next {
                Next::End => false,
                Next::Stage(next) => {
                    let to = quest.stage_index(next);
                    to == at || leads[to].contains(&at)
                }
            }
        }
    }
}

/// Each `not_member` in `requires` that the gate, made of `alongside`, also needs to be a
/// member of, or to hold a rank in. A problem already found isn't reported again.
fn contradictions(
    gate: &Gate,
    requires: &Requirements,
    alongside: &[&Requirements],
    problems: &mut Vec<QuestProblem>,
) {
    for (index, faction) in requires.not_member.iter().enumerate() {
        let needs = if alongside.iter().any(|other| other.member.contains(faction)) {
            Needs::Member
        } else if alongside
            .iter()
            .any(|other| other.rank_at_least.contains_key(faction))
        {
            Needs::Rank
        } else {
            continue;
        };
        let problem = QuestProblem::ContradictoryGate {
            gate: gate.clone(),
            index,
            faction: faction.clone(),
            needs,
        };
        if !problems.contains(&problem) {
            problems.push(problem);
        }
    }
}

#[cfg(test)]
mod tests {
    use factional_reputation::{FactionId, RankId};

    use super::*;
    use crate::{Choice, ChoiceEffects, ChoiceId, Leftovers, Questline, Stage, StageId};

    fn quest_id(id: &str) -> QuestId {
        QuestId::new(id).expect("valid id")
    }

    fn line_id(id: &str) -> QuestlineId {
        QuestlineId::new(id).expect("valid id")
    }

    fn faction(id: &str) -> FactionId {
        FactionId::new(id).expect("valid id")
    }

    fn done(progress: &[&str]) -> Requirements {
        Requirements {
            done: progress
                .iter()
                .map(|text| Progress::parse(text).expect("valid"))
                .collect(),
            ..Requirements::default()
        }
    }

    /// A stage whose choices are `(id, next)`, `None` for the end.
    fn stage(id: &str, choices: &[(&str, Option<&str>)]) -> Stage {
        Stage {
            id: StageId::new(id).expect("valid id"),
            requires: Requirements::default(),
            choices: choices
                .iter()
                .map(|(choice, next)| Choice {
                    id: ChoiceId::new(choice).expect("valid id"),
                    effects: ChoiceEffects::None,
                    next: next.map_or(Next::End, |next| {
                        Next::Stage(StageId::new(next).expect("valid id"))
                    }),
                })
                .collect(),
        }
    }

    fn quest(id: &str, stages: Vec<Stage>) -> Quest {
        Quest {
            id: quest_id(id),
            name: id.to_owned(),
            giver: None,
            requires: Requirements::default(),
            stages,
        }
    }

    /// A one-stage quest: `go`, with `done` to the end.
    fn simple(id: &str) -> Quest {
        quest(id, vec![stage("go", &[("done", None)])])
    }

    fn step(quests: &[&str], need: Option<usize>, leftovers: Leftovers) -> Step {
        Step {
            quests: quests.iter().map(|id| quest_id(id)).collect(),
            need,
            requires: Requirements::default(),
            leftovers,
        }
    }

    fn quests(quests: Vec<Quest>, lines: Vec<(&str, Vec<Step>)>) -> Quests {
        Quests {
            quests: quests.into_iter().map(|q| (q.id.clone(), q)).collect(),
            questlines: lines
                .into_iter()
                .map(|(id, steps)| {
                    (
                        line_id(id),
                        Questline {
                            id: line_id(id),
                            name: id.to_owned(),
                            giver: None,
                            steps,
                        },
                    )
                })
                .collect(),
        }
    }

    /// `heist`: `case` (`scout` to `vault`, `quit` to the end), `vault` (`crack` to
    /// `escape`, `abort` to the end), `escape` (`run` to the end).
    fn heist() -> Quest {
        quest(
            "heist",
            vec![
                stage("case", &[("scout", Some("vault")), ("quit", None)]),
                stage("vault", &[("crack", Some("escape")), ("abort", None)]),
                stage("escape", &[("run", None)]),
            ],
        )
    }

    fn problems(quests: &Quests) -> Vec<QuestProblem> {
        quests.reach_problems()
    }

    fn progress(text: &str) -> Progress {
        Progress::parse(text).expect("valid")
    }

    #[test]
    fn sound_quests_have_nothing_to_report() {
        assert_eq!(
            problems(&quests(vec![heist(), simple("errand")], vec![])),
            []
        );
        assert_eq!(problems(&Quests::default()), []);
    }

    #[test]
    fn a_stage_reached_only_from_one_no_choice_reaches_is_unreachable_too() {
        let mut heist = heist();
        heist.stages[0].choices[0].next = Next::End;
        assert_eq!(
            problems(&quests(vec![heist], vec![])),
            [
                QuestProblem::UnreachableStage {
                    quest: quest_id("heist"),
                    stage: 1,
                },
                QuestProblem::UnreachableStage {
                    quest: quest_id("heist"),
                    stage: 2,
                },
            ]
        );
    }

    #[test]
    fn a_gate_needing_membership_or_rank_and_not_member_of_one_faction_never_holds() {
        let mut heist = heist();
        heist.requires.member = vec![faction("guild")];
        heist.requires.not_member = vec![faction("watch"), faction("guild")];
        heist.stages[1].requires.not_member = vec![faction("watch")];
        heist.stages[1].requires.rank_at_least =
            [(faction("watch"), RankId::new("sergeant").expect("valid"))].into();
        let mut errand = simple("errand");
        errand.requires.not_member = vec![faction("temple")];
        let mut first = step(&["errand", "chore"], None, Leftovers::Open);
        first.requires.member = vec![faction("temple")];
        first.requires.not_member = vec![faction("watch")];
        let mut chore = simple("chore");
        chore.requires.rank_at_least =
            [(faction("watch"), RankId::new("recruit").expect("valid"))].into();
        assert_eq!(
            problems(&quests(
                vec![heist, errand, chore],
                vec![("work", vec![first])]
            )),
            [
                QuestProblem::ContradictoryGate {
                    gate: Gate::Quest(quest_id("errand")),
                    index: 0,
                    faction: faction("temple"),
                    needs: Needs::Member,
                },
                QuestProblem::ContradictoryGate {
                    gate: Gate::Quest(quest_id("heist")),
                    index: 1,
                    faction: faction("guild"),
                    needs: Needs::Member,
                },
                QuestProblem::ContradictoryGate {
                    gate: Gate::Stage {
                        quest: quest_id("heist"),
                        stage: 1,
                    },
                    index: 0,
                    faction: faction("watch"),
                    needs: Needs::Rank,
                },
                QuestProblem::ContradictoryGate {
                    gate: Gate::Step {
                        questline: line_id("work"),
                        step: 0,
                    },
                    index: 0,
                    faction: faction("watch"),
                    needs: Needs::Rank,
                },
            ]
        );
    }

    #[test]
    fn a_quest_never_needs_its_own_progress_to_start() {
        let mut heist = heist();
        heist.requires = done(&["errand", "heist.case"]);
        let mut first = step(&["errand", "chore"], None, Leftovers::Open);
        first.requires = done(&["heist", "chore.go.done"]);
        assert_eq!(
            problems(&quests(
                vec![heist, simple("errand"), simple("chore")],
                vec![("work", vec![first])]
            )),
            [
                QuestProblem::OwnProgress {
                    gate: Gate::Quest(quest_id("heist")),
                    index: 1,
                    progress: progress("heist.case"),
                },
                QuestProblem::OwnProgress {
                    gate: Gate::Step {
                        questline: line_id("work"),
                        step: 0,
                    },
                    index: 1,
                    progress: progress("chore.go.done"),
                },
            ]
        );
    }

    #[test]
    fn a_stage_needs_only_its_own_progress_that_leads_there() {
        let mut heist = heist();
        heist.stages[2].requires = done(&[
            "heist.case",
            "heist.vault",
            "heist.case.scout",
            "heist.vault.crack",
        ]);
        assert_eq!(problems(&quests(vec![heist.clone()], vec![])), []);
        heist.stages[2].requires = done(&[
            "heist.escape",
            "heist.case.quit",
            "heist.vault.abort",
            "heist",
            "heist.escape.run",
        ]);
        heist.stages[1].requires = done(&["heist.escape"]);
        let own = |stage, index, text: &str| QuestProblem::OwnProgress {
            gate: Gate::Stage {
                quest: quest_id("heist"),
                stage,
            },
            index,
            progress: progress(text),
        };
        assert_eq!(
            problems(&quests(vec![heist], vec![])),
            [
                own(1, 0, "heist.escape"),
                own(2, 0, "heist.escape"),
                own(2, 1, "heist.case.quit"),
                own(2, 2, "heist.vault.abort"),
                own(2, 3, "heist"),
                own(2, 4, "heist.escape.run"),
            ]
        );
    }

    #[test]
    fn the_runs_wait_until_the_structure_is_sound() {
        let mut heist = heist();
        heist.requires = done(&["heist"]);
        let mut errand = simple("errand");
        errand.requires = done(&["heist"]);
        assert_eq!(
            problems(&quests(vec![heist, errand], vec![])),
            [QuestProblem::OwnProgress {
                gate: Gate::Quest(quest_id("heist")),
                index: 0,
                progress: progress("heist"),
            }]
        );
    }

    #[test]
    fn quests_waiting_on_each_other_never_start() {
        let mut heist = heist();
        heist.requires = done(&["errand.go.done"]);
        let mut errand = simple("errand");
        errand.requires = done(&["chore", "heist.vault"]);
        let mut chore = simple("chore");
        chore.requires = done(&["heist.case.scout"]);
        assert_eq!(
            problems(&quests(vec![heist, errand, chore.clone()], vec![])),
            [
                QuestProblem::NeverStarts {
                    quest: quest_id("chore"),
                    blocker: Blocker::Done(progress("heist.case.scout")),
                },
                QuestProblem::NeverStarts {
                    quest: quest_id("errand"),
                    blocker: Blocker::Done(progress("chore")),
                },
                QuestProblem::NeverStarts {
                    quest: quest_id("heist"),
                    blocker: Blocker::Done(progress("errand.go.done")),
                },
            ]
        );
        // Break the circle, and everything can start.
        chore.requires = Requirements::default();
        let mut heist = self::heist();
        heist.requires = done(&["errand.go.done"]);
        let mut errand = simple("errand");
        errand.requires = done(&["chore", "chore.go", "chore.go.done"]);
        assert_eq!(problems(&quests(vec![heist, errand, chore], vec![])), []);
    }

    #[test]
    fn a_step_that_can_never_be_complete_holds_back_every_later_one() {
        let mut errand = simple("errand");
        errand.requires = done(&["finale"]);
        let lines = vec![(
            "work",
            vec![
                step(&["errand", "chore"], Some(2), Leftovers::Open),
                step(&["extra"], Some(0), Leftovers::Open),
                step(&["finale"], None, Leftovers::Open),
            ],
        )];
        let blocked = |quest: &str| QuestProblem::NeverStarts {
            quest: quest_id(quest),
            blocker: Blocker::Step {
                questline: line_id("work"),
                step: 0,
            },
        };
        assert_eq!(
            problems(&quests(
                vec![
                    errand.clone(),
                    simple("chore"),
                    simple("extra"),
                    simple("finale")
                ],
                lines.clone()
            )),
            [
                QuestProblem::NeverStarts {
                    quest: quest_id("errand"),
                    blocker: Blocker::Done(progress("finale")),
                },
                blocked("extra"),
                blocked("finale"),
            ]
        );
        // Needing one of the two is enough to move on.
        let mut lines = lines;
        lines[0].1[0].need = Some(1);
        assert_eq!(
            problems(&quests(
                vec![errand, simple("chore"), simple("extra"), simple("finale")],
                lines
            )),
            []
        );
    }

    #[test]
    fn a_quest_its_own_steps_leftovers_close_before_it_can_start_never_starts() {
        let mut errand = simple("errand");
        errand.requires = done(&["finale"]);
        let work = |leftovers, need| {
            vec![(
                "work",
                vec![
                    step(&["errand", "chore"], need, leftovers),
                    step(&["finale"], None, Leftovers::Open),
                ],
            )]
        };
        let all = || vec![errand.clone(), simple("chore"), simple("finale")];
        assert_eq!(
            problems(&quests(all(), work(Leftovers::Close, Some(1)))),
            [QuestProblem::NeverStarts {
                quest: quest_id("errand"),
                blocker: Blocker::Closed {
                    questline: line_id("work"),
                    step: 0,
                },
            }]
        );
        assert_eq!(problems(&quests(all(), work(Leftovers::Open, Some(1)))), []);
        // A step that needs every quest has no leftovers; nor does the last step.
        assert_eq!(
            problems(&quests(all(), work(Leftovers::Close, None)))
                .first()
                .map(|problem| matches!(
                    problem,
                    QuestProblem::NeverStarts {
                        blocker: Blocker::Done(_),
                        ..
                    }
                )),
            Some(true)
        );
        let last = vec![(
            "work",
            vec![
                step(&["chore"], None, Leftovers::Open),
                step(&["errand", "finale"], Some(1), Leftovers::Close),
            ],
        )];
        assert_eq!(problems(&quests(all(), last)), []);
    }

    #[test]
    fn a_stage_whose_done_never_happens_is_reported_at_its_first_such_need() {
        let mut heist = heist();
        heist.stages[1].requires = done(&["errand", "chore"]);
        let mut chore = simple("chore");
        chore.requires = done(&["heist.escape"]);
        assert_eq!(
            problems(&quests(vec![heist, simple("errand"), chore], vec![])),
            [
                QuestProblem::NeverStarts {
                    quest: quest_id("chore"),
                    blocker: Blocker::Done(progress("heist.escape")),
                },
                QuestProblem::StageNeverReached {
                    quest: quest_id("heist"),
                    stage: 1,
                    index: 1,
                    progress: progress("chore"),
                },
            ]
        );
    }

    #[test]
    fn a_stage_needing_what_comes_only_after_its_quest_is_over_is_never_reached() {
        let mut heist = heist();
        heist.stages[1].requires = done(&["errand"]);
        let mut errand = simple("errand");
        errand.requires = done(&["heist"]);
        assert_eq!(
            problems(&quests(vec![heist.clone(), errand], vec![])),
            [QuestProblem::OnlyAfter {
                quest: quest_id("heist"),
                stage: 1,
                index: 0,
                progress: progress("errand"),
            }]
        );
        // Needing the stage before it, through another quest, is fine.
        let mut errand = simple("errand");
        errand.requires = done(&["heist.case.scout"]);
        assert_eq!(problems(&quests(vec![heist.clone(), errand], vec![])), []);
        // So is needing a stage on the way there.
        heist.stages[2].requires = done(&["errand"]);
        heist.stages[1].requires = Requirements::default();
        let mut errand = simple("errand");
        errand.requires = done(&["heist.vault"]);
        assert_eq!(problems(&quests(vec![heist.clone(), errand], vec![])), []);
        // But not a choice that leaves the way.
        let mut errand = simple("errand");
        errand.requires = done(&["heist.vault.abort"]);
        assert_eq!(
            problems(&quests(vec![heist, errand], vec![])),
            [QuestProblem::OnlyAfter {
                quest: quest_id("heist"),
                stage: 2,
                index: 0,
                progress: progress("errand"),
            }]
        );
    }

    #[test]
    fn leads_lists_every_stage_a_path_reaches() {
        let leads = leads(&heist());
        assert_eq!(
            leads,
            [BTreeSet::from([1, 2]), BTreeSet::from([2]), BTreeSet::new()]
        );
    }

    #[test]
    fn reachability_problems_say_what_is_wrong_in_a_designers_words() {
        let stage = |stage| Gate::Stage {
            quest: quest_id("heist"),
            stage,
        };
        let messages: Vec<String> = [
            QuestProblem::UnreachableStage {
                quest: quest_id("heist"),
                stage: 1,
            },
            QuestProblem::ContradictoryGate {
                gate: Gate::Quest(quest_id("heist")),
                index: 0,
                faction: faction("guild"),
                needs: Needs::Member,
            },
            QuestProblem::ContradictoryGate {
                gate: stage(1),
                index: 0,
                faction: faction("watch"),
                needs: Needs::Rank,
            },
            QuestProblem::OwnProgress {
                gate: stage(2),
                index: 0,
                progress: progress("heist"),
            },
            QuestProblem::OwnProgress {
                gate: stage(2),
                index: 0,
                progress: progress("heist.case.quit"),
            },
            QuestProblem::OwnProgress {
                gate: Gate::Quest(quest_id("heist")),
                index: 0,
                progress: progress("heist.case"),
            },
            QuestProblem::OwnProgress {
                gate: Gate::Step {
                    questline: line_id("work"),
                    step: 0,
                },
                index: 0,
                progress: progress("chore.go"),
            },
            QuestProblem::NeverStarts {
                quest: quest_id("heist"),
                blocker: Blocker::Done(progress("errand.go")),
            },
            QuestProblem::NeverStarts {
                quest: quest_id("heist"),
                blocker: Blocker::Step {
                    questline: line_id("work"),
                    step: 0,
                },
            },
            QuestProblem::NeverStarts {
                quest: quest_id("heist"),
                blocker: Blocker::Closed {
                    questline: line_id("work"),
                    step: 1,
                },
            },
            QuestProblem::StageNeverReached {
                quest: quest_id("heist"),
                stage: 1,
                index: 0,
                progress: progress("chore"),
            },
            QuestProblem::OnlyAfter {
                quest: quest_id("heist"),
                stage: 1,
                index: 0,
                progress: progress("errand"),
            },
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        assert_eq!(
            messages,
            [
                "no choice leads to this stage, so it can never be reached",
                "it also needs to be in guild, so it can never hold",
                "it also needs a rank in watch, so it can never hold",
                "heist can't be over while one of its own stages is under way",
                "heist.case.quit doesn't lead to this stage, so it can't be done here",
                "a quest can't need its own progress to start",
                "chore is in this step, and a quest can't need its own progress to start",
                "it can never start: it needs errand.go done, which can never happen",
                "it can never start: work.steps[0] can never be complete",
                "it can never start: what it needs only comes after work moves on from steps[1], which closes it",
                "chore can never happen, so this stage can never be reached",
                "errand can only happen once heist is over, so this stage can never be reached",
            ]
        );
    }

    #[test]
    fn a_blocked_stage_is_reported_but_not_the_stages_only_it_leads_to() {
        let mut heist = heist();
        heist.stages[0].requires = done(&["chore"]);
        heist.stages[1].requires = done(&["chore"]);
        heist.stages[2].requires = done(&["chore.go"]);
        let mut chore = simple("chore");
        chore.requires = done(&["errand.go.done"]);
        let mut errand = simple("errand");
        errand.requires = done(&["chore.go"]);
        let reached = |stage, text: &str| QuestProblem::StageNeverReached {
            quest: quest_id("heist"),
            stage,
            index: 0,
            progress: progress(text),
        };
        assert_eq!(
            problems(&quests(
                vec![heist.clone(), chore.clone(), errand.clone()],
                vec![]
            ))[2..],
            [reached(0, "chore")]
        );
        heist.stages[0].requires = Requirements::default();
        assert_eq!(
            problems(&quests(vec![heist, chore, errand], vec![]))[2..],
            [reached(1, "chore")]
        );
    }

    #[test]
    fn only_a_quests_own_choices_lead_to_its_stages() {
        // The heist's `case` is blocked, so its `vault` isn't reported, even though the
        // errand reaches a `vault` of its own.
        let mut heist = heist();
        heist.stages[0].requires = done(&["chore"]);
        heist.stages[1].requires = done(&["chore"]);
        let mut chore = simple("chore");
        chore.requires = done(&["chore_two"]);
        let mut chore_two = simple("chore_two");
        chore_two.requires = done(&["chore"]);
        let errand = quest(
            "errand",
            vec![
                stage("start", &[("go", Some("vault"))]),
                stage("vault", &[("done", None)]),
            ],
        );
        assert_eq!(
            problems(&quests(vec![heist, chore, chore_two, errand], vec![]))[2..],
            [QuestProblem::StageNeverReached {
                quest: quest_id("heist"),
                stage: 0,
                index: 0,
                progress: progress("chore"),
            }]
        );
    }

    #[test]
    fn closing_leftovers_shuts_only_its_own_questlines_later_steps() {
        let mut errand = simple("errand");
        errand.requires = done(&["finale"]);
        let lines = vec![
            (
                "work",
                vec![
                    step(&["errand", "chore"], Some(1), Leftovers::Close),
                    step(&["extra"], None, Leftovers::Open),
                ],
            ),
            (
                "other",
                vec![
                    step(&["prelude"], None, Leftovers::Open),
                    step(&["finale"], None, Leftovers::Open),
                ],
            ),
        ];
        assert_eq!(
            problems(&quests(
                vec![
                    errand,
                    simple("chore"),
                    simple("extra"),
                    simple("prelude"),
                    simple("finale")
                ],
                lines
            )),
            []
        );
    }
}
