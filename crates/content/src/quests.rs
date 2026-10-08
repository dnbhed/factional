//! Reading `quests.toml` and `questlines.toml` (DESIGN.md §17.1), and putting each quest
//! problem at its file and key. Whether what they name exists is the quest module's check
//! (P-32); this only reads.

use std::collections::BTreeMap;

use factional_core::InvalidId;
use factional_quests::{
    Choice, ChoiceAt, ChoiceEffects, ChoiceId, Gate, Leftovers, Next, Owner, PartyRef, Progress,
    Quest, QuestId, QuestProblem, QuestWarning, Questline, QuestlineId, RequirementKey,
    Requirements, Stage, StageId, Step,
};
use factional_reputation::{Effects, FactionId, OutcomeId, Party, RankId};
use toml::{Table, Value};

use crate::reader::{Report, Section};
use crate::{Diagnostic, STANDING_EXAMPLE, read_delta, read_named_standing, read_tables};

pub(crate) const QUESTS_FILE: &str = "quests.toml";
pub(crate) const QUESTLINES_FILE: &str = "questlines.toml";

const REQUIRES_EXAMPLE: &str =
    "{ standing = { city_watch = 10.0 }, not_member = [\"lantern_guild\"] }";
const EFFECTS_EXAMPLE: &str =
    "{ alignment = { law = 2.0 }, standing = { factions = { city_watch = 5.0 } } }";

/// `quests.toml`: each quest, keyed by its id. A quest with any problem is reported and left
/// out.
pub(crate) fn read_quests(text: &str, report: &mut Report) -> BTreeMap<QuestId, Quest> {
    read_tables(text, report, "quest", QuestId::new, read_quest)
}

/// `questlines.toml`: each questline, keyed by its id. A questline with any problem is
/// reported and left out.
pub(crate) fn read_questlines(text: &str, report: &mut Report) -> BTreeMap<QuestlineId, Questline> {
    read_tables(text, report, "questline", QuestlineId::new, read_questline)
}

fn read_quest(id: QuestId, fields: &Table, report: &mut Report) -> Option<Quest> {
    let errors = report.diagnostics.len();
    let mut section = Section::new(fields, id.to_string());
    let name = section.text("name", report);
    let giver = read_giver(&mut section, report);
    let requires = read_requires(&mut section, report);
    let stages = read_list(
        &mut section,
        "stages",
        "[[<quest>.stages]]",
        report,
        read_stage,
    );
    section.finish(report);
    if report.diagnostics.len() != errors {
        return None;
    }
    Some(Quest {
        id,
        name: name?,
        giver,
        requires,
        stages: stages?,
    })
}

fn read_stage(mut section: Section<'_>, report: &mut Report) -> Option<Stage> {
    let errors = report.diagnostics.len();
    let id = read_id(&mut section, "id", StageId::new, report);
    let requires = read_requires(&mut section, report);
    let choices = read_list(
        &mut section,
        "choices",
        "[{ id = \"report\", next = \"end\" }]",
        report,
        read_choice,
    );
    section.finish(report);
    if report.diagnostics.len() != errors {
        return None;
    }
    Some(Stage {
        id: id?,
        requires,
        choices: choices?,
    })
}

fn read_choice(mut section: Section<'_>, report: &mut Report) -> Option<Choice> {
    let errors = report.diagnostics.len();
    let id = read_id(&mut section, "id", ChoiceId::new, report);
    let next_at = section.path_to("next");
    let next = section.text("next", report).and_then(|text| {
        if text == Next::END {
            Some(Next::End)
        } else {
            StageId::new(&text)
                .map(Next::Stage)
                .map_err(|invalid| report.error(&next_at, invalid.to_string()))
                .ok()
        }
    });
    let outcome_at = section.path_to("outcome");
    let outcome = section.optional_text("outcome", report).and_then(|text| {
        OutcomeId::new(&text)
            .map_err(|invalid| report.error(&outcome_at, invalid.to_string()))
            .ok()
    });
    let inline = section
        .optional_table("effects", EFFECTS_EXAMPLE, report)
        .map(|mut block| {
            let alignment = read_delta(&mut block, report);
            let standing = block
                .optional_table("standing", STANDING_EXAMPLE, report)
                .map(|standing| read_named_standing(standing, report))
                .unwrap_or_default();
            block.finish(report);
            Effects {
                alignment,
                standing,
            }
        });
    let effects = match (outcome, inline) {
        (None, None) => ChoiceEffects::None,
        (Some(outcome), None) => ChoiceEffects::Outcome(outcome),
        (None, Some(effects)) => ChoiceEffects::Inline(effects),
        (Some(_), Some(_)) => {
            report.error(
                section.path(),
                "a choice has an outcome or effects, not both",
            );
            ChoiceEffects::None
        }
    };
    section.finish(report);
    if report.diagnostics.len() != errors {
        return None;
    }
    Some(Choice {
        id: id?,
        effects,
        next: next?,
    })
}

fn read_questline(id: QuestlineId, fields: &Table, report: &mut Report) -> Option<Questline> {
    let errors = report.diagnostics.len();
    let mut section = Section::new(fields, id.to_string());
    let name = section.text("name", report);
    let giver = read_giver(&mut section, report);
    let steps = read_list(
        &mut section,
        "steps",
        "[[<questline>.steps]]",
        report,
        read_step,
    );
    section.finish(report);
    if report.diagnostics.len() != errors {
        return None;
    }
    Some(Questline {
        id,
        name: name?,
        giver,
        steps: steps?,
    })
}

fn read_step(mut section: Section<'_>, report: &mut Report) -> Option<Step> {
    let errors = report.diagnostics.len();
    let path = section.path_to("quests");
    let quests = section
        .list("quests", "[\"watch_oath\"]", report)
        .map(|items| read_ids(items, &path, "a quest's id", QuestId::new, report));
    let need = section
        .optional_whole("need", report)
        .and_then(|need| usize::try_from(need).ok());
    let requires = read_requires(&mut section, report);
    let leftovers_at = section.path_to("leftovers");
    let leftovers = match section.optional_text("leftovers", report) {
        None => Leftovers::default(),
        Some(text) => Leftovers::ALL
            .into_iter()
            .find(|leftovers| leftovers.key() == text)
            .unwrap_or_else(|| {
                let keys: Vec<&str> = Leftovers::ALL.map(Leftovers::key).to_vec();
                report.error(
                    &leftovers_at,
                    format!("unknown leftovers '{text}': use {}", keys.join(" or ")),
                );
                Leftovers::default()
            }),
    };
    section.finish(report);
    if report.diagnostics.len() != errors {
        return None;
    }
    Some(Step {
        quests: quests?,
        need,
        requires,
        leftovers,
    })
}

/// An optional `giver`: a faction or character id.
fn read_giver(section: &mut Section<'_>, report: &mut Report) -> Option<PartyRef> {
    let at = section.path_to("giver");
    let text = section.optional_text("giver", report)?;
    PartyRef::new(&text)
        .map_err(|invalid| report.error(&at, invalid.to_string()))
        .ok()
}

/// A required id under `key`, such as a stage's `id`.
fn read_id<Id>(
    section: &mut Section<'_>,
    key: &'static str,
    new_id: impl Fn(&str) -> Result<Id, InvalidId>,
    report: &mut Report,
) -> Option<Id> {
    let at = section.path_to(key);
    let text = section.text(key, report)?;
    new_id(&text)
        .map_err(|invalid| report.error(&at, invalid.to_string()))
        .ok()
}

/// Each item of a list of ids in quotes, such as a step's quests; any that isn't one is
/// reported and left out.
fn read_ids<Id>(
    items: &[Value],
    path: &str,
    what: &str,
    new_id: impl Fn(&str) -> Result<Id, InvalidId>,
    report: &mut Report,
) -> Vec<Id> {
    items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let at = format!("{path}[{index}]");
            let Value::String(text) = item else {
                report.error(&at, format!("expected {what} in quotes"));
                return None;
            };
            new_id(text)
                .map_err(|invalid| report.error(&at, invalid.to_string()))
                .ok()
        })
        .collect()
}

/// A list of tables under `key`, such as `[[watch_oath.stages]]`, each read by `read`; empty
/// if it's left out. `None` if any can't be read.
fn read_list<'t, T>(
    section: &mut Section<'t>,
    key: &'static str,
    example: &str,
    report: &mut Report,
    read: impl Fn(Section<'t>, &mut Report) -> Option<T>,
) -> Option<Vec<T>> {
    let path = section.path_to(key);
    let items = match section.optional_value(key) {
        None => return Some(Vec::new()),
        Some(Value::Array(items)) => items,
        Some(_) => {
            report.error(&path, format!("expected a list of tables, like {example}"));
            return None;
        }
    };
    let read: Vec<Option<T>> = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let at = format!("{path}[{index}]");
            match item {
                Value::Table(fields) => read(Section::new(fields, at), report),
                _ => {
                    report.error(&at, format!("expected a table, like {example}"));
                    None
                }
            }
        })
        .collect();
    read.into_iter().collect()
}

/// An optional `requires` table; nothing is required if it's left out. Whether what it
/// names exists is the quest module's check (P-32).
fn read_requires(section: &mut Section<'_>, report: &mut Report) -> Requirements {
    let mut requires = Requirements::default();
    let Some(mut table) = section.optional_table("requires", REQUIRES_EXAMPLE, report) else {
        return requires;
    };
    if let Some(mut standing) = table.optional_table("standing", "{ city_watch = 10.0 }", report) {
        for key in standing.keys() {
            match PartyRef::new(&key) {
                Ok(party) => {
                    if let Some(value) = standing.fixed_any(&key, report) {
                        requires.standing.insert(party, value);
                    }
                }
                Err(invalid) => {
                    standing.mark(&key);
                    report.error(standing.path(), invalid.to_string());
                }
            }
        }
        standing.finish(report);
    }
    let factions = |table: &mut Section<'_>, key: &'static str, report: &mut Report| {
        let path = table.path_to(key);
        table
            .optional_list(key, "[\"temple\"]", report)
            .map(|items| read_ids(items, &path, "a faction's id", FactionId::new, report))
            .unwrap_or_default()
    };
    requires.member = factions(&mut table, "member", report);
    requires.not_member = factions(&mut table, "not_member", report);
    if let Some(mut ranks) =
        table.optional_table("rank_at_least", "{ city_watch = \"sergeant\" }", report)
    {
        for key in ranks.keys() {
            let at = ranks.path_to(&key);
            let faction = FactionId::new(&key).map_err(|invalid| invalid.to_string());
            let rank = match ranks.value_any(&key) {
                Some(Value::String(text)) => {
                    RankId::new(text).map_err(|invalid| invalid.to_string())
                }
                _ => Err("expected a rank id in quotes, like \"sergeant\"".to_owned()),
            };
            match (faction, rank) {
                (Ok(faction), Ok(rank)) => {
                    requires.rank_at_least.insert(faction, rank);
                }
                (Err(message), _) | (_, Err(message)) => report.error(&at, message),
            }
        }
        ranks.finish(report);
    }
    requires.within_tolerance = factions(&mut table, "within_tolerance", report);
    let done_path = table.path_to("done");
    if let Some(items) = table.optional_list("done", "[\"watch_oath.patrol\"]", report) {
        for (index, item) in items.iter().enumerate() {
            let at = format!("{done_path}[{index}]");
            let Value::String(text) = item else {
                report.error(
                    &at,
                    "expected a quest, stage or choice in quotes, like \"watch_oath.patrol\"",
                );
                continue;
            };
            match Progress::parse(text) {
                Ok(progress) => requires.done.push(progress),
                Err(problem) => report.error(&at, problem.to_string()),
            }
        }
    }
    table.finish(report);
    requires
}

/// A quest problem at its file and key.
pub(crate) fn problem_diagnostic(problem: &QuestProblem) -> Diagnostic {
    let (file, key) = match problem {
        QuestProblem::UnknownGiver {
            owner: Owner::Quest(quest),
            ..
        } => (QUESTS_FILE, format!("{quest}.giver")),
        QuestProblem::UnknownGiver {
            owner: Owner::Questline(questline),
            ..
        } => (QUESTLINES_FILE, format!("{questline}.giver")),
        QuestProblem::NoStages(quest) => (QUESTS_FILE, format!("{quest}.stages")),
        QuestProblem::DuplicateStage { quest, stage, .. }
        | QuestProblem::StageCalledEnd { quest, stage } => {
            (QUESTS_FILE, format!("{quest}.stages[{stage}].id"))
        }
        QuestProblem::NoChoices { quest, stage } => {
            (QUESTS_FILE, format!("{quest}.stages[{stage}].choices"))
        }
        QuestProblem::DuplicateChoice { at, .. } => (QUESTS_FILE, format!("{}.id", choice(at))),
        QuestProblem::UnknownOutcome { at, .. } => (QUESTS_FILE, format!("{}.outcome", choice(at))),
        QuestProblem::UnknownEffectParty { at, party, .. }
        | QuestProblem::EffectOutOfRange { at, party, .. } => {
            let kind = match party {
                Party::Faction(_) => "factions",
                Party::Character(_) => "characters",
            };
            (
                QUESTS_FILE,
                format!("{}.effects.standing.{kind}.{party}", choice(at)),
            )
        }
        QuestProblem::UnknownNext { at, .. } | QuestProblem::BackwardNext { at, .. } => {
            (QUESTS_FILE, format!("{}.next", choice(at)))
        }
        QuestProblem::UnknownParty { gate, party, .. }
        | QuestProblem::RequirementOutOfRange { gate, party, .. } => {
            requirement(gate, &RequirementKey::Standing(party.clone()))
        }
        QuestProblem::UnknownFaction { gate, key, .. }
        | QuestProblem::Repeated { gate, key, .. } => requirement(gate, key),
        QuestProblem::UnknownRank { gate, faction, .. } => {
            requirement(gate, &RequirementKey::RankAtLeast(faction.clone()))
        }
        QuestProblem::UnknownProgress { gate, index, .. } => {
            requirement(gate, &RequirementKey::Done(*index))
        }
        QuestProblem::NoSteps(questline) => (QUESTLINES_FILE, format!("{questline}.steps")),
        QuestProblem::EmptyStep { questline, step } => {
            (QUESTLINES_FILE, format!("{questline}.steps[{step}].quests"))
        }
        QuestProblem::UnknownQuest {
            questline,
            step,
            index,
            ..
        }
        | QuestProblem::QuestRepeated {
            questline,
            step,
            index,
            ..
        } => (
            QUESTLINES_FILE,
            format!("{questline}.steps[{step}].quests[{index}]"),
        ),
        QuestProblem::NeedTooMany {
            questline, step, ..
        } => (QUESTLINES_FILE, format!("{questline}.steps[{step}].need")),
    };
    Diagnostic {
        file: file.to_owned(),
        key: Some(key),
        message: problem.to_string(),
    }
}

/// A quest warning at its file and key.
pub(crate) fn warning_diagnostic(warning: &QuestWarning) -> Diagnostic {
    match warning {
        QuestWarning::LeftoversNeverLeft { questline, step } => Diagnostic {
            file: QUESTLINES_FILE.to_owned(),
            key: Some(format!("{questline}.steps[{step}].leftovers")),
            message: warning.to_string(),
        },
    }
}

/// Where a choice is in `quests.toml`.
fn choice(at: &ChoiceAt) -> String {
    format!("{}.stages[{}].choices[{}]", at.quest, at.stage, at.choice)
}

/// Where one requirement is: its file, and its key under the `requires` table.
fn requirement(gate: &Gate, key: &RequirementKey) -> (&'static str, String) {
    let (file, requires) = match gate {
        Gate::Quest(quest) => (QUESTS_FILE, format!("{quest}.requires")),
        Gate::Stage { quest, stage } => (QUESTS_FILE, format!("{quest}.stages[{stage}].requires")),
        Gate::Step { questline, step } => (
            QUESTLINES_FILE,
            format!("{questline}.steps[{step}].requires"),
        ),
    };
    let key = match key {
        RequirementKey::Standing(party) => format!("standing.{party}"),
        RequirementKey::Member(index) => format!("member[{index}]"),
        RequirementKey::NotMember(index) => format!("not_member[{index}]"),
        RequirementKey::RankAtLeast(faction) => format!("rank_at_least.{faction}"),
        RequirementKey::WithinTolerance(index) => format!("within_tolerance[{index}]"),
        RequirementKey::Done(index) => format!("done[{index}]"),
    };
    (file, format!("{requires}.{key}"))
}
