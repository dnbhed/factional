//! Loads designer content (TOML) from disk, validates it, and turns mistakes into diagnostics
//! that name the file and the key path (DESIGN.md §12.1).

mod edit;
mod outline;
mod quests;
mod reader;
mod save;
mod schema;

pub use edit::{EditError, Field, Step, ValueKind, ValuePath, entry_fields, set_value};
pub use outline::{
    CONTENT_FILES, ContentTexts, FileState, Outline, OutlineEntry, OutlineFile, outline,
    outline_texts, read_texts,
};
pub use save::{Fingerprint, Restored, SAVE_VERSION, SaveError, fingerprint_of, restore, save};
pub use schema::{SCHEMA_FILES, schema, schema_text};

use std::collections::BTreeMap;
use std::path::Path;
use std::{fmt, fs, io};

use factional_core::{Curve, Fixed, suggest};
use factional_quests::Quests;
use factional_reputation::{
    Action, ActionId, ActionStanding, Alignment, AlignmentDelta, Axis, Balance, Band, BandProblem,
    Bands, Character, CharacterId, ComponentKind, Condition, ConflictRule, Consequence, Content,
    ContentProblem, ContentWarning, DispositionWeights, DriftPolicy, Effects, Faction, FactionId,
    Inertia, InertiaProfile, InvalidId, KnowledgeModel, Metric, Outcome, OutcomeId, Party,
    ProfileId, ProfileUser, Rank, RankId, RankKey, RankRef, Relation, RelationEnds, RelationShift,
    RelationSide, Rule, ShiftProblem, StandingEffects, StandingKey, StandingOwner,
    StartingMembership, TableKind, TableOwner, TableProblem, TargetCurve, ToleranceProblem,
    Tolerances, Toward, Verdict, WeightProblem, Weights,
};
use reader::{Report, Section};
use serde::Deserialize;
use toml::Value;

/// One problem in the content, such as
/// `characters.toml: vex.alignment.law: 120.00 is outside -100.00..100.00`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub file: String,
    /// Where in the file, like `vex.alignment.law`; `None` for the file as a whole.
    pub key: Option<String>,
    pub message: String,
}

/// Every problem found, so a designer can fix them all in one pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentError {
    pub diagnostics: Vec<Diagnostic>,
}

/// The text of each content file; `None` for a file that isn't there. A missing file means
/// the defaults (for `balance.toml`) or nothing of that kind (for the others).
#[derive(Debug, Clone, Copy, Default)]
pub struct Sources<'a> {
    pub balance: Option<&'a str>,
    pub factions: Option<&'a str>,
    pub characters: Option<&'a str>,
    pub actions: Option<&'a str>,
    pub relations: Option<&'a str>,
    pub outcomes: Option<&'a str>,
    pub quests: Option<&'a str>,
    pub questlines: Option<&'a str>,
}

const BALANCE_FILE: &str = "balance.toml";
const FACTIONS_FILE: &str = "factions.toml";
const CHARACTERS_FILE: &str = "characters.toml";
const ACTIONS_FILE: &str = "actions.toml";
const RELATIONS_FILE: &str = "relations.toml";
const OUTCOMES_FILE: &str = "outcomes.toml";

/// The text of each content file in a directory, as read; `None` for a missing file.
struct Texts {
    balance: Option<String>,
    factions: Option<String>,
    characters: Option<String>,
    actions: Option<String>,
    relations: Option<String>,
    outcomes: Option<String>,
    quests: Option<String>,
    questlines: Option<String>,
}

impl Texts {
    /// The text of one content file, by name; `None` if it isn't there.
    fn of(&self, file: &str) -> Option<&str> {
        let text = match file {
            BALANCE_FILE => &self.balance,
            FACTIONS_FILE => &self.factions,
            CHARACTERS_FILE => &self.characters,
            ACTIONS_FILE => &self.actions,
            RELATIONS_FILE => &self.relations,
            OUTCOMES_FILE => &self.outcomes,
            quests::QUESTS_FILE => &self.quests,
            quests::QUESTLINES_FILE => &self.questlines,
            _ => return None,
        };
        text.as_deref()
    }

    /// Reads every content file in `dir`.
    fn read(dir: &Path) -> Result<Texts, ContentError> {
        let unreadable = |error: io::Error| ContentError {
            diagnostics: vec![Diagnostic {
                file: dir.display().to_string(),
                key: None,
                message: format!("cannot read the directory: {error}"),
            }],
        };
        if let Err(error) = fs::read_dir(dir) {
            return Err(unreadable(error));
        }
        let read = |file: &str| match fs::read_to_string(dir.join(file)) {
            Ok(text) => Ok(Some(text)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(ContentError {
                diagnostics: vec![Diagnostic {
                    file: file.to_owned(),
                    key: None,
                    message: format!("cannot read the file: {error}"),
                }],
            }),
        };
        Ok(Texts {
            balance: read(BALANCE_FILE)?,
            factions: read(FACTIONS_FILE)?,
            characters: read(CHARACTERS_FILE)?,
            actions: read(ACTIONS_FILE)?,
            outcomes: read(OUTCOMES_FILE)?,
            relations: read(RELATIONS_FILE)?,
            quests: read(quests::QUESTS_FILE)?,
            questlines: read(quests::QUESTLINES_FILE)?,
        })
    }

    fn sources(&self) -> Sources<'_> {
        Sources {
            balance: self.balance.as_deref(),
            factions: self.factions.as_deref(),
            characters: self.characters.as_deref(),
            actions: self.actions.as_deref(),
            relations: self.relations.as_deref(),
            outcomes: self.outcomes.as_deref(),
            quests: self.quests.as_deref(),
            questlines: self.questlines.as_deref(),
        }
    }
}

/// Reads and validates the content files in `dir`, quests included, with a fingerprint of
/// exactly what was read, for saves (T4).
pub fn load_dir_fingerprinted(dir: &Path) -> Result<(Content, Quests, Fingerprint), ContentError> {
    let texts = Texts::read(dir)?;
    let fingerprint = Fingerprint::of([
        (BALANCE_FILE, texts.balance.as_deref()),
        (FACTIONS_FILE, texts.factions.as_deref()),
        (CHARACTERS_FILE, texts.characters.as_deref()),
        (ACTIONS_FILE, texts.actions.as_deref()),
        (OUTCOMES_FILE, texts.outcomes.as_deref()),
        (RELATIONS_FILE, texts.relations.as_deref()),
        (quests::QUESTS_FILE, texts.quests.as_deref()),
        (quests::QUESTLINES_FILE, texts.questlines.as_deref()),
    ]);
    let (content, quests) = parse_quests(texts.sources())?;
    Ok((content, quests, fingerprint))
}

/// Reads and validates the content files in `dir`, quests included, keeping the content.
pub fn load_dir(dir: &Path) -> Result<Content, ContentError> {
    load_dir_fingerprinted(dir).map(|(content, ..)| content)
}

/// Reads and validates the content files in `dir` with their quests, as `parse_quests` does.
pub fn load_quests(dir: &Path) -> Result<(Content, Quests), ContentError> {
    parse_quests(Texts::read(dir)?.sources())
}

/// Validates content from the text of its files, reporting every problem at once: each
/// file's own, in file order, then problems across files (P-32), then the quests' (Q1 to Q4),
/// keeping the content. A world loads only if its quests reconcile too (D-20).
pub fn parse_content(sources: Sources<'_>) -> Result<Content, ContentError> {
    parse_quests(sources).map(|(content, _)| content)
}

/// Validates content and its quests from the text of their files, reporting every problem
/// at once, as `parse_content` does, keeping both.
pub fn parse_quests(sources: Sources<'_>) -> Result<(Content, Quests), ContentError> {
    let read = read_all(sources);
    if read.diagnostics.is_empty() {
        Ok((read.content, read.quests))
    } else {
        Err(ContentError {
            diagnostics: read.diagnostics,
        })
    }
}

/// Quests' warnings against `content`: things allowed but probably not meant, each with its
/// file and key.
pub fn quest_warnings(content: &Content, quests: &Quests) -> Vec<Diagnostic> {
    quests
        .warnings(content)
        .iter()
        .map(quests::warning_diagnostic)
        .collect()
}

/// Everything read from the files, and every problem found.
struct Read {
    content: Content,
    quests: Quests,
    diagnostics: Vec<Diagnostic>,
}

fn read_all(sources: Sources<'_>) -> Read {
    let mut reports = Vec::new();
    let balance = read_file(BALANCE_FILE, sources.balance, &mut reports, read_balance);
    let factions = read_file(
        FACTIONS_FILE,
        sources.factions,
        &mut reports,
        |text, report| read_tables(text, report, "faction", FactionId::new, read_faction),
    );
    let relations = read_file(
        RELATIONS_FILE,
        sources.relations,
        &mut reports,
        read_relations,
    );
    let characters = read_file(
        CHARACTERS_FILE,
        sources.characters,
        &mut reports,
        |text, report| read_tables(text, report, "character", CharacterId::new, read_character),
    );
    let actions = read_file(
        ACTIONS_FILE,
        sources.actions,
        &mut reports,
        |text, report| {
            read_tables(
                text,
                report,
                "action",
                ActionId::new,
                |id, fields, report| Some(read_action(id, fields, report)),
            )
        },
    );
    let outcomes = read_file(
        OUTCOMES_FILE,
        sources.outcomes,
        &mut reports,
        |text, report| {
            read_tables(
                text,
                report,
                "outcome",
                OutcomeId::new,
                |id, fields, report| Some(read_outcome(id, fields, report)),
            )
        },
    );
    let content = Content {
        balance: balance.unwrap_or_default(),
        factions: factions.unwrap_or_default(),
        characters: characters.unwrap_or_default(),
        actions: actions.unwrap_or_default(),
        relations: relations.unwrap_or_default(),
        outcomes: outcomes.unwrap_or_default(),
    };

    let quest_list = read_file(
        quests::QUESTS_FILE,
        sources.quests,
        &mut reports,
        quests::read_quests,
    );
    let questlines = read_file(
        quests::QUESTLINES_FILE,
        sources.questlines,
        &mut reports,
        quests::read_questlines,
    );
    let quests = Quests {
        quests: quest_list.unwrap_or_default(),
        questlines: questlines.unwrap_or_default(),
    };

    let mut diagnostics: Vec<Diagnostic> = reports
        .into_iter()
        .flat_map(|report| report.diagnostics)
        .collect();
    let read_cleanly = diagnostics.is_empty();
    diagnostics.extend(content.problems().iter().map(|problem| {
        let (file, key) = match problem {
            ContentProblem::SharedId(id) => (FACTIONS_FILE, id.to_string()),
            ContentProblem::AffinityOutOfRange(_) => (BALANCE_FILE, "disposition.affinity".into()),
            ContentProblem::SpilloverOutOfRange(_) => (BALANCE_FILE, "standing.spillover".into()),
            ContentProblem::UnknownMembershipFaction {
                character, index, ..
            }
            | ContentProblem::DuplicateMembership {
                character, index, ..
            } => (
                CHARACTERS_FILE,
                format!("{character}.memberships[{index}].faction"),
            ),
            ContentProblem::ConflictThresholdOutOfRange(_) => {
                (BALANCE_FILE, "relations.conflict_threshold".into())
            }
            ContentProblem::UnknownRelationFaction { index, side, .. } => {
                let side = match side {
                    RelationSide::Between(end) => format!("between[{end}]"),
                    RelationSide::From => "from".to_owned(),
                    RelationSide::To => "to".to_owned(),
                };
                (RELATIONS_FILE, format!("relation[{index}].{side}"))
            }
            ContentProblem::SelfRelation { index }
            | ContentProblem::DuplicateRelation { index, .. } => {
                (RELATIONS_FILE, format!("relation[{index}]"))
            }
            ContentProblem::RelationOutOfRange { index, .. } => {
                (RELATIONS_FILE, format!("relation[{index}].value"))
            }
            ContentProblem::StartsInConflict {
                character, index, ..
            } => (
                CHARACTERS_FILE,
                format!("{character}.memberships[{index}].faction"),
            ),
            ContentProblem::UnknownStandingParty { owner, party, .. } => {
                let (file, at) = standing_owner(owner);
                let kind = match party {
                    Party::Faction(_) => "factions",
                    Party::Character(_) => "characters",
                };
                (file, format!("{at}.standing.{kind}.{party}"))
            }
            ContentProblem::StandingOutOfRange { owner, key, .. } => {
                let (file, at) = standing_owner(owner);
                let key = match key {
                    StandingKey::Target => "target".to_owned(),
                    StandingKey::TargetFactions => "target_factions".to_owned(),
                    StandingKey::Party(Party::Faction(id)) => format!("factions.{id}"),
                    StandingKey::Party(Party::Character(id)) => format!("characters.{id}"),
                };
                (file, format!("{at}.standing.{key}"))
            }
            ContentProblem::LeaveStandingOutOfRange { faction, .. } => {
                (FACTIONS_FILE, format!("{faction}.leave_standing_change"))
            }
            ContentProblem::ExpelStandingOutOfRange { faction, .. } => {
                (FACTIONS_FILE, format!("{faction}.expel_standing_change"))
            }
            ContentProblem::NegativeDispositionWeight { component, .. } => (
                BALANCE_FILE,
                format!("disposition.weights.{}", component.key()),
            ),
            ContentProblem::NoRanks(faction) => (FACTIONS_FILE, format!("{faction}.ranks")),
            ContentProblem::DuplicateRank { faction, index, .. } => {
                (FACTIONS_FILE, format!("{faction}.ranks[{index}].id"))
            }
            ContentProblem::RankValueOutOfRange {
                faction,
                index,
                key,
                ..
            } => {
                let key = match key {
                    RankKey::Standing => "requires.standing",
                    RankKey::Tolerance => "tolerance",
                };
                (FACTIONS_FILE, format!("{faction}.ranks[{index}].{key}"))
            }
            ContentProblem::UnknownRank {
                character, index, ..
            } => (
                CHARACTERS_FILE,
                format!("{character}.memberships[{index}].rank"),
            ),
            ContentProblem::SameFactionOutOfRange(_) => {
                (BALANCE_FILE, "disposition.same_faction".to_owned())
            }
            ContentProblem::NegativeHysteresis(_) => {
                (BALANCE_FILE, "disposition.hysteresis".to_owned())
            }
            ContentProblem::UnknownProfile {
                user: ProfileUser::Default,
                ..
            } => (BALANCE_FILE, "inertia.default_profile".to_owned()),
            ContentProblem::UnknownProfile {
                user: ProfileUser::Character(character),
                ..
            } => (CHARACTERS_FILE, format!("{character}.inertia")),
            ContentProblem::NegativeTargetScaling { action, curve, .. } => {
                (ACTIONS_FILE, format!("{action}.by_target.{}", curve.key()))
            }
            ContentProblem::NegativeInertia {
                profile, toward, ..
            } => (
                BALANCE_FILE,
                format!(
                    "inertia.profiles.{profile}.{}.{}",
                    toward.axis().key(),
                    toward.key()
                ),
            ),
            ContentProblem::RuleTable {
                owner,
                kind,
                problem,
            } => {
                let (file, table) = match owner {
                    TableOwner::World => (BALANCE_FILE, format!("membership.{kind}")),
                    TableOwner::Faction(faction) => (FACTIONS_FILE, format!("{faction}.{kind}")),
                };
                let at = match problem {
                    TableProblem::RungBelowOne {
                        rule, condition, ..
                    }
                    | TableProblem::RankInWorldTable {
                        rule, condition, ..
                    }
                    | TableProblem::UnknownRank {
                        rule, condition, ..
                    } => format!("rules[{rule}].when.{condition}"),
                    TableProblem::ValueOutOfRange { rule, key, .. } if *key == STANDING_CHANGE => {
                        format!("rules[{rule}].{key}")
                    }
                    TableProblem::ValueOutOfRange { rule, key, .. } => {
                        format!("rules[{rule}].when.{key}")
                    }
                    TableProblem::MightNotDecide { rules: 0 } => "rules".to_owned(),
                    TableProblem::MightNotDecide { rules } => {
                        format!("rules[{}].when", rules - 1)
                    }
                };
                (file, format!("{table}.{at}"))
            }
            ContentProblem::NoRippleStrength => {
                (BALANCE_FILE, "knowledge.ripple.strength".to_owned())
            }
            ContentProblem::RippleStrengthOutOfRange { index, .. }
            | ContentProblem::RippleStrengthRises { index, .. } => {
                (BALANCE_FILE, format!("knowledge.ripple.strength[{index}]"))
            }
            ContentProblem::NoHopTicks => (BALANCE_FILE, "knowledge.ripple.hop_ticks".to_owned()),
            ContentProblem::UnknownContact {
                character, index, ..
            }
            | ContentProblem::SelfContact { character, index }
            | ContentProblem::DuplicateContact {
                character, index, ..
            }
            | ContentProblem::MutualContact {
                character, index, ..
            } => (CHARACTERS_FILE, format!("{character}.contacts[{index}]")),
            ContentProblem::SecretMembersNeedKnowledge(faction) => {
                (FACTIONS_FILE, format!("{faction}.secret_members"))
            }
            ContentProblem::SecretMembershipNotAllowed {
                character, index, ..
            } => (
                CHARACTERS_FILE,
                format!("{character}.memberships[{index}].secret"),
            ),
            ContentProblem::OutcomeRelation { outcome, problem } => {
                (OUTCOMES_FILE, format!("{outcome}.{}", shift_key(problem)))
            }
        };
        Diagnostic {
            file: file.to_owned(),
            key: Some(key),
            message: problem.to_string(),
        }
    }));
    // The checks across quests wait until every file reads cleanly, so they never report
    // something missing only because it couldn't be read.
    if read_cleanly {
        diagnostics.extend(
            quests
                .problems(&content)
                .iter()
                .map(quests::problem_diagnostic),
        );
    }
    Read {
        content,
        quests,
        diagnostics,
    }
}

/// Content's warnings: things allowed but probably not meant, each with the file and key
/// it's about. Loading succeeds regardless; show them to the designer.
pub fn warnings(content: &Content) -> Vec<Diagnostic> {
    content
        .warnings()
        .iter()
        .map(|warning| match warning {
            ContentWarning::OutsideMemberTolerance {
                character, index, ..
            } => Diagnostic {
                file: CHARACTERS_FILE.to_owned(),
                key: Some(format!("{character}.memberships[{index}]")),
                message: warning.to_string(),
            },
            ContentWarning::BelowRankStanding {
                character, index, ..
            } => Diagnostic {
                file: CHARACTERS_FILE.to_owned(),
                key: Some(format!("{character}.memberships[{index}].rank")),
                message: warning.to_string(),
            },
            ContentWarning::RankToleranceLooser { faction, index, .. } => Diagnostic {
                file: FACTIONS_FILE.to_owned(),
                key: Some(format!("{faction}.ranks[{index}].tolerance")),
                message: warning.to_string(),
            },
            ContentWarning::NoOneWithinTolerance { faction, .. } => Diagnostic {
                file: FACTIONS_FILE.to_owned(),
                key: Some(format!("{faction}.tolerance")),
                message: warning.to_string(),
            },
            ContentWarning::ContactsUnused { character } => Diagnostic {
                file: CHARACTERS_FILE.to_owned(),
                key: Some(format!("{character}.contacts")),
                message: warning.to_string(),
            },
        })
        .collect()
}

/// Reads one file, if it's there, keeping its report; `None` for a missing file.
fn read_file<T>(
    file: &'static str,
    text: Option<&str>,
    reports: &mut Vec<Report>,
    reader: impl FnOnce(&str, &mut Report) -> T,
) -> Option<T> {
    let mut report = Report::new(file);
    let read = reader(text?, &mut report);
    reports.push(report);
    Some(read)
}

/// `balance.toml`: world-wide rules and defaults; anything left out keeps its default.
fn read_balance(text: &str, report: &mut Report) -> Balance {
    let mut balance = Balance::default();
    let Some(table) = report.parse(text) else {
        return balance;
    };
    let mut file = Section::new(&table, String::new());
    if let Some(mut alignment) = file.optional_table("alignment", "[alignment]", report) {
        if let Some(threshold) = alignment.optional_fixed("label_threshold", report) {
            let (low, high) = (Fixed::from_hundredths(1), factional_reputation::AXIS_LIMIT);
            if (low..=high).contains(&threshold) {
                balance.label_threshold = threshold;
            } else {
                report.error(
                    &alignment.path_to("label_threshold"),
                    format!("{threshold} must be between {low} and {high}"),
                );
            }
        }
        if let Some(key) = alignment.optional_text("metric", report) {
            match Metric::from_key(&key) {
                Some(metric) => balance.metric = metric,
                None => {
                    let keys = Metric::ALL.map(Metric::key);
                    let message = match suggest(&key, keys) {
                        Some(close) => format!("unknown metric '{key}' (did you mean '{close}'?)"),
                        None => format!(
                            "unknown metric '{key}': use {}, {} or {}",
                            keys[0], keys[1], keys[2]
                        ),
                    };
                    report.error(&alignment.path_to("metric"), message);
                }
            }
        }
        if let Some(weights) = read_weights(&mut alignment, "default_weights", report) {
            balance.default_weights = weights;
        }
        alignment.finish(report);
    }
    if let Some(mut disposition) = file.optional_table("disposition", "[disposition]", report) {
        if let Some(affinity) = disposition.optional_curve("affinity", report) {
            balance.affinity = affinity;
        }
        if let Some(bands) = read_bands(&mut disposition, report) {
            balance.bands = bands;
        }
        let example = "{ affinity = 1.0, standing = 1.0, kinship = 0.5 }";
        if let Some(mut weights) = disposition.optional_table("weights", example, report) {
            let current = balance.disposition_weights;
            let mut weight = |kind: ComponentKind| {
                weights
                    .optional_fixed(kind.key(), report)
                    .unwrap_or(current.get(kind))
            };
            balance.disposition_weights = DispositionWeights {
                affinity: weight(ComponentKind::Affinity),
                standing: weight(ComponentKind::Standing),
                kinship: weight(ComponentKind::Kinship),
                faction_opinion: weight(ComponentKind::FactionOpinion),
                modifiers: weight(ComponentKind::Modifiers),
            };
            weights.finish(report);
        }
        if let Some(same_faction) = disposition.optional_fixed("same_faction", report) {
            balance.same_faction = same_faction;
        }
        if let Some(hysteresis) = disposition.optional_fixed("hysteresis", report) {
            balance.hysteresis = hysteresis;
        }
        disposition.finish(report);
    }
    if let Some(mut standing) = file.optional_table("standing", "[standing]", report) {
        if let Some(spillover) = standing.optional_curve("spillover", report) {
            balance.spillover = spillover;
        }
        standing.finish(report);
    }
    if let Some(mut inertia) = file.optional_table("inertia", "[inertia]", report) {
        balance.inertia = read_inertia(&mut inertia, report);
        inertia.finish(report);
    }
    if let Some(mut membership) = file.optional_table("membership", "[membership]", report) {
        if let Some(policy) = read_drift(&mut membership, "default_drift", report) {
            balance.default_drift = policy;
        }
        if let Some(rule) = read_conflict_rule(&mut membership, report) {
            balance.conflict = rule;
        }
        balance.rule_tables = read_rule_tables(&mut membership, report);
        membership.finish(report);
    }
    if let Some(mut relations) = file.optional_table("relations", "[relations]", report) {
        if let Some(threshold) = relations.optional_fixed("conflict_threshold", report) {
            balance.conflict_threshold = threshold;
        }
        if let Some(bands) = read_bands(&mut relations, report) {
            balance.relation_bands = bands;
        }
        relations.finish(report);
    }
    if let Some(mut knowledge) = file.optional_table("knowledge", "[knowledge]", report) {
        if let Some(key) = knowledge.optional_text("model", report) {
            match KnowledgeModel::from_key(&key) {
                Some(model) => balance.knowledge = model,
                None => {
                    let keys = KnowledgeModel::ALL.map(KnowledgeModel::key);
                    let message = match suggest(&key, keys) {
                        Some(close) => {
                            format!("unknown knowledge model '{key}' (did you mean '{close}'?)")
                        }
                        None => format!(
                            "unknown knowledge model '{key}': use {}, {} or {}",
                            keys[0], keys[1], keys[2]
                        ),
                    };
                    report.error(&knowledge.path_to("model"), message);
                }
            }
        }
        if let Some(mut ripple) = knowledge.optional_table("ripple", "[knowledge.ripple]", report) {
            if let Some(items) = ripple.optional_list("strength", "[0.5, 0.25, 0.1]", report) {
                let strength: Vec<Option<Fixed>> = items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| {
                        ripple.to_fixed(&format!("strength[{index}]"), item, report)
                    })
                    .collect();
                if let Some(strength) = strength.into_iter().collect() {
                    balance.ripple.strength = strength;
                }
            }
            if ripple.has_any(&["hop_ticks"])
                && let Some(ticks) = ripple.whole("hop_ticks", report)
            {
                balance.ripple.hop_ticks = ticks;
            }
            ripple.finish(report);
        }
        knowledge.finish(report);
    }
    file.finish(report);
    balance
}

/// `[inertia]`: `default_profile`, and `[inertia.profiles.<id>]`, each with up to four
/// curves, such as `good.toward_good` (DESIGN.md §5.3). `steady` is always there. Whether
/// the profiles named exist, and the curves stay at or above 0, are the world's checks
/// (P-32).
fn read_inertia(section: &mut Section<'_>, report: &mut Report) -> Inertia {
    let mut inertia = Inertia::default();
    if let Some(text) = section.optional_text("default_profile", report) {
        match ProfileId::new(&text) {
            Ok(id) => inertia.default_profile = id,
            Err(invalid) => report.error(&section.path_to("default_profile"), invalid.to_string()),
        }
    }
    let example = format!("[{}]", section.path_to("profiles.steady"));
    let Some(mut profiles) = section.optional_table("profiles", &example, report) else {
        return inertia;
    };
    for key in profiles.keys() {
        let id = match ProfileId::new(&key) {
            Ok(id) => id,
            Err(invalid) => {
                profiles.mark(&key);
                report.error(profiles.path(), invalid.to_string());
                continue;
            }
        };
        let example = format!("[{}]", profiles.path_to(&key));
        let Some(mut fields) = profiles.table_any(&key, &example, report) else {
            continue;
        };
        let mut curves = BTreeMap::new();
        for axis in [Axis::Law, Axis::Good] {
            let towards: Vec<Toward> = Toward::ALL
                .into_iter()
                .filter(|toward| toward.axis() == axis)
                .collect();
            let example = format!("{{ {} = 1.0 }}", towards[0].key());
            let Some(mut directions) = fields.optional_table(axis.key(), &example, report) else {
                continue;
            };
            for toward in towards {
                if let Some(curve) = directions.optional_curve(toward.key(), report) {
                    curves.insert(toward, curve);
                }
            }
            directions.finish(report);
        }
        fields.finish(report);
        inertia.profiles.insert(id, InertiaProfile { curves });
    }
    profiles.finish(report);
    inertia
}

/// `disposition.bands`, lowest first. The checks across bands only run once every band has
/// been read, so each problem's path names the band it's about.
fn read_bands(section: &mut Section<'_>, report: &mut Report) -> Option<Bands> {
    const BAND: &str = "{ name = \"neutral\", up_to = 25.0 }";
    let example = format!("[{BAND}, {{ name = \"friendly\" }}]");
    let items = section.optional_list("bands", &example, report)?;
    let path = section.path_to("bands");
    let at = |index: usize| format!("{path}[{index}]");
    let mut bands = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let Value::Table(fields) = item else {
            report.error(&at(index), format!("expected a table, like {BAND}"));
            bands.push(None);
            continue;
        };
        let mut band = Section::new(fields, at(index));
        let errors = report.diagnostics.len();
        let name = band.text("name", report);
        let up_to = band.optional_fixed("up_to", report);
        // A wrong `up_to` reads as none at all, so it would look open-ended: leave it out.
        let read = report.diagnostics.len() == errors;
        band.finish(report);
        bands.push(name.filter(|_| read).map(|name| Band { name, up_to }));
    }
    let bands: Vec<Band> = bands.into_iter().collect::<Option<_>>()?;
    match Bands::new(bands) {
        Ok(bands) => Some(bands),
        Err(problems) => {
            for problem in problems {
                let key = match problem {
                    BandProblem::NoBands => path.clone(),
                    BandProblem::InvalidName { index, .. }
                    | BandProblem::DuplicateName { index, .. } => format!("{}.name", at(index)),
                    BandProblem::MissingUpTo { index } => at(index),
                    BandProblem::LastHasUpTo { index }
                    | BandProblem::NotIncreasing { index, .. } => {
                        format!("{}.up_to", at(index))
                    }
                };
                report.error(&key, problem.to_string());
            }
            None
        }
    }
}

/// A file of tables keyed by id, such as `characters.toml`: each entry's id is checked, and
/// its fields are read by `read`. An entry with problems is reported and left out.
fn read_tables<Id: Ord + Clone, T>(
    text: &str,
    report: &mut Report,
    kind: &str,
    new_id: impl Fn(&str) -> Result<Id, InvalidId>,
    read: impl Fn(Id, &toml::Table, &mut Report) -> Option<T>,
) -> BTreeMap<Id, T> {
    let mut entries = BTreeMap::new();
    let Some(table) = report.parse(text) else {
        return entries;
    };
    for (key, value) in &table {
        let id = match new_id(key) {
            Ok(id) => id,
            Err(invalid) => {
                report.error(key, invalid.to_string());
                continue;
            }
        };
        let Value::Table(fields) = value else {
            report.error(
                key,
                format!("expected a table of {kind} fields, like [{key}]"),
            );
            continue;
        };
        if let Some(entry) = read(id.clone(), fields, report) {
            entries.insert(id, entry);
        }
    }
    entries
}

/// One character in `characters.toml`.
fn read_character(id: CharacterId, fields: &toml::Table, report: &mut Report) -> Option<Character> {
    let mut section = Section::new(fields, id.to_string());
    let name = section.text("name", report);
    let alignment = read_alignment(&mut section, report);
    let weights = read_weights(&mut section, "weights", report);
    let inertia =
        section
            .optional_text("inertia", report)
            .and_then(|text| match ProfileId::new(&text) {
                Ok(id) => Some(id),
                Err(invalid) => {
                    report.error(&section.path_to("inertia"), invalid.to_string());
                    None
                }
            });
    let memberships = read_memberships(&mut section, report);
    let contacts = read_contacts(&mut section, report);
    let standing = section
        .optional_table("standing", STANDING_EXAMPLE, report)
        .map(|standing| read_named_standing(standing, report))
        .unwrap_or_default();
    section.finish(report);
    Some(Character {
        id,
        name: name?,
        alignment: alignment?,
        weights,
        inertia,
        memberships: memberships?,
        standing,
        contacts: contacts?,
    })
}

/// A character's `contacts`: the ids of the characters they pass news to; empty if left
/// out. `None` if any is wrong (that's reported).
fn read_contacts(section: &mut Section<'_>, report: &mut Report) -> Option<Vec<CharacterId>> {
    let Some(items) = section.optional_list("contacts", "[\"captain_hale\"]", report) else {
        return Some(Vec::new());
    };
    let path = section.path_to("contacts");
    let contacts: Vec<Option<CharacterId>> = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let at = format!("{path}[{index}]");
            let Value::String(text) = item else {
                report.error(&at, "expected a character's id in quotes");
                return None;
            };
            CharacterId::new(text)
                .map_err(|invalid| report.error(&at, invalid.to_string()))
                .ok()
        })
        .collect();
    contacts.into_iter().collect()
}

/// A rule's `standing_change` key, which a table problem may point at.
const STANDING_CHANGE: &str = "standing_change";

const STANDING_EXAMPLE: &str = "{ factions = { city_watch = 10.0 }, characters = { vex = -5.0 } }";

/// A `standing` block's `factions` and `characters`, each a table of ids and values, then
/// the rest of the block checked for unknown keys. Whether each party exists, and each value
/// is in range, are the world's checks (P-32).
fn read_named_standing(mut section: Section<'_>, report: &mut Report) -> StandingEffects {
    let effects = named_standing(&mut section, report);
    section.finish(report);
    effects
}

fn named_standing(section: &mut Section<'_>, report: &mut Report) -> StandingEffects {
    StandingEffects {
        factions: standing_table(section, "factions", FactionId::new, report),
        characters: standing_table(section, "characters", CharacterId::new, report),
    }
}

/// One table of `id = value` pairs, such as `factions = { city_watch = -20.0 }`.
fn standing_table<Id: Ord>(
    section: &mut Section<'_>,
    key: &'static str,
    new_id: impl Fn(&str) -> Result<Id, InvalidId>,
    report: &mut Report,
) -> BTreeMap<Id, Fixed> {
    let mut values = BTreeMap::new();
    let Some(mut table) = section.optional_table(key, "{ city_watch = 10.0 }", report) else {
        return values;
    };
    for name in &table.keys() {
        match new_id(name) {
            Ok(id) => {
                if let Some(value) = table.fixed_any(name, report) {
                    values.insert(id, value);
                }
            }
            Err(invalid) => {
                table.mark(name);
                report.error(table.path(), invalid.to_string());
            }
        }
    }
    table.finish(report);
    values
}

/// Where a relation shift's problem is, under its effects: `relations[0].between[1]`, or
/// `relations[0].by`, or the shift itself.
fn shift_key(problem: &ShiftProblem) -> String {
    let shift = format!("relations[{}]", problem.index());
    match problem {
        ShiftProblem::UnknownFaction { side, .. } => match side {
            RelationSide::Between(end) => format!("{shift}.between[{end}]"),
            RelationSide::From => format!("{shift}.from"),
            RelationSide::To => format!("{shift}.to"),
        },
        ShiftProblem::OutOfRange { .. } => format!("{shift}.by"),
        ShiftProblem::SelfRelation { .. } | ShiftProblem::Repeated { .. } => shift,
    }
}

/// Where a `standing` block's owner lives: its file and key.
fn standing_owner(owner: &StandingOwner) -> (&'static str, String) {
    match owner {
        StandingOwner::Character(id) => (CHARACTERS_FILE, id.to_string()),
        StandingOwner::Action(id) => (ACTIONS_FILE, id.to_string()),
        StandingOwner::Outcome(id) => (OUTCOMES_FILE, id.to_string()),
    }
}

/// A character's optional `memberships = [{ faction = "lantern_guild" }]`, as listed. Whether
/// each faction exists is the world's check (P-32), reported at its `faction` key.
fn read_memberships(
    section: &mut Section<'_>,
    report: &mut Report,
) -> Option<Vec<StartingMembership>> {
    const MEMBERSHIP: &str = "{ faction = \"lantern_guild\" }";
    let Some(items) = section.optional_list("memberships", &format!("[{MEMBERSHIP}]"), report)
    else {
        return Some(Vec::new());
    };
    let path = section.path_to("memberships");
    let mut factions = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let at = format!("{path}[{index}]");
        let Value::Table(fields) = item else {
            report.error(&at, format!("expected a table, like {MEMBERSHIP}"));
            factions.push(None);
            continue;
        };
        let mut membership = Section::new(fields, at);
        let faction = membership.text("faction", report).and_then(|text| {
            FactionId::new(&text)
                .map_err(|invalid| {
                    report.error(&membership.path_to("faction"), invalid.to_string())
                })
                .ok()
        });
        let rank = match membership.optional_text("rank", report) {
            None => Some(None),
            Some(text) => match RankId::new(&text) {
                Ok(rank) => Some(Some(rank)),
                Err(invalid) => {
                    report.error(&membership.path_to("rank"), invalid.to_string());
                    None
                }
            },
        };
        let secret = membership.optional_flag("secret", report).unwrap_or(false);
        membership.finish(report);
        factions.push(faction.zip(rank).map(|(faction, rank)| StartingMembership {
            faction,
            rank,
            secret,
        }));
    }
    factions.into_iter().collect()
}

/// `relations.toml`: a list of `[[relation]]` tables. Whether the factions they name exist,
/// and each direction is set once, are the world's checks (P-32).
fn read_relations(text: &str, report: &mut Report) -> Vec<Relation> {
    let Some(table) = report.parse(text) else {
        return Vec::new();
    };
    let mut file = Section::new(&table, String::new());
    let mut relations = Vec::new();
    match file.optional_value("relation") {
        None => {}
        Some(Value::Array(items)) => {
            for (index, item) in items.iter().enumerate() {
                let at = format!("relation[{index}]");
                let Value::Table(fields) = item else {
                    report.error(&at, "expected a table, like [[relation]]");
                    continue;
                };
                relations.extend(read_relation(Section::new(fields, at), report));
            }
        }
        Some(_) => report.error("relation", "expected a list of [[relation]] tables"),
    }
    file.finish(report);
    relations
}

/// One `[[relation]]`: `between = [a, b]`, or `from` and `to`, and a `value`.
fn read_relation(mut section: Section<'_>, report: &mut Report) -> Option<Relation> {
    let ends = read_ends(&mut section, report);
    let value = section.fixed("value", report);
    section.finish(report);
    Some(Relation {
        ends: ends?,
        value: value?,
    })
}

const SHIFT_EXAMPLE: &str = "{ between = [\"city_watch\", \"temple\"], by = -10.0 }";

/// An optional `relations = [...]` of shifts, as an outcome or a choice's effects write them;
/// none if left out. A shift that can't be read is reported and left out.
fn read_relation_shifts(section: &mut Section<'_>, report: &mut Report) -> Vec<RelationShift> {
    let path = section.path_to("relations");
    let Some(items) = section.optional_list("relations", &format!("[{SHIFT_EXAMPLE}]"), report)
    else {
        return Vec::new();
    };
    items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let at = format!("{path}[{index}]");
            let Value::Table(fields) = item else {
                report.error(&at, format!("expected a table, like {SHIFT_EXAMPLE}"));
                return None;
            };
            let mut shift = Section::new(fields, at);
            let ends = read_ends(&mut shift, report);
            let by = shift.fixed("by", report);
            shift.finish(report);
            Some(RelationShift {
                ends: ends?,
                by: by?,
            })
        })
        .collect()
}

/// The factions a relation or a shift names: `between = [a, b]`, or `from` and `to`.
fn read_ends(section: &mut Section<'_>, report: &mut Report) -> Option<RelationEnds> {
    const PAIR: &str = "[\"city_watch\", \"lantern_guild\"]";
    let between = section.optional_value("between").map(|value| {
        let path = section.path_to("between");
        let ids = match value {
            Value::Array(items) if items.len() == 2 => items
                .iter()
                .enumerate()
                .map(|(end, item)| match item {
                    Value::String(text) => FactionId::new(text)
                        .map_err(|invalid| {
                            report.error(&format!("{path}[{end}]"), invalid.to_string())
                        })
                        .ok(),
                    _ => {
                        report.error(&format!("{path}[{end}]"), "expected text in quotes");
                        None
                    }
                })
                .collect::<Option<Vec<FactionId>>>(),
            _ => {
                report.error(&path, format!("expected 2 factions, like {PAIR}"));
                None
            }
        };
        ids.map(|ids| (ids[0].clone(), ids[1].clone()))
    });
    let directed = section.has_any(&["from", "to"]);
    match (between, directed) {
        (Some(_), true) => {
            report.error(
                section.path(),
                "give either between = [a, b], or from and to, not both",
            );
            None
        }
        (Some(between), false) => between.map(|(a, b)| RelationEnds::Between(a, b)),
        (None, false) => {
            report.error(
                section.path(),
                "give either between = [a, b], or from and to",
            );
            None
        }
        (None, true) => {
            let mut end = |key| {
                let text = section.text(key, report)?;
                FactionId::new(&text)
                    .map_err(|invalid| report.error(&section.path_to(key), invalid.to_string()))
                    .ok()
            };
            let (from, to) = (end("from"), end("to"));
            Some(RelationEnds::Directed {
                from: from?,
                to: to?,
            })
        }
    }
}

/// One faction in `factions.toml`.
fn read_faction(id: FactionId, fields: &toml::Table, report: &mut Report) -> Option<Faction> {
    let mut section = Section::new(fields, id.to_string());
    let name = section.text("name", report);
    let alignment = read_alignment(&mut section, report);
    let weights = read_weights(&mut section, "weights", report);
    let tolerance = section.fixed("tolerance", report);
    let member = section.optional_fixed("member_tolerance", report);
    let leave_standing_change = section
        .optional_fixed("leave_standing_change", report)
        .unwrap_or_default();
    let ranks = read_ranks(&mut section, report);
    let rule_tables = read_rule_tables(&mut section, report);
    let drift = read_drift(&mut section, "drift", report);
    let expel_standing_change = section
        .optional_fixed("expel_standing_change", report)
        .unwrap_or(Faction::DEFAULT_EXPEL_STANDING_CHANGE);
    let tolerances = tolerance.and_then(|tolerance| match Tolerances::new(tolerance, member) {
        Ok(tolerances) => Some(tolerances),
        Err(problem) => {
            let key = match problem {
                ToleranceProblem::Negative(_) => "tolerance",
                ToleranceProblem::MemberBelowTolerance { .. } => "member_tolerance",
            };
            report.error(&section.path_to(key), problem.to_string());
            None
        }
    });
    let secret_members = section
        .optional_flag("secret_members", report)
        .unwrap_or(false);
    section.finish(report);
    Some(Faction {
        secret_members,
        id,
        name: name?,
        alignment: alignment?,
        weights,
        tolerances: tolerances?,
        leave_standing_change,
        ranks: ranks?,
        rule_tables,
        drift,
        expel_standing_change,
    })
}

/// A drift setting, `{ policy = "flag" }`, under `key`: a faction's `drift`, or
/// `membership.default_drift` (DESIGN.md §9.3). `None` if it's absent or wrong (that's
/// reported).
fn read_drift(
    section: &mut Section<'_>,
    key: &'static str,
    report: &mut Report,
) -> Option<DriftPolicy> {
    let mut drift = section.optional_table(key, "{ policy = \"flag\" }", report)?;
    let text = drift.text("policy", report);
    let policy = match text.as_deref() {
        None => None,
        Some("probation") => read_probation(&mut drift, report),
        Some(text) => {
            let policy = DriftPolicy::simple(text);
            if policy.is_none() {
                let keys = DriftPolicy::KEYS;
                let message = match suggest(text, keys) {
                    Some(close) => {
                        format!("unknown drift policy '{text}' (did you mean '{close}'?)")
                    }
                    None => format!(
                        "unknown drift policy '{text}': use {}, {}, {}, {} or {}",
                        keys[0], keys[1], keys[2], keys[3], keys[4]
                    ),
                };
                report.error(&drift.path_to("policy"), message);
            }
            policy
        }
    };
    drift.finish(report);
    policy
}

/// `membership.conflict = { resolve = "ask" | "auto", auto_after_ticks = N }`: how a war
/// between two of someone's factions is settled (DESIGN.md §9.4). `auto_after_ticks` only
/// goes with `ask`. `None` if it's absent or wrong (that's reported).
fn read_conflict_rule(section: &mut Section<'_>, report: &mut Report) -> Option<ConflictRule> {
    let mut rule = section.optional_table("conflict", "{ resolve = \"ask\" }", report)?;
    let resolve = rule.text("resolve", report);
    let after = if rule.has_any(&["auto_after_ticks"]) {
        Some(rule.whole("auto_after_ticks", report))
    } else {
        None
    };
    let read = match (resolve.as_deref(), after) {
        (None, _) => None,
        (Some("ask"), None) => Some(ConflictRule::Ask {
            auto_after_ticks: None,
        }),
        (Some("ask"), Some(ticks)) => ticks.map(|ticks| ConflictRule::Ask {
            auto_after_ticks: Some(ticks),
        }),
        (Some("auto"), None) => Some(ConflictRule::Auto),
        (Some("auto"), Some(_)) => {
            report.error(
                &rule.path_to("auto_after_ticks"),
                "auto settles straight away; auto_after_ticks goes with ask",
            );
            None
        }
        (Some(other), _) => {
            let message = match suggest(other, ["ask", "auto"]) {
                Some(close) => {
                    format!("unknown way to resolve '{other}' (did you mean '{close}'?)")
                }
                None => format!("unknown way to resolve '{other}': use ask or auto"),
            };
            report.error(&rule.path_to("resolve"), message);
            None
        }
    };
    rule.finish(report);
    read
}

/// A probation's `grace_ticks`, at least 1, and `then`, `demote` or `expel`.
fn read_probation(drift: &mut Section<'_>, report: &mut Report) -> Option<DriftPolicy> {
    let grace_ticks = drift.whole("grace_ticks", report).and_then(|ticks| {
        if ticks == 0 {
            report.error(&drift.path_to("grace_ticks"), "0 must be at least 1");
            None
        } else {
            Some(ticks)
        }
    });
    let then = drift.text("then", report).and_then(|text| {
        let found = Consequence::ALL.into_iter().find(|then| then.key() == text);
        if found.is_none() {
            let keys = Consequence::ALL.map(Consequence::key);
            let message = match suggest(&text, keys) {
                Some(close) => format!("unknown consequence '{text}' (did you mean '{close}'?)"),
                None => format!(
                    "unknown consequence '{text}': use {} or {}",
                    keys[0], keys[1]
                ),
            };
            report.error(&drift.path_to("then"), message);
        }
        found
    });
    Some(DriftPolicy::Probation {
        grace_ticks: grace_ticks?,
        then: then?,
    })
}

/// The `defectors` and `deserters` tables under `section`, each `{ rules = [...] }`
/// (DESIGN.md §9.2). A table with any rule that can't be read is left out, so the checks
/// across rules wait until every rule reads cleanly. Whether rank ids name ranks on the
/// right ladder, and the numbers are in range, are the world's checks (P-32).
fn read_rule_tables(
    section: &mut Section<'_>,
    report: &mut Report,
) -> BTreeMap<TableKind, Vec<Rule>> {
    let mut tables = BTreeMap::new();
    for kind in TableKind::ALL {
        let example = format!("[{}]", section.path_to(kind.key()));
        let Some(mut table) = section.optional_table(kind.key(), &example, report) else {
            continue;
        };
        if let Some(rules) = read_rules(&mut table, kind, report) {
            tables.insert(kind, rules);
        }
        table.finish(report);
    }
    tables
}

/// A table's `rules`, top to bottom; `None` if any can't be read.
fn read_rules(table: &mut Section<'_>, kind: TableKind, report: &mut Report) -> Option<Vec<Rule>> {
    let example = format!(
        "{{ when = {{ closer_to_target = true }}, then = \"{}\" }}",
        kind.allow_key()
    );
    let items = table.list("rules", &format!("[{example}]"), report)?;
    let path = table.path_to("rules");
    let mut rules = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let at = format!("{path}[{index}]");
        let Value::Table(fields) = item else {
            report.error(&at, format!("expected a table, like {example}"));
            rules.push(None);
            continue;
        };
        rules.push(read_rule(Section::new(fields, at), kind, report));
    }
    rules.into_iter().collect()
}

/// One rule: `when` (left out, it always holds), `then`, and `standing_change` for letting
/// someone through or `reason` for refusing them.
fn read_rule(mut rule: Section<'_>, kind: TableKind, report: &mut Report) -> Option<Rule> {
    const REFUSE: &str = "refuse";
    let errors = report.diagnostics.len();
    let when = rule
        .optional_table("when", "{ closer_to_target = true }", report)
        .map(|mut when| {
            let conditions = read_conditions(&mut when, report);
            when.finish(report);
            conditions
        })
        .unwrap_or_default();
    let then = rule.text("then", report);
    let standing_change = rule.optional_fixed(STANDING_CHANGE, report);
    let reason = rule.optional_text("reason", report);
    let outcomes = kind.outcomes();
    let verdict = match then.as_deref() {
        None => None,
        Some(REFUSE) if outcomes.contains(&REFUSE) => {
            if standing_change.is_some() {
                report.error(
                    &rule.path_to(STANDING_CHANGE),
                    "a refusal changes nothing, so it has no standing_change",
                );
            }
            if reason.is_none() {
                report.error(rule.path(), "missing 'reason'");
            }
            Some(Verdict::Refuse { reason })
        }
        Some(outcome) if outcomes.contains(&outcome) => {
            if reason.is_some() {
                report.error(&rule.path_to("reason"), "only a refusal has a reason");
            }
            let standing_change = standing_change.unwrap_or_default();
            Some(match outcome {
                "demote" => Verdict::Demote { standing_change },
                "expel" => Verdict::Expel { standing_change },
                _ => Verdict::Allow { standing_change },
            })
        }
        Some(unknown) => {
            let message = match suggest(unknown, outcomes.iter().copied()) {
                Some(close) => {
                    format!("unknown outcome '{unknown}' for {kind} (did you mean '{close}'?)")
                }
                None => {
                    let quoted: Vec<String> = outcomes
                        .iter()
                        .map(|outcome| format!("'{outcome}'"))
                        .collect();
                    let (last, rest) = quoted.split_last().expect("every table has outcomes");
                    format!(
                        "unknown outcome '{unknown}' for {kind}: use {} or {last}",
                        rest.join(", ")
                    )
                }
            };
            report.error(&rule.path_to("then"), message);
            None
        }
    };
    rule.finish(report);
    let read = report.diagnostics.len() == errors;
    verdict.filter(|_| read).map(|then| Rule { when, then })
}

/// A rule's conditions, in the order `Condition::KEYS` lists them.
fn read_conditions(when: &mut Section<'_>, report: &mut Report) -> Vec<Condition> {
    let mut conditions = Vec::new();
    for key in Condition::KEYS {
        let at = when.path_to(key);
        let condition = match key {
            "rank_at_least" | "rank_below" => when
                .optional_value(key)
                .and_then(|value| read_rank_ref(value, &at, report))
                .map(|rank| match key {
                    "rank_at_least" => Condition::RankAtLeast(rank),
                    _ => Condition::RankBelow(rank),
                }),
            "closer_to_target" | "outside_member_tolerance" => when
                .optional_value(key)
                .and_then(|value| read_flag(value, &at, report))
                .map(|flag| match key {
                    "closer_to_target" => Condition::CloserToTarget(flag),
                    _ => Condition::OutsideMemberTolerance(flag),
                }),
            _ => when.optional_fixed(key, report).map(|value| match key {
                "standing_with_current_at_least" => Condition::StandingWithCurrentAtLeast(value),
                "standing_with_current_below" => Condition::StandingWithCurrentBelow(value),
                "standing_with_target_at_least" => Condition::StandingWithTargetAtLeast(value),
                _ => Condition::StandingWithTargetBelow(value),
            }),
        };
        conditions.extend(condition);
    }
    conditions
}

/// A rung number, or a rank id in quotes.
fn read_rank_ref(value: &Value, at: &str, report: &mut Report) -> Option<RankRef> {
    match value {
        Value::Integer(rung) => Some(RankRef::Rung(*rung)),
        Value::String(text) => RankId::new(text)
            .map(RankRef::Id)
            .map_err(|invalid| report.error(at, invalid.to_string()))
            .ok(),
        _ => {
            report.error(at, "expected a rung number, like 3, or a rank id in quotes");
            None
        }
    }
}

fn read_flag(value: &Value, at: &str, report: &mut Report) -> Option<bool> {
    match value {
        Value::Boolean(flag) => Some(*flag),
        _ => {
            report.error(at, "expected true or false");
            None
        }
    }
}

/// A faction's `[[<faction>.ranks]]` ladder, lowest first; empty if it's left out (which the
/// world reports). `None` if any rung can't be read, so the checks across rungs, which name
/// rungs by their place, wait until every rung reads cleanly.
fn read_ranks(section: &mut Section<'_>, report: &mut Report) -> Option<Vec<Rank>> {
    let path = section.path_to("ranks");
    let items = match section.optional_value("ranks") {
        None => return Some(Vec::new()),
        Some(Value::Array(items)) => items,
        Some(_) => {
            report.error(&path, format!("expected a list of [[{path}]] tables"));
            return None;
        }
    };
    let mut ranks = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let at = format!("{path}[{index}]");
        let Value::Table(fields) = item else {
            report.error(&at, format!("expected a table, like [[{path}]]"));
            ranks.push(None);
            continue;
        };
        let mut rung = Section::new(fields, at);
        let errors = report.diagnostics.len();
        let id = rung.text("id", report).and_then(|text| {
            RankId::new(&text)
                .map_err(|invalid| report.error(&rung.path_to("id"), invalid.to_string()))
                .ok()
        });
        let requires_standing = rung
            .optional_table("requires", "{ standing = 30.0 }", report)
            .and_then(|mut requires| {
                let standing = requires.optional_fixed("standing", report);
                requires.finish(report);
                standing
            });
        let tolerance = rung.optional_fixed("tolerance", report);
        rung.finish(report);
        let read = report.diagnostics.len() == errors;
        ranks.push(id.filter(|_| read).map(|id| Rank {
            id,
            requires_standing,
            tolerance,
        }));
    }
    ranks.into_iter().collect()
}

/// A required `alignment = { law = …, good = … }`, with both axes in range.
fn read_alignment(section: &mut Section<'_>, report: &mut Report) -> Option<Alignment> {
    let mut axes = section.table("alignment", "{ law = 0.0, good = 0.0 }", report)?;
    let law = axes.fixed("law", report);
    let good = axes.fixed("good", report);
    let alignment = match Alignment::new(law?, good?) {
        Ok(alignment) => Some(alignment),
        Err(out_of_range) => {
            for problem in out_of_range {
                report.error(&axes.path_to(problem.axis.key()), problem.to_string());
            }
            None
        }
    };
    axes.finish(report);
    alignment
}

/// Optional weights under `key`, such as `weights = { law = 1.0, good = 0.25 }`: both axes,
/// each 0.00–1.00, at least one above 0. `None` when they're left out, or wrong; anything
/// wrong is reported, which fails the whole load.
fn read_weights(
    section: &mut Section<'_>,
    key: &'static str,
    report: &mut Report,
) -> Option<Weights> {
    let mut axes = section.optional_table(key, "{ law = 1.0, good = 1.0 }", report)?;
    let law = axes.fixed("law", report);
    let good = axes.fixed("good", report);
    let weights = match Weights::new(law?, good?) {
        Ok(weights) => Some(weights),
        Err(problems) => {
            for problem in problems {
                let path = match problem {
                    WeightProblem::OutOfRange { axis, .. } => axes.path_to(axis.key()),
                    WeightProblem::AllZero => axes.path().to_owned(),
                };
                report.error(&path, problem.to_string());
            }
            None
        }
    };
    axes.finish(report);
    weights
}

/// One action. Its `alignment` and each axis in it may be left out: an act needn't touch
/// both axes, or alignment at all. Any problem is reported, and fails the whole load.
fn read_action(id: ActionId, fields: &toml::Table, report: &mut Report) -> Action {
    let mut section = Section::new(fields, id.to_string());
    let alignment = read_delta(&mut section, report);
    let mut standing = ActionStanding::default();
    let example = "{ target = -20.0, target_factions = -10.0, factions = { city_watch = 5.0 } }";
    if let Some(mut block) = section.optional_table("standing", example, report) {
        standing.target = block.optional_fixed("target", report);
        standing.target_factions = block.optional_fixed("target_factions", report);
        standing.named = named_standing(&mut block, report);
        block.finish(report);
    }
    let mut by_target = BTreeMap::new();
    let example = "{ good = [[-100.0, 0.2], [0.0, 1.0], [100.0, 1.5]] }";
    if let Some(mut curves) = section.optional_table("by_target", example, report) {
        for curve in TargetCurve::ALL {
            if let Some(shape) = curves.optional_curve(curve.key(), report) {
                by_target.insert(curve, shape);
            }
        }
        curves.finish(report);
    }
    section.finish(report);
    Action {
        id,
        alignment,
        standing,
        by_target,
    }
}

/// An optional `alignment = { law = …, good = … }` change; an axis left out isn't moved.
fn read_delta(section: &mut Section<'_>, report: &mut Report) -> AlignmentDelta {
    let mut alignment = AlignmentDelta::default();
    if let Some(mut axes) = section.optional_table("alignment", "{ law = 0.0, good = 0.0 }", report)
    {
        alignment.law = axes.optional_fixed("law", report).unwrap_or_default();
        alignment.good = axes.optional_fixed("good", report).unwrap_or_default();
        axes.finish(report);
    }
    alignment
}

/// One outcome in `outcomes.toml`: an optional `alignment` change and `standing` effects,
/// applied together (P-26).
fn read_outcome(id: OutcomeId, fields: &toml::Table, report: &mut Report) -> Outcome {
    let mut section = Section::new(fields, id.to_string());
    let alignment = read_delta(&mut section, report);
    let standing = section
        .optional_table("standing", STANDING_EXAMPLE, report)
        .map(|standing| read_named_standing(standing, report))
        .unwrap_or_default();
    let relations = read_relation_shifts(&mut section, report);
    section.finish(report);
    Outcome {
        id,
        effects: Effects {
            alignment,
            standing,
            relations,
        },
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.key {
            Some(key) => write!(f, "{}: {key}: {}", self.file, self.message),
            None => write!(f, "{}: {}", self.file, self.message),
        }
    }
}

impl fmt::Display for ContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let lines: Vec<String> = self.diagnostics.iter().map(Diagnostic::to_string).collect();
        f.write_str(&lines.join("\n"))
    }
}

impl std::error::Error for ContentError {}

/// Reads a curve written the way it would appear in a content file: a number, or a list of
/// `[x, y]` points such as `[[0, 50], [60, 0], [200, -50]]`.
pub fn parse_curve(text: &str) -> Result<Curve, String> {
    #[derive(Deserialize)]
    struct Wrapper {
        curve: Curve,
    }
    toml::from_str::<Wrapper>(&format!("curve = {text}"))
        .map(|wrapper| wrapper.curve)
        .map_err(|error| error.message().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use factional_core::Fixed;
    use factional_reputation::{
        ActionId, CharacterId, Condition, ConflictRule, Consequence, DriftPolicy, FactionId,
        Inertia, InertiaProfile, Metric, ProfileId, RankRef, Rule, TableKind, TargetCurve, Toward,
        Verdict, Weights,
    };

    const fn h(hundredths: i64) -> Fixed {
        Fixed::from_hundredths(hundredths)
    }

    fn characters(text: &str) -> Result<Content, ContentError> {
        parse_content(Sources {
            characters: Some(text),
            ..Sources::default()
        })
    }

    fn actions(text: &str) -> Result<Content, ContentError> {
        parse_content(Sources {
            actions: Some(text),
            ..Sources::default()
        })
    }

    fn factions(text: &str) -> Result<Content, ContentError> {
        parse_content(Sources {
            factions: Some(text),
            ..Sources::default()
        })
    }

    fn balance(text: &str) -> Result<Content, ContentError> {
        parse_content(Sources {
            balance: Some(text),
            ..Sources::default()
        })
    }

    /// The problems reported, one string per diagnostic.
    fn problems(result: Result<Content, ContentError>) -> Vec<String> {
        result
            .expect_err("the content has problems")
            .diagnostics
            .iter()
            .map(Diagnostic::to_string)
            .collect()
    }

    const VEX: &str = r#"
        [vex]
        name = "Vex"
        alignment = { law = -55.0, good = -20.0 }
    "#;

    // Reading

    #[test]
    fn reads_characters() {
        let content = characters(VEX).expect("valid content");
        let vex = &content.characters[&CharacterId::new("vex").expect("valid id")];
        assert_eq!(vex.name, "Vex");
        assert_eq!(
            (vex.alignment.law(), vex.alignment.good()),
            (h(-55_00), h(-20_00))
        );
        assert_eq!(content.characters.len(), 1);
    }

    #[test]
    fn missing_files_mean_defaults_and_no_characters_or_actions() {
        let content = parse_content(Sources::default()).expect("valid content");
        assert_eq!(content.balance.label_threshold, h(33_00));
        assert!(content.characters.is_empty());
        assert!(content.factions.is_empty());
        assert!(content.actions.is_empty());
    }

    #[test]
    fn reads_the_label_threshold() {
        let content = balance("[alignment]\nlabel_threshold = 40").expect("valid content");
        assert_eq!(content.balance.label_threshold, h(40_00));
    }

    #[test]
    fn reads_the_metric_and_default_weights() {
        let content = balance(
            "[alignment]\nmetric = \"manhattan\"\ndefault_weights = { law = 0.5, good = 1.0 }",
        )
        .expect("valid content");
        assert_eq!(content.balance.metric, Metric::Manhattan);
        assert_eq!(
            content.balance.default_weights,
            Weights::new(h(50), h(1_00)).expect("valid")
        );
        for metric in ["euclidean", "chebyshev"] {
            let content =
                balance(&format!("[alignment]\nmetric = \"{metric}\"")).expect("valid content");
            assert_eq!(content.balance.metric.key(), metric);
        }
    }

    #[test]
    fn reads_the_component_weights_and_same_faction() {
        let content = balance(
            "[disposition]\nweights = { kinship = 0.25, modifiers = 0.0 }\nsame_faction = 30.0",
        )
        .expect("valid content");
        let weights = content.balance.disposition_weights;
        assert_eq!((weights.kinship, weights.modifiers), (h(25), h(0)));
        assert_eq!(
            (weights.affinity, weights.standing, weights.faction_opinion),
            (h(1_00), h(1_00), h(50)),
            "weights left out keep their defaults"
        );
        assert_eq!(content.balance.same_faction, h(30_00));
        assert_eq!(
            problems(balance(
                "[disposition]\nweights = { kinship = -0.5, kinsip = 1 }\nsame_faction = 120.0"
            )),
            [
                "balance.toml: disposition.weights: unknown key 'kinsip' (did you mean 'kinship'?)",
                "balance.toml: disposition.weights.kinship: -0.50 must be at least 0.00",
                "balance.toml: disposition.same_faction: 120.00 is outside -100.00..100.00",
            ]
        );
    }

    #[test]
    fn reads_the_affinity_curve_and_bands() {
        let content = balance(
            r#"
            [disposition]
            affinity = [[0.0, 100.0], [100.0, -100.0]]
            bands = [
              { name = "hostile", up_to = -30.0 },
              { name = "wary", up_to = 10 },
              { name = "warm" },
            ]
            "#,
        )
        .expect("valid content");
        assert_eq!(content.balance.affinity.at(h(25_00)), h(50_00));
        let bands: Vec<(&str, Option<Fixed>)> = content
            .balance
            .bands
            .iter()
            .map(|band| (band.name.as_str(), band.up_to))
            .collect();
        assert_eq!(
            bands,
            [
                ("hostile", Some(h(-30_00))),
                ("wary", Some(h(10_00))),
                ("warm", None)
            ]
        );
    }

    #[test]
    fn an_empty_disposition_section_keeps_the_defaults() {
        let content = balance("[disposition]").expect("valid content");
        assert_eq!(content.balance, Balance::default());
    }

    #[test]
    fn reports_mistakes_in_the_bands_at_their_keys() {
        let text = r#"
            [disposition]
            bands = [
              { name = "unfriendly", up_to = -25.0 },
              { name = "Very Cold", up_to = -30.0 },
              { name = "unfriendly" },
              { name = "friendly", up_to = 90.0, colour = "green" },
            ]
        "#;
        assert_eq!(
            problems(balance(text)),
            [
                "balance.toml: disposition.bands[3]: unknown key 'colour'",
                "balance.toml: disposition.bands[1].name: 'Very Cold' isn't a valid id: use lowercase letters, digits and _, starting with a letter",
                "balance.toml: disposition.bands[1].up_to: -30.00 must be above the previous band's -25.00",
                "balance.toml: disposition.bands[2].name: another band is already called 'unfriendly'",
                "balance.toml: disposition.bands[2]: missing 'up_to': only the last band takes every score above the band before it",
                "balance.toml: disposition.bands[3].up_to: the last band can't have 'up_to': it takes every score above the band before it",
            ]
        );
    }

    #[test]
    fn reports_a_band_list_that_is_empty_or_not_a_list() {
        assert_eq!(
            problems(balance("[disposition]\nbands = []")),
            ["balance.toml: disposition.bands: there must be at least one band"]
        );
        assert_eq!(
            problems(balance("[disposition]\nbands = \"three\"")),
            [
                "balance.toml: disposition.bands: expected a list, like [{ name = \"neutral\", up_to = 25.0 }, { name = \"friendly\" }]"
            ]
        );
        // An entry that can't be read stops the checks across bands, so their paths stay true.
        assert_eq!(
            problems(balance(
                "[disposition]\nbands = [{ up_to = 3.0 }, { name = \"x\" }]"
            )),
            ["balance.toml: disposition.bands[0]: missing 'name'"]
        );
        assert_eq!(
            problems(balance(
                "[disposition]\nbands = [{ name = \"x\", up_to = \"low\" }, { name = \"y\" }]"
            )),
            ["balance.toml: disposition.bands[0].up_to: expected a number, like 25.0"]
        );
        assert_eq!(
            problems(balance(
                "[disposition]\nbands = [7, { name = \"x\", up_to = 1.0 }]"
            )),
            [
                "balance.toml: disposition.bands[0]: expected a table, like { name = \"neutral\", up_to = 25.0 }"
            ]
        );
    }

    #[test]
    fn reports_an_affinity_curve_that_is_invalid_or_out_of_range() {
        assert_eq!(
            problems(balance(
                "[disposition]\naffinity = [[0.0, 150.0], [60.0, 0.0]]"
            )),
            ["balance.toml: disposition.affinity: curve value 150.00 is outside -100.00 to 100.00"]
        );
        assert_eq!(
            problems(balance(
                "[disposition]\naffinity = [[60.0, 50.0], [0.0, 0.0]]"
            )),
            [
                "balance.toml: disposition.affinity: curve points must have increasing x: 60.00 then 0.00"
            ]
        );
        assert_eq!(
            problems(balance("[disposition]\naffinity = 20.0\nhysteria = 1")),
            ["balance.toml: disposition: unknown key 'hysteria'"]
        );
    }

    #[test]
    fn reads_the_knowledge_model() {
        let content = balance("[knowledge]\nmodel = \"witnessed\"").expect("valid content");
        assert_eq!(
            content.balance.knowledge,
            factional_reputation::KnowledgeModel::Witnessed
        );
        let content = balance("[knowledge]\nmodel = \"omniscient\"").expect("valid content");
        assert_eq!(
            content.balance.knowledge,
            factional_reputation::KnowledgeModel::Omniscient
        );
        let content = balance("[knowledge]").expect("valid content");
        assert_eq!(
            content.balance.knowledge,
            factional_reputation::KnowledgeModel::Omniscient,
            "omniscient when left out"
        );
    }

    #[test]
    fn reports_an_unknown_knowledge_model() {
        assert_eq!(
            problems(balance("[knowledge]\nmodel = \"witnesed\"")),
            [
                "balance.toml: knowledge.model: unknown knowledge model 'witnesed' (did you mean 'witnessed'?)"
            ]
        );
        assert_eq!(
            problems(balance("[knowledge]\nmodel = \"rumour\"")),
            [
                "balance.toml: knowledge.model: unknown knowledge model 'rumour': use omniscient, witnessed or ripple"
            ]
        );
        assert_eq!(
            problems(balance("[knowledge]\nmodel = 1")),
            ["balance.toml: knowledge.model: expected text in quotes"]
        );
        assert_eq!(
            problems(balance("[knowledge]\nmodle = \"witnessed\"")),
            ["balance.toml: knowledge: unknown key 'modle' (did you mean 'model'?)"]
        );
        assert_eq!(
            problems(balance("knowledge = 1")),
            ["balance.toml: knowledge: expected a table, like [knowledge]"]
        );
    }

    #[test]
    fn reads_how_news_ripples() {
        let content = balance(
            "[knowledge]\nmodel = \"ripple\"\n\n[knowledge.ripple]\nstrength = [0.6, 0.3]\nhop_ticks = 10",
        )
        .expect("valid content");
        assert_eq!(
            content.balance.knowledge,
            factional_reputation::KnowledgeModel::Ripple
        );
        assert_eq!(
            content.balance.ripple,
            factional_reputation::Ripple {
                strength: vec![h(60), h(30)],
                hop_ticks: 10,
            }
        );
        let defaults = balance("[knowledge]\nmodel = \"ripple\"").expect("valid content");
        assert_eq!(
            defaults.balance.ripple,
            factional_reputation::Ripple {
                strength: vec![h(50), h(25), h(10)],
                hop_ticks: 1,
            },
            "0.50, 0.25 and 0.10, a tick a hop"
        );
    }

    #[test]
    fn reports_ripple_mistakes_at_their_keys() {
        assert_eq!(
            problems(balance(
                "[knowledge.ripple]\nstrength = 0.5\nhop_ticks = 1.5\nhops = 3"
            )),
            [
                "balance.toml: knowledge.ripple.strength: expected a list, like [0.5, 0.25, 0.1]",
                "balance.toml: knowledge.ripple.hop_ticks: expected a whole number, like 100",
                "balance.toml: knowledge.ripple: unknown key 'hops'",
            ]
        );
        assert_eq!(
            problems(balance("[knowledge.ripple]\nstrength = [0.5, \"half\"]")),
            ["balance.toml: knowledge.ripple.strength[1]: expected a number, like 25.0"]
        );
        assert_eq!(
            problems(balance(
                "[knowledge.ripple]\nstrength = [0.5, 0.0, 0.6]\nhop_ticks = 0"
            )),
            [
                "balance.toml: knowledge.ripple.strength[1]: 0.00 must be between 0.01 and 1.00",
                "balance.toml: knowledge.ripple.strength[2]: 0.60 is stronger than the hop before it, 0.00: news only weakens as it travels",
                "balance.toml: knowledge.ripple.hop_ticks: a hop takes at least 1 tick",
            ]
        );
        assert_eq!(
            problems(balance("[knowledge.ripple]\nstrength = []")),
            [
                "balance.toml: knowledge.ripple.strength: list at least one hop's strength; for news that doesn't travel, use the witnessed model"
            ]
        );
    }

    #[test]
    fn reads_a_characters_contacts() {
        let content = characters(&format!(
            "{VEX}contacts = [\"ava\"]\n\n[ava]\nname = \"Ava\"\nalignment = {{ law = 20.0, good = 10.0 }}\n"
        ))
        .expect("valid content");
        let id = |text| CharacterId::new(text).expect("valid id");
        assert_eq!(content.characters[&id("vex")].contacts, [id("ava")]);
        assert!(content.characters[&id("ava")].contacts.is_empty());
    }

    #[test]
    fn reports_contact_mistakes_at_their_keys() {
        assert_eq!(
            problems(characters(&format!("{VEX}contacts = \"ava\"\n"))),
            ["characters.toml: vex.contacts: expected a list, like [\"captain_hale\"]"]
        );
        assert_eq!(
            problems(characters(&format!("{VEX}contacts = [\"Ava\", 3]\n"))),
            [
                "characters.toml: vex.contacts[0]: 'Ava' isn't a valid id: use lowercase letters, digits and _, starting with a letter",
                "characters.toml: vex.contacts[1]: expected a character's id in quotes",
            ]
        );
        let ava = "[ava]\nname = \"Ava\"\nalignment = { law = 20.0, good = 10.0 }\ncontacts = [\"vex\"]\n";
        assert_eq!(
            problems(characters(&format!(
                "{VEX}contacts = [\"ava\", \"vex\", \"avx\", \"ava\"]\n\n{ava}"
            ))),
            [
                "characters.toml: vex.contacts[0]: ava already lists vex: a contact works both ways, so list it on one side only",
                "characters.toml: vex.contacts[1]: a character can't be their own contact",
                "characters.toml: vex.contacts[2]: unknown character 'avx' (did you mean 'ava'?)",
                "characters.toml: vex.contacts[3]: 'ava' is listed twice",
            ]
        );
    }

    #[test]
    fn warns_about_contacts_outside_the_ripple_model() {
        let text = format!(
            "{VEX}contacts = [\"ava\"]\n\n[ava]\nname = \"Ava\"\nalignment = {{ law = 20.0, good = 10.0 }}\n"
        );
        let warned: Vec<String> = warnings(&characters(&text).expect("valid content"))
            .iter()
            .map(Diagnostic::to_string)
            .collect();
        assert_eq!(
            warned,
            [
                "characters.toml: vex.contacts: contacts only carry news when knowledge.model is ripple"
            ]
        );
        let rippling = parse_content(Sources {
            balance: Some("[knowledge]\nmodel = \"ripple\""),
            characters: Some(&text),
            ..Sources::default()
        })
        .expect("valid content");
        assert!(warnings(&rippling).is_empty());
    }

    #[test]
    fn reports_an_unknown_metric() {
        assert_eq!(
            problems(balance("[alignment]\nmetric = \"euclidian\"")),
            [
                "balance.toml: alignment.metric: unknown metric 'euclidian' (did you mean 'euclidean'?)"
            ]
        );
        assert_eq!(
            problems(balance("[alignment]\nmetric = \"cosine\"")),
            [
                "balance.toml: alignment.metric: unknown metric 'cosine': use euclidean, manhattan or chebyshev"
            ]
        );
        assert_eq!(
            problems(balance("[alignment]\nmetric = 2")),
            ["balance.toml: alignment.metric: expected text in quotes"]
        );
        assert_eq!(
            problems(balance(
                "[alignment]\ndefault_weights = { law = 0.0, good = 0.0 }"
            )),
            ["balance.toml: alignment.default_weights: at least one weight must be above 0.00"]
        );
    }

    #[test]
    fn an_empty_alignment_section_keeps_the_default_threshold() {
        let content = balance("[alignment]").expect("valid content");
        assert_eq!(content.balance, Balance::default());
    }

    // Mistakes in characters.toml

    #[test]
    fn reports_an_axis_out_of_range_at_its_key() {
        let text = VEX.replace("law = -55.0", "law = 120");
        assert_eq!(
            problems(characters(&text)),
            ["characters.toml: vex.alignment.law: 120.00 is outside -100.00..100.00"]
        );
    }

    #[test]
    fn reports_an_unknown_key_with_a_suggestion() {
        let text = VEX.replace("alignment =", "alignmnet =");
        assert_eq!(
            problems(characters(&text)),
            [
                "characters.toml: vex: missing 'alignment'",
                "characters.toml: vex: unknown key 'alignmnet' (did you mean 'alignment'?)",
            ]
        );
    }

    #[test]
    fn reports_an_unknown_key_without_a_suggestion_when_nothing_is_close() {
        let text = format!("{VEX}colour = \"red\"");
        assert_eq!(
            problems(characters(&text)),
            ["characters.toml: vex: unknown key 'colour'"]
        );
    }

    #[test]
    fn reports_missing_and_mistyped_fields() {
        let text = r#"
            [ava]
            alignment = { law = 20.0, good = 10.0 }

            [hale]
            name = 7
            alignment = "lawful"

            [mira]
            name = "Mira"
            alignment = { law = 35.0 }
        "#;
        assert_eq!(
            problems(characters(text)),
            [
                "characters.toml: ava: missing 'name'",
                "characters.toml: hale.name: expected text in quotes",
                "characters.toml: hale.alignment: expected a table, like { law = 0.0, good = 0.0 }",
                "characters.toml: mira.alignment: missing 'good'",
            ]
        );
    }

    #[test]
    fn reports_bad_numbers_in_a_designers_words() {
        let text = VEX.replace("good = -20.0", "good = -20.125");
        assert_eq!(
            problems(characters(&text)),
            ["characters.toml: vex.alignment.good: -20.125 has more than 2 decimal places"]
        );
        for wrong in ["\"-20\"", "true", "[1.0]"] {
            let text = VEX.replace("-20.0", wrong);
            assert_eq!(
                problems(characters(&text)),
                ["characters.toml: vex.alignment.good: expected a number, like 25.0"],
                "{wrong}"
            );
        }
    }

    #[test]
    fn reports_an_invalid_character_id() {
        let text = VEX.replace("[vex]", "[\"Vex Thief\"]");
        assert_eq!(
            problems(characters(&text)),
            [
                "characters.toml: Vex Thief: 'Vex Thief' isn't a valid id: use lowercase letters, digits and _, starting with a letter"
            ]
        );
    }

    #[test]
    fn reports_a_character_that_is_not_a_table() {
        assert_eq!(
            problems(characters("vex = 3")),
            ["characters.toml: vex: expected a table of character fields, like [vex]"]
        );
    }

    #[test]
    fn reports_a_toml_syntax_error_with_its_line() {
        let found = problems(characters("[vex]\nname = \"Vex\nalignment = 3"));
        assert_eq!(found.len(), 1);
        assert!(
            found[0].starts_with("characters.toml: line 2: "),
            "{found:?}"
        );
    }

    // factions.toml

    const WATCH: &str = r#"
        [city_watch]
        name = "The City Watch"
        alignment = { law = 70.0, good = 20.0 }
        weights = { law = 1.0, good = 0.25 }
        tolerance = 40.0
        member_tolerance = 50.0

        [[city_watch.ranks]]
        id = "recruit"

        [[city_watch.ranks]]
        id = "sergeant"
        requires = { standing = 30.0 }

        [[city_watch.ranks]]
        id = "captain"
        requires = { standing = 70.0 }
        tolerance = 25.0
    "#;

    const GUILD: &str = r#"
        [lantern_guild]
        name = "The Lantern Guild"
        alignment = { law = -60.0, good = -10.0 }
        weights = { law = 1.0, good = 0.5 }
        tolerance = 45.0
        member_tolerance = 60.0

        [[lantern_guild.ranks]]
        id = "cutpurse"

        [[lantern_guild.ranks]]
        id = "fence"
        requires = { standing = 25.0 }

        [[lantern_guild.ranks]]
        id = "shadow"
        requires = { standing = 60.0 }
    "#;

    #[test]
    fn reads_exposed_tables() {
        let content = balance(
            r#"
            [membership.exposed]
            rules = [
              { when = { standing_with_current_at_least = 60.0 }, then = "keep", standing_change = -30.0 },
              { when = { rank_at_least = 2 }, then = "demote" },
              { then = "expel", standing_change = -40.0 },
            ]
            "#,
        )
        .expect("valid content");
        assert_eq!(
            content.balance.rule_tables[&TableKind::Exposed],
            [
                Rule {
                    when: vec![Condition::StandingWithCurrentAtLeast(h(60_00))],
                    then: Verdict::Allow {
                        standing_change: h(-30_00)
                    },
                },
                Rule {
                    when: vec![Condition::RankAtLeast(RankRef::Rung(2))],
                    then: Verdict::Demote {
                        standing_change: Fixed::ZERO
                    },
                },
                Rule {
                    when: Vec::new(),
                    then: Verdict::Expel {
                        standing_change: h(-40_00)
                    },
                },
            ]
        );
    }

    #[test]
    fn reports_exposed_table_mistakes_at_their_keys() {
        assert_eq!(
            problems(balance(
                r#"
                [membership.exposed]
                rules = [
                  { then = "refuse", reason = "No." },
                  { then = "expell" },
                  { then = "keep", reason = "Fine." },
                  { then = "expel" },
                ]
                "#
            )),
            [
                "balance.toml: membership.exposed.rules[0].then: unknown outcome 'refuse' for exposed: use 'keep', 'demote' or 'expel'",
                "balance.toml: membership.exposed.rules[1].then: unknown outcome 'expell' for exposed (did you mean 'expel'?)",
                "balance.toml: membership.exposed.rules[2].reason: only a refusal has a reason",
            ]
        );
        assert_eq!(
            problems(balance(
                "[membership.defectors]\nrules = [{ then = \"expel\" }]"
            )),
            [
                "balance.toml: membership.defectors.rules[0].then: unknown outcome 'expel' for defectors: use 'accept' or 'refuse'"
            ]
        );
    }

    /// The Lantern Guild with secret members, in a world where news ripples.
    const SECRET_GUILD: &str = r#"
        [lantern_guild]
        name = "The Lantern Guild"
        alignment = { law = -60.0, good = -10.0 }
        tolerance = 45.0
        secret_members = true

        [[lantern_guild.ranks]]
        id = "cutpurse"
    "#;

    fn secret_world(
        model: &str,
        characters: &str,
        relations: Option<&str>,
    ) -> Result<Content, ContentError> {
        parse_content(Sources {
            balance: Some(&format!("[knowledge]\nmodel = \"{model}\"")),
            factions: Some(&format!("{SECRET_GUILD}\n{WATCH}")),
            characters: Some(characters),
            relations,
            ..Sources::default()
        })
    }

    #[test]
    fn reads_secret_members_and_secret_memberships() {
        let content = secret_world(
            "ripple",
            &format!(
                "{VEX}memberships = [{{ faction = \"lantern_guild\", secret = true }}, {{ faction = \"city_watch\" }}]\n"
            ),
            None,
        )
        .expect("valid content");
        let faction = |id| &content.factions[&FactionId::new(id).expect("valid id")];
        assert!(faction("lantern_guild").secret_members);
        assert!(!faction("city_watch").secret_members, "false when left out");
        let vex = &content.characters[&CharacterId::new("vex").expect("valid id")];
        let secret: Vec<bool> = vex.memberships.iter().map(|m| m.secret).collect();
        assert_eq!(secret, [true, false]);
    }

    #[test]
    fn reports_secret_membership_mistakes_at_their_keys() {
        let guild = SECRET_GUILD.replace("secret_members = true", "secret_members = \"yes\"");
        assert_eq!(
            problems(factions(&guild)),
            ["factions.toml: lantern_guild.secret_members: expected true or false"]
        );
        assert_eq!(
            problems(secret_world(
                "ripple",
                &format!("{VEX}memberships = [{{ faction = \"lantern_guild\", secret = 1 }}]\n"),
                None
            )),
            ["characters.toml: vex.memberships[0].secret: expected true or false"]
        );
        assert_eq!(
            problems(secret_world("omniscient", VEX, None)),
            [
                "factions.toml: lantern_guild.secret_members: secret members need knowledge.model witnessed or ripple: under omniscient, everyone knows everything"
            ]
        );
        assert_eq!(
            problems(secret_world(
                "ripple",
                &format!("{VEX}memberships = [{{ faction = \"city_watch\", secret = true }}]\n"),
                None
            )),
            [
                "characters.toml: vex.memberships[0].secret: city_watch doesn't allow secret members: set secret_members = true on it"
            ]
        );
    }

    #[test]
    fn a_character_may_start_secretly_in_an_enemy_of_their_faction() {
        let war = "[[relation]]\nbetween = [\"city_watch\", \"lantern_guild\"]\nvalue = -80.0\n";
        let both = |secret: bool| {
            secret_world(
                "ripple",
                &format!(
                    "{VEX}memberships = [{{ faction = \"city_watch\" }}, {{ faction = \"lantern_guild\", secret = {secret} }}]\n"
                ),
                Some(war),
            )
        };
        assert!(both(true).is_ok(), "the Watch doesn't know");
        assert_eq!(
            problems(both(false)),
            [
                "characters.toml: vex.memberships[1].faction: vex can't start in both city_watch and lantern_guild: they're in conflict (-80.00)"
            ]
        );
    }

    #[test]
    fn reads_factions_with_their_weights() {
        let text = format!(
            "{WATCH}\n[free_company]\nname = \"The Free Company\"\nalignment = {{ law = -10.0, good = 0.0 }}\ntolerance = 60.0\n[[free_company.ranks]]\nid = \"sellsword\""
        );
        let content = factions(&text).expect("valid content");
        let watch = &content.factions[&FactionId::new("city_watch").expect("valid id")];
        assert_eq!(watch.name, "The City Watch");
        assert_eq!(
            (watch.alignment.law(), watch.alignment.good()),
            (h(70_00), h(20_00))
        );
        assert_eq!(
            watch.weights,
            Some(Weights::new(h(1_00), h(25)).expect("valid"))
        );
        assert_eq!(
            (watch.tolerances.tolerance(), watch.tolerances.member()),
            (h(40_00), h(50_00))
        );
        let company = &content.factions[&FactionId::new("free_company").expect("valid id")];
        assert_eq!(company.weights, None, "no weights of its own");
        assert_eq!(
            company.tolerances.member(),
            h(60_00),
            "member tolerance defaults to the tolerance"
        );
    }

    #[test]
    fn reports_mistakes_in_a_faction() {
        let text = r#"
            guild = 3

            ["Lantern Guild"]
            name = "The Lantern Guild"

            [temple]
            alignment = { law = 30.0, good = 180.0 }
            tolerance = 35.0

            [watch]
            name = "The Watch"
            alignment = { law = 70.0, good = 20.0 }
            weights = { law = 1.5, good = 0.25 }
            motto = "Order"
        "#;
        assert_eq!(
            problems(factions(text)),
            [
                "factions.toml: Lantern Guild: 'Lantern Guild' isn't a valid id: use lowercase letters, digits and _, starting with a letter",
                "factions.toml: guild: expected a table of faction fields, like [guild]",
                "factions.toml: temple: missing 'name'",
                "factions.toml: temple.alignment.good: 180.00 is outside -100.00..100.00",
                "factions.toml: watch.weights.law: 1.50 must be between 0.00 and 1.00",
                "factions.toml: watch: missing 'tolerance'",
                "factions.toml: watch: unknown key 'motto'",
            ]
        );
    }

    #[test]
    fn reports_weights_that_count_for_nothing_or_miss_an_axis() {
        assert_eq!(
            problems(factions(
                &WATCH.replace("law = 1.0, good = 0.25", "law = 0, good = 0.0")
            )),
            ["factions.toml: city_watch.weights: at least one weight must be above 0.00"]
        );
        assert_eq!(
            problems(factions(
                &WATCH.replace("law = 1.0, good = 0.25", "law = 1.0")
            )),
            ["factions.toml: city_watch.weights: missing 'good'"]
        );
        assert_eq!(
            problems(factions(&WATCH.replace("weights = {", "weights = 1 #"))),
            ["factions.toml: city_watch.weights: expected a table, like { law = 1.0, good = 1.0 }"]
        );
    }

    #[test]
    fn reports_tolerances_that_are_negative_or_inverted() {
        assert_eq!(
            problems(factions(
                &WATCH.replace("tolerance = 40.0", "tolerance = -5.0")
            )),
            ["factions.toml: city_watch.tolerance: -5.00 must be at least 0.00"]
        );
        assert_eq!(
            problems(factions(
                &WATCH.replace("member_tolerance = 50.0", "member_tolerance = 30.0")
            )),
            [
                "factions.toml: city_watch.member_tolerance: 30.00 must be at least the faction's tolerance, 40.00"
            ]
        );
        assert_eq!(
            problems(factions(
                &WATCH.replace("tolerance = 40.0", "tolerance = \"far\"")
            )),
            ["factions.toml: city_watch.tolerance: expected a number, like 25.0"]
        );
    }

    fn world(factions: &str, characters: &str) -> Result<Content, ContentError> {
        parse_content(Sources {
            factions: Some(factions),
            characters: Some(characters),
            ..Sources::default()
        })
    }

    #[test]
    fn reads_rank_ladders() {
        let content = factions(WATCH).expect("valid content");
        let watch = &content.factions[&FactionId::new("city_watch").expect("valid id")];
        let ladder: Vec<(&str, Option<Fixed>, Option<Fixed>)> = watch
            .ranks
            .iter()
            .map(|rank| (rank.id.as_str(), rank.requires_standing, rank.tolerance))
            .collect();
        assert_eq!(
            ladder,
            [
                ("recruit", None, None),
                ("sergeant", Some(h(30_00)), None),
                ("captain", Some(h(70_00)), Some(h(25_00))),
            ]
        );
    }

    #[test]
    fn reports_mistakes_in_a_ladder_at_their_keys() {
        let text = r#"
            [temple]
            name = "Temple of the Dawn"
            alignment = { law = 30.0, good = 80.0 }
            tolerance = 35.0

            [[temple.ranks]]
            id = "acolyte"
            requires = { standing = 120.0, rank = 2 }

            [[temple.ranks]]
            id = "High Priest"
            tolerance = -5.0

            [[temple.ranks]]
            requires = 3

            [[temple.ranks]]
            id = "acolyte"

            [ashen_circle]
            name = "The Ashen Circle"
            alignment = { law = 20.0, good = -80.0 }
            tolerance = 30.0
            ranks = "initiate"

            [free_company]
            name = "The Free Company"
            alignment = { law = -10.0, good = 0.0 }
            tolerance = 60.0
        "#;
        assert_eq!(
            problems(factions(text)),
            [
                "factions.toml: ashen_circle.ranks: expected a list of [[ashen_circle.ranks]] tables",
                "factions.toml: temple.ranks[0].requires: unknown key 'rank'",
                "factions.toml: temple.ranks[1].id: 'High Priest' isn't a valid id: use lowercase letters, digits and _, starting with a letter",
                "factions.toml: temple.ranks[2]: missing 'id'",
                "factions.toml: temple.ranks[2].requires: expected a table, like { standing = 30.0 }",
                // A faction whose ladder can't be read is left out, so the checks across
                // rungs wait until it can: only the Free Company gets that far.
                "factions.toml: free_company.ranks: a faction needs at least one rank",
            ]
        );
        let text = r#"
            [temple]
            name = "Temple of the Dawn"
            alignment = { law = 30.0, good = 80.0 }
            tolerance = 35.0

            [[temple.ranks]]
            id = "acolyte"
            requires = { standing = 120.0 }

            [[temple.ranks]]
            id = "acolyte"
            tolerance = -5.0
        "#;
        assert_eq!(
            problems(factions(text)),
            [
                "factions.toml: temple.ranks[0].requires.standing: 120.00 is outside -100.00..100.00",
                "factions.toml: temple.ranks[1].tolerance: -5.00 must be at least 0.00",
                "factions.toml: temple.ranks[1].id: another rank is already called 'acolyte'",
            ]
        );
    }

    #[test]
    fn a_starting_rank_must_be_on_the_ladder() {
        let text =
            format!("{VEX}memberships = [{{ faction = \"lantern_guild\", rank = \"fense\" }}]");
        assert_eq!(
            problems(world(GUILD, &text)),
            [
                "characters.toml: vex.memberships[0].rank: unknown rank 'fense' for lantern_guild (did you mean 'fence'?)"
            ]
        );
        let text =
            format!("{VEX}memberships = [{{ faction = \"lantern_guild\", rank = \"Fence\" }}]");
        assert_eq!(
            problems(world(GUILD, &text)),
            [
                "characters.toml: vex.memberships[0].rank: 'Fence' isn't a valid id: use lowercase letters, digits and _, starting with a letter"
            ]
        );
        let text =
            format!("{VEX}memberships = [{{ faction = \"lantern_guild\", rank = \"fence\" }}]");
        let content = world(GUILD, &text).expect("valid content");
        let vex = &content.characters[&CharacterId::new("vex").expect("valid id")];
        assert_eq!(
            vex.memberships[0].rank.as_ref().map(RankId::as_str),
            Some("fence")
        );
    }

    #[test]
    fn warns_of_ranks_that_are_probably_mistakes() {
        let text = format!(
            "{VEX}memberships = [{{ faction = \"lantern_guild\", rank = \"shadow\" }}]\nstanding = {{ factions = {{ lantern_guild = 30.0 }} }}"
        );
        let guild = GUILD.replace(
            "id = \"shadow\"\n        requires = { standing = 60.0 }",
            "id = \"shadow\"\n        requires = { standing = 60.0 }\n        tolerance = 70.0",
        );
        let content = world(&guild, &text).expect("warnings don't stop loading");
        let found: Vec<String> = warnings(&content)
            .iter()
            .map(Diagnostic::to_string)
            .collect();
        assert_eq!(
            found,
            [
                "characters.toml: vex.memberships[0].rank: vex starts as a shadow with standing 30.00, below the 60.00 it requires",
                "factions.toml: lantern_guild.ranks[2].tolerance: shadow's tolerance 70.00 is looser than the faction's member tolerance, 60.00, so it changes nothing",
            ]
        );
    }

    #[test]
    fn reads_starting_memberships() {
        let text = format!(
            "{VEX}memberships = [{{ faction = \"lantern_guild\" }}, {{ faction = \"city_watch\" }}]"
        );
        let content = world(&format!("{WATCH}{GUILD}"), &text).expect("valid content");
        let vex = &content.characters[&CharacterId::new("vex").expect("valid id")];
        let factions: Vec<&str> = vex.factions().map(FactionId::as_str).collect();
        assert_eq!(factions, ["lantern_guild", "city_watch"], "as listed");
        let plain = characters(VEX).expect("valid content");
        assert!(
            plain.characters[&CharacterId::new("vex").expect("valid id")]
                .memberships
                .is_empty()
        );
    }

    #[test]
    fn a_membership_must_name_a_faction_that_exists_once() {
        let text = format!(
            "{VEX}memberships = [{{ faction = \"lantern_gild\" }}, {{ faction = \"lantern_guild\" }}, {{ faction = \"lantern_guild\" }}]"
        );
        assert_eq!(
            problems(world(GUILD, &text)),
            [
                "characters.toml: vex.memberships[0].faction: unknown faction 'lantern_gild' (did you mean 'lantern_guild'?)",
                "characters.toml: vex.memberships[2].faction: vex already belongs to lantern_guild",
            ]
        );
    }

    #[test]
    fn reports_mistakes_in_a_membership() {
        let text = format!(
            "{VEX}memberships = [{{ faction = \"Lantern Guild\" }}, {{}}, {{ faction = \"lantern_guild\", colour = \"red\" }}, 3]"
        );
        assert_eq!(
            problems(world(GUILD, &text)),
            [
                "characters.toml: vex.memberships[0].faction: 'Lantern Guild' isn't a valid id: use lowercase letters, digits and _, starting with a letter",
                "characters.toml: vex.memberships[1]: missing 'faction'",
                "characters.toml: vex.memberships[2]: unknown key 'colour'",
                "characters.toml: vex.memberships[3]: expected a table, like { faction = \"lantern_guild\" }",
            ]
        );
        assert_eq!(
            problems(world(
                GUILD,
                &format!("{VEX}memberships = \"lantern_guild\"")
            )),
            [
                "characters.toml: vex.memberships: expected a list, like [{ faction = \"lantern_guild\" }]"
            ]
        );
    }

    #[test]
    fn warns_of_a_starting_member_outside_member_tolerance() {
        let reformed = VEX.replace("law = -55.0, good = -20.0", "law = 35.0, good = 10.0");
        let text = format!("{reformed}memberships = [{{ faction = \"lantern_guild\" }}]");
        let content = world(GUILD, &text).expect("a warning doesn't stop loading");
        let found: Vec<String> = warnings(&content)
            .iter()
            .map(Diagnostic::to_string)
            .collect();
        assert_eq!(
            found,
            [
                "characters.toml: vex.memberships[0]: vex starts 95.52 from The Lantern Guild, outside its member tolerance of 60.00",
                // Vex is the only character, so no one starts within the guild's tolerance.
                "factions.toml: lantern_guild.tolerance: no one starts within The Lantern Guild's tolerance of 45.00: the nearest is vex, 95.52 away",
            ]
        );
        let faithful = world(
            GUILD,
            &format!("{VEX}memberships = [{{ faction = \"lantern_guild\" }}]"),
        )
        .expect("valid content");
        assert!(warnings(&faithful).is_empty());
    }

    #[test]
    fn reads_a_characters_own_weights() {
        let text = format!("{VEX}weights = {{ law = 0.5, good = 1.0 }}");
        let content = characters(&text).expect("valid content");
        let vex = &content.characters[&CharacterId::new("vex").expect("valid id")];
        assert_eq!(
            vex.weights,
            Some(Weights::new(h(50), h(1_00)).expect("valid"))
        );
        let plain = characters(VEX).expect("valid content");
        assert_eq!(
            plain.characters[&CharacterId::new("vex").expect("valid id")].weights,
            None
        );
        assert_eq!(
            problems(characters(&format!(
                "{VEX}weights = {{ law = -0.5, good = 1.0 }}"
            ))),
            ["characters.toml: vex.weights.law: -0.50 must be between 0.00 and 1.00"]
        );
    }

    #[test]
    fn a_faction_and_a_character_cannot_share_an_id() {
        let found = problems(parse_content(Sources {
            factions: Some(&WATCH.replace("city_watch", "vex")),
            characters: Some(VEX),
            ..Sources::default()
        }));
        assert_eq!(
            found,
            [
                "factions.toml: vex: 'vex' is also a character's id: factions and characters need different ids"
            ]
        );
    }

    // Drift (M8)

    fn with_drift(lines: &str) -> String {
        WATCH.replacen(
            "member_tolerance = 50.0",
            &format!("member_tolerance = 50.0\n        {lines}"),
            1,
        )
    }

    #[test]
    fn reads_drift_policies_and_the_cost_of_expulsion() {
        let content = parse_content(Sources {
            balance: Some("[membership]\ndefault_drift = { policy = \"ignore\" }\n"),
            factions: Some(&with_drift(
                "drift = { policy = \"expel\" }\n        expel_standing_change = -35.0",
            )),
            ..Sources::default()
        })
        .expect("valid content");
        let watch = &content.factions[&FactionId::new("city_watch").expect("valid id")];
        assert_eq!(
            (watch.drift, watch.expel_standing_change),
            (Some(DriftPolicy::Expel), h(-35_00))
        );
        assert_eq!(content.balance.default_drift, DriftPolicy::Ignore);
        let plain = factions(WATCH).expect("valid content");
        let watch = &plain.factions[&FactionId::new("city_watch").expect("valid id")];
        assert_eq!(
            (watch.drift, watch.expel_standing_change),
            (None, h(-20_00))
        );
        assert_eq!(plain.balance.default_drift, DriftPolicy::Flag);
    }

    #[test]
    fn reads_a_probation_with_its_grace_and_its_consequence() {
        let content = factions(&with_drift(
            "drift = { policy = \"probation\", grace_ticks = 100, then = \"demote\" }",
        ))
        .expect("valid content");
        let watch = &content.factions[&FactionId::new("city_watch").expect("valid id")];
        assert_eq!(
            watch.drift,
            Some(DriftPolicy::Probation {
                grace_ticks: 100,
                then: Consequence::Demote,
            })
        );
    }

    #[test]
    fn reports_probation_mistakes_at_their_keys() {
        let cases: [(&str, &[&str]); 6] = [
            (
                "drift = { policy = \"probation\" }",
                &[
                    "factions.toml: city_watch.drift: missing 'grace_ticks'",
                    "factions.toml: city_watch.drift: missing 'then'",
                ],
            ),
            (
                "drift = { policy = \"probation\", grace_ticks = 0, then = \"expel\" }",
                &["factions.toml: city_watch.drift.grace_ticks: 0 must be at least 1"],
            ),
            (
                "drift = { policy = \"probation\", grace_ticks = -5, then = \"expel\" }",
                &["factions.toml: city_watch.drift.grace_ticks: expected a whole number, like 100"],
            ),
            (
                "drift = { policy = \"probation\", grace_ticks = 2.5, then = \"expel\" }",
                &["factions.toml: city_watch.drift.grace_ticks: expected a whole number, like 100"],
            ),
            (
                "drift = { policy = \"probation\", grace_ticks = 10, then = \"expell\" }",
                &[
                    "factions.toml: city_watch.drift.then: unknown consequence 'expell' (did you mean 'expel'?)",
                ],
            ),
            (
                "drift = { policy = \"probation\", grace_ticks = 10, then = \"flag\" }",
                &[
                    "factions.toml: city_watch.drift.then: unknown consequence 'flag': use demote or expel",
                ],
            ),
        ];
        for (lines, expected) in cases {
            assert_eq!(problems(factions(&with_drift(lines))), expected, "{lines}");
        }
    }

    #[test]
    fn reports_drift_mistakes_at_their_keys() {
        let cases = [
            (
                "drift = { policy = \"flagg\" }",
                "factions.toml: city_watch.drift.policy: unknown drift policy 'flagg' (did you mean 'flag'?)",
            ),
            (
                "drift = { policy = \"punish\" }",
                "factions.toml: city_watch.drift.policy: unknown drift policy 'punish': use ignore, flag, demote, expel or probation",
            ),
            (
                "drift = { policy = \"flag\", grace_ticks = 5 }",
                "factions.toml: city_watch.drift: unknown key 'grace_ticks'",
            ),
            (
                "drift = \"flag\"",
                "factions.toml: city_watch.drift: expected a table, like { policy = \"flag\" }",
            ),
            (
                "drift = {}",
                "factions.toml: city_watch.drift: missing 'policy'",
            ),
            (
                "expel_standing_change = -120.0",
                "factions.toml: city_watch.expel_standing_change: -120.00 is outside -100.00..100.00",
            ),
        ];
        for (lines, expected) in cases {
            assert_eq!(
                problems(factions(&with_drift(lines))),
                [expected],
                "{lines}"
            );
        }
        assert_eq!(
            problems(balance(
                "[membership]\ndefault_drift = { policy = \"expell\" }\n"
            )),
            [
                "balance.toml: membership.default_drift.policy: unknown drift policy 'expell' (did you mean 'expel'?)"
            ]
        );
    }

    // Wars between your own factions (M9)

    fn conflict_rule(line: &str) -> Result<Content, ContentError> {
        balance(&format!("[membership]\nconflict = {line}\n"))
    }

    #[test]
    fn reads_how_wars_between_your_own_factions_are_settled() {
        let rule = |line: &str| conflict_rule(line).expect("valid content").balance.conflict;
        assert_eq!(
            rule("{ resolve = \"ask\" }"),
            ConflictRule::Ask {
                auto_after_ticks: None
            }
        );
        assert_eq!(
            rule("{ resolve = \"ask\", auto_after_ticks = 50 }"),
            ConflictRule::Ask {
                auto_after_ticks: Some(50)
            }
        );
        assert_eq!(rule("{ resolve = \"auto\" }"), ConflictRule::Auto);
        assert_eq!(
            balance("").expect("valid").balance.conflict,
            ConflictRule::default()
        );
    }

    #[test]
    fn reports_conflict_rule_mistakes_at_their_keys() {
        let cases = [
            (
                "{ resolve = \"auot\" }",
                "balance.toml: membership.conflict.resolve: unknown way to resolve 'auot' (did you mean 'auto'?)",
            ),
            (
                "{ resolve = \"never\" }",
                "balance.toml: membership.conflict.resolve: unknown way to resolve 'never': use ask or auto",
            ),
            (
                "{ resolve = \"ask\", auto_after_ticks = -1 }",
                "balance.toml: membership.conflict.auto_after_ticks: expected a whole number, like 100",
            ),
            (
                "{ resolve = \"auto\", auto_after_ticks = 5 }",
                "balance.toml: membership.conflict.auto_after_ticks: auto settles straight away; auto_after_ticks goes with ask",
            ),
            (
                "{ auto_after_ticks = 5 }",
                "balance.toml: membership.conflict: missing 'resolve'",
            ),
        ];
        for (line, expected) in cases {
            assert_eq!(problems(conflict_rule(line)), [expected], "{line}");
        }
    }

    // Spillover (M6)

    #[test]
    fn reads_and_checks_the_spillover_curve() {
        let content = balance("[standing]\nspillover = [[-100.0, -0.5], [100.0, 0.5]]\n")
            .expect("valid content");
        assert_eq!(
            content.balance.spillover,
            parse_curve("[[-100.0, -0.5], [100.0, 0.5]]").expect("valid curve")
        );
        assert_eq!(
            balance("").expect("valid").balance.spillover,
            factional_reputation::Balance::default_spillover()
        );
        assert_eq!(
            problems(balance(
                "[standing]\nspillover = [[-100.0, -0.5], [100.0, 1.5]]\nspilover = 1\n"
            )),
            [
                "balance.toml: standing: unknown key 'spilover' (did you mean 'spillover'?)",
                "balance.toml: standing.spillover: curve value 1.50 is outside -1.00 to 1.00",
            ]
        );
    }

    // Hysteresis (D3)

    #[test]
    fn reads_and_checks_the_hysteresis_margin() {
        let content = balance("[disposition]\nhysteresis = 5.0\n").expect("valid content");
        assert_eq!(content.balance.hysteresis, h(5_00));
        assert_eq!(balance("").expect("valid").balance.hysteresis, h(0));
        assert_eq!(
            problems(balance("[disposition]\nhysteresis = -1.0\n")),
            ["balance.toml: disposition.hysteresis: -1.00 must be at least 0.00"]
        );
        assert_eq!(
            problems(balance("[disposition]\nhysteresis = \"wide\"\n")),
            ["balance.toml: disposition.hysteresis: expected a number, like 25.0"]
        );
    }

    // Target-aware effects (A5)

    const MURDER: &str = r#"
        [murder]
        alignment = { law = -10.0, good = -15.0 }
        by_target.good = [[-100.0, 0.2], [0.0, 1.0], [100.0, 1.5]]
        by_target.relation = [[-100.0, 0.5], [-50.0, 0.8], [0.0, 1.0]]
    "#;

    #[test]
    fn reads_an_actions_by_target_curves() {
        let content = actions(MURDER).expect("valid content");
        let murder = &content.actions[&ActionId::new("murder").expect("valid id")];
        let curve = |text: &str| parse_curve(text).expect("valid curve");
        assert_eq!(
            murder.by_target,
            [
                (
                    TargetCurve::Good,
                    curve("[[-100.0, 0.2], [0.0, 1.0], [100.0, 1.5]]")
                ),
                (
                    TargetCurve::Relation,
                    curve("[[-100.0, 0.5], [-50.0, 0.8], [0.0, 1.0]]")
                ),
            ]
            .into()
        );
        let law = actions("[spit]\nalignment = { law = -1.0 }\nby_target.law = 0.5\n")
            .expect("valid content");
        assert_eq!(
            law.actions[&ActionId::new("spit").expect("valid id")].by_target,
            [(TargetCurve::Law, curve("0.5"))].into()
        );
        let plain = actions("[spit]\nalignment = { law = -1.0 }\n").expect("valid content");
        assert!(
            plain.actions[&ActionId::new("spit").expect("valid id")]
                .by_target
                .is_empty()
        );
    }

    #[test]
    fn reports_by_target_mistakes_at_their_keys() {
        let text = r#"
            [murder]
            alignment = { law = -10.0, good = -15.0 }
            by_target.relaton = 0.5
            by_target.good = [[0.0, 1.0]]

            [spit]
            by_target = 3

            [shove]
            by_target.law = [[-100.0, -0.1], [100.0, 1.0]]
        "#;
        assert_eq!(
            problems(actions(text)),
            [
                "actions.toml: murder.by_target.good: a curve is a single number or at least 2 points",
                "actions.toml: murder.by_target: unknown key 'relaton' (did you mean 'relation'?)",
                "actions.toml: spit.by_target: expected a table, like { good = [[-100.0, 0.2], [0.0, 1.0], [100.0, 1.5]] }",
                "actions.toml: shove.by_target.law: -0.10 is below 0.00: a target can soften or sharpen an act, never reverse it",
            ]
        );
    }

    // Inertia (A4)

    const HARDENING: &str = r#"
        [inertia]
        default_profile = "hardening"

        [inertia.profiles.hardening]
        good.toward_good = [[-100.0, 0.5], [0.0, 1.0], [100.0, 0.3]]
        good.toward_evil = [[-100.0, 0.3], [0.0, 1.0], [100.0, 0.5]]
        law.toward_lawful = 0.8
    "#;

    fn profile(id: &str) -> ProfileId {
        ProfileId::new(id).expect("valid id")
    }

    #[test]
    fn reads_inertia_profiles_and_keeps_steady() {
        let content = balance(HARDENING).expect("valid content");
        let inertia = &content.balance.inertia;
        assert_eq!(inertia.default_profile, profile("hardening"));
        let curve = |text: &str| parse_curve(text).expect("valid curve");
        assert_eq!(
            inertia.profiles,
            [
                (
                    profile("hardening"),
                    InertiaProfile {
                        curves: [
                            (Toward::Lawful, curve("0.8")),
                            (
                                Toward::Good,
                                curve("[[-100.0, 0.5], [0.0, 1.0], [100.0, 0.3]]")
                            ),
                            (
                                Toward::Evil,
                                curve("[[-100.0, 0.3], [0.0, 1.0], [100.0, 0.5]]")
                            ),
                        ]
                        .into()
                    }
                ),
                (Inertia::steady(), InertiaProfile::default()),
            ]
            .into()
        );
        assert_eq!(
            balance("").expect("valid").balance.inertia,
            Inertia::default()
        );
    }

    #[test]
    fn reads_a_characters_inertia_profile() {
        let content = parse_content(Sources {
            balance: Some(HARDENING),
            characters: Some(&format!("{VEX}inertia = \"steady\"\n")),
            ..Sources::default()
        })
        .expect("valid content");
        let vex = &content.characters[&CharacterId::new("vex").expect("valid id")];
        assert_eq!(vex.inertia, Some(Inertia::steady()));
        let plain = characters(VEX).expect("valid content");
        assert_eq!(
            plain.characters[&CharacterId::new("vex").expect("valid id")].inertia,
            None
        );
    }

    #[test]
    fn reports_inertia_mistakes_at_their_keys() {
        let text = r#"
            [inertia]
            default_profile = "Hardening"
            profile = 3

            [inertia.profiles]
            steady = 1.0

            [inertia.profiles.hardening]
            good.toward_lawful = 0.5
            good.toward_good = [[0.0, 1.0]]
            loyalty.toward_good = 1.0

            [inertia.profiles.Soft]
        "#;
        assert_eq!(
            problems(balance(text)),
            [
                "balance.toml: inertia.default_profile: 'Hardening' isn't a valid id: use lowercase letters, digits and _, starting with a letter",
                "balance.toml: inertia.profiles: 'Soft' isn't a valid id: use lowercase letters, digits and _, starting with a letter",
                "balance.toml: inertia.profiles.hardening.good.toward_good: a curve is a single number or at least 2 points",
                "balance.toml: inertia.profiles.hardening.good: unknown key 'toward_lawful'",
                "balance.toml: inertia.profiles.hardening: unknown key 'loyalty'",
                "balance.toml: inertia.profiles.steady: expected a table, like [inertia.profiles.steady]",
                "balance.toml: inertia: unknown key 'profile' (did you mean 'profiles'?)",
            ]
        );
        assert_eq!(
            problems(characters(&format!("{VEX}inertia = 3\n"))),
            ["characters.toml: vex.inertia: expected text in quotes"]
        );
    }

    #[test]
    fn reports_profiles_that_do_not_exist_or_go_below_zero() {
        let text = r#"
            [inertia]
            default_profile = "hardenin"

            [inertia.profiles.hardening]
            good.toward_evil = [[-100.0, -0.1], [100.0, 0.5]]
        "#;
        let result = parse_content(Sources {
            balance: Some(text),
            characters: Some(&format!("{VEX}inertia = \"stedy\"\n")),
            ..Sources::default()
        });
        assert_eq!(
            problems(result),
            [
                "balance.toml: inertia.default_profile: unknown inertia profile 'hardenin' (did you mean 'hardening'?)",
                "balance.toml: inertia.profiles.hardening.good.toward_evil: -0.10 is below 0.00: inertia can damp or amplify a shift, never reverse it",
                "characters.toml: vex.inertia: unknown inertia profile 'stedy' (did you mean 'steady'?)",
            ]
        );
    }

    // Defectors and deserters (M7)

    const TABLES: &str = r#"
        [membership.defectors]
        rules = [
          { when = { standing_with_target_at_least = 50.0 }, then = "accept" },
          { when = { closer_to_target = true }, then = "accept", standing_change = -10.0 },
          { then = "refuse", reason = "You serve our enemies." },
        ]

        [membership.deserters]
        rules = [
          { when = { rank_at_least = 3 }, then = "refuse", reason = "Officers don't walk away." },
          { when = { outside_member_tolerance = true }, then = "release" },
          { then = "release", standing_change = -40.0 },
        ]
    "#;

    fn rule(when: Vec<Condition>, then: Verdict) -> Rule {
        Rule { when, then }
    }

    fn allow(standing_change: i64) -> Verdict {
        Verdict::Allow {
            standing_change: h(standing_change),
        }
    }

    fn refuse(reason: &str) -> Verdict {
        Verdict::Refuse {
            reason: Some(reason.to_owned()),
        }
    }

    #[test]
    fn reads_the_worlds_defectors_and_deserters_tables() {
        let content = balance(TABLES).expect("valid content");
        assert_eq!(
            content.balance.rule_tables,
            [
                (
                    TableKind::Defectors,
                    vec![
                        rule(
                            vec![Condition::StandingWithTargetAtLeast(h(50_00))],
                            allow(0)
                        ),
                        rule(vec![Condition::CloserToTarget(true)], allow(-10_00)),
                        rule(Vec::new(), refuse("You serve our enemies.")),
                    ]
                ),
                (
                    TableKind::Deserters,
                    vec![
                        rule(
                            vec![Condition::RankAtLeast(RankRef::Rung(3))],
                            refuse("Officers don't walk away.")
                        ),
                        rule(vec![Condition::OutsideMemberTolerance(true)], allow(0)),
                        rule(Vec::new(), allow(-40_00)),
                    ]
                ),
            ]
            .into()
        );
        let plain = balance("").expect("valid content");
        assert!(plain.balance.rule_tables.is_empty(), "built in");
    }

    #[test]
    fn reads_a_factions_own_tables_which_may_name_its_ranks() {
        let text = format!(
            r#"{GUILD}
            [lantern_guild.deserters]
            rules = [
              {{ when = {{ rank_below = "fence", standing_with_current_below = 10.0 }}, then = "release" }},
              {{ when = {{ rank_at_least = "shadow", closer_to_target = false }}, then = "refuse", reason = "Stay." }},
              {{ when = {{}}, then = "release", standing_change = -20.0 }},
            ]

            [lantern_guild.defectors]
            rules = [
              {{ when = {{ standing_with_current_at_least = 10.0, standing_with_target_below = 0.0 }}, then = "refuse", reason = "Spy." }},
              {{ when = {{ outside_member_tolerance = false }}, then = "accept" }},
              {{ then = "refuse", reason = "No." }},
            ]
            "#
        );
        let content = factions(&text).expect("valid content");
        let guild = &content.factions[&FactionId::new("lantern_guild").expect("valid id")];
        let rank = |id: &str| RankRef::Id(RankId::new(id).expect("valid id"));
        assert_eq!(
            guild.rule_tables,
            [
                (
                    TableKind::Defectors,
                    vec![
                        rule(
                            vec![
                                Condition::StandingWithCurrentAtLeast(h(10_00)),
                                Condition::StandingWithTargetBelow(h(0)),
                            ],
                            refuse("Spy.")
                        ),
                        rule(vec![Condition::OutsideMemberTolerance(false)], allow(0)),
                        rule(Vec::new(), refuse("No.")),
                    ]
                ),
                (
                    TableKind::Deserters,
                    vec![
                        rule(
                            vec![
                                Condition::RankBelow(rank("fence")),
                                Condition::StandingWithCurrentBelow(h(10_00)),
                            ],
                            allow(0)
                        ),
                        rule(
                            vec![
                                Condition::RankAtLeast(rank("shadow")),
                                Condition::CloserToTarget(false),
                            ],
                            refuse("Stay.")
                        ),
                        rule(Vec::new(), allow(-20_00)),
                    ]
                ),
            ]
            .into()
        );
    }

    #[test]
    fn reports_mistakes_in_a_rule_table_at_their_keys() {
        let text = r#"
            [membership.defectors]
            rule = []
            rules = [
              { when = { closer_to_targt = true }, then = "accept" },
              { when = { standing_with_target_at_least = "high" }, then = "accept" },
              { when = { closer_to_target = "yes" }, then = "accept" },
              { when = { rank_at_least = 2.5 }, then = "accept" },
              { when = { rank_below = "Shadow" }, then = "accept" },
              { when = true, then = "accept" },
              { then = "release" },
              { then = "refuze", reason = "No." },
              { then = "refuse" },
              { then = "refuse", reason = "No.", standing_change = -5.0 },
              { then = "accept", reason = "Welcome." },
              "refuse",
            ]

            [membership.deserter]
            rules = []

            [membership.deserters]
        "#;
        assert_eq!(
            problems(balance(text)),
            [
                "balance.toml: membership.defectors.rules[0].when: unknown key 'closer_to_targt' (did you mean 'closer_to_target'?)",
                "balance.toml: membership.defectors.rules[1].when.standing_with_target_at_least: expected a number, like 25.0",
                "balance.toml: membership.defectors.rules[2].when.closer_to_target: expected true or false",
                "balance.toml: membership.defectors.rules[3].when.rank_at_least: expected a rung number, like 3, or a rank id in quotes",
                "balance.toml: membership.defectors.rules[4].when.rank_below: 'Shadow' isn't a valid id: use lowercase letters, digits and _, starting with a letter",
                "balance.toml: membership.defectors.rules[5].when: expected a table, like { closer_to_target = true }",
                "balance.toml: membership.defectors.rules[6].then: unknown outcome 'release' for defectors: use 'accept' or 'refuse'",
                "balance.toml: membership.defectors.rules[7].then: unknown outcome 'refuze' for defectors (did you mean 'refuse'?)",
                "balance.toml: membership.defectors.rules[8]: missing 'reason'",
                "balance.toml: membership.defectors.rules[9].standing_change: a refusal changes nothing, so it has no standing_change",
                "balance.toml: membership.defectors.rules[10].reason: only a refusal has a reason",
                "balance.toml: membership.defectors.rules[11]: expected a table, like { when = { closer_to_target = true }, then = \"accept\" }",
                "balance.toml: membership.defectors: unknown key 'rule' (did you mean 'rules'?)",
                "balance.toml: membership.deserters: missing 'rules'",
                "balance.toml: membership: unknown key 'deserter' (did you mean 'deserters'?)",
            ]
        );
    }

    #[test]
    fn reports_tables_that_name_ranks_wrongly_or_might_not_decide() {
        let balance_text = r#"
            [membership.deserters]
            rules = [
              { when = { rank_at_least = 0, standing_with_current_below = 120.0 }, then = "refuse", reason = "No." },
              { when = { rank_below = "shadow" }, then = "release", standing_change = -120.0 },
              { when = { outside_member_tolerance = true }, then = "release" },
            ]
        "#;
        let factions_text = format!(
            r#"{GUILD}
            [lantern_guild.defectors]
            rules = [
              {{ when = {{ rank_at_least = "captain" }}, then = "accept" }},
              {{ when = {{ rank_below = "shadw" }}, then = "accept" }},
              {{ then = "refuse", reason = "No." }},
            ]

            [lantern_guild.deserters]
            rules = []
            "#
        );
        let result = parse_content(Sources {
            balance: Some(balance_text),
            factions: Some(&factions_text),
            ..Sources::default()
        });
        assert_eq!(
            problems(result),
            [
                "balance.toml: membership.deserters.rules[0].when.rank_at_least: 0 isn't a rung: rungs count from 1, the lowest",
                "balance.toml: membership.deserters.rules[0].when.standing_with_current_below: 120.00 is outside -100.00..100.00",
                "balance.toml: membership.deserters.rules[1].when.rank_below: 'shadow' is a rank id, but the world's tables can't name ranks: use a rung number, 1 for the lowest",
                "balance.toml: membership.deserters.rules[1].standing_change: -120.00 is outside -100.00..100.00",
                "balance.toml: membership.deserters.rules[2].when: the last rule must have no conditions, so the table always decides",
                "factions.toml: lantern_guild.defectors.rules[0].when.rank_at_least: unknown rank 'captain' for lantern_guild",
                "factions.toml: lantern_guild.defectors.rules[1].when.rank_below: unknown rank 'shadw' for lantern_guild (did you mean 'shadow'?)",
                "factions.toml: lantern_guild.deserters.rules: a table needs at least one rule, and its last must have no conditions",
            ]
        );
    }

    #[test]
    fn a_faction_table_with_a_mistake_is_reported_at_the_factions_key() {
        let text = format!(
            r#"{GUILD}
            [lantern_guild.deserters]
            rules = [{{ then = "accept" }}]
            "#
        );
        assert_eq!(
            problems(factions(&text)),
            [
                "factions.toml: lantern_guild.deserters.rules[0].then: unknown outcome 'accept' for deserters: use 'release' or 'refuse'"
            ]
        );
        let text = GUILD.replacen(
            "member_tolerance = 60.0",
            "member_tolerance = 60.0\n        defectors = 3",
            1,
        );
        assert_eq!(
            problems(factions(&text)),
            [
                "factions.toml: lantern_guild.defectors: expected a table, like [lantern_guild.defectors]"
            ]
        );
    }

    // relations.toml

    fn relations_of(factions: &str, relations: &str) -> Result<Content, ContentError> {
        parse_content(Sources {
            factions: Some(factions),
            relations: Some(relations),
            ..Sources::default()
        })
    }

    const RELATIONS: &str = r#"
        [[relation]]
        between = ["city_watch", "lantern_guild"]
        value = -80.0

        [[relation]]
        from = "city_watch"
        to = "lantern_guild"
        value = -90.0
    "#;

    #[test]
    fn reads_relations_both_ways_and_one_way() {
        let content = relations_of(
            &format!("{WATCH}{GUILD}"),
            r#"
            [[relation]]
            between = ["city_watch", "lantern_guild"]
            value = -80.0
            "#,
        )
        .expect("valid content");
        assert_eq!(
            content.relations,
            [Relation {
                ends: RelationEnds::Between(
                    FactionId::new("city_watch").expect("valid id"),
                    FactionId::new("lantern_guild").expect("valid id")
                ),
                value: h(-80_00),
            }]
        );
        let content = relations_of(
            &format!("{WATCH}{GUILD}"),
            r#"
            [[relation]]
            from = "city_watch"
            to = "lantern_guild"
            value = -30.0
            "#,
        )
        .expect("valid content");
        assert_eq!(
            content.relations,
            [Relation {
                ends: RelationEnds::Directed {
                    from: FactionId::new("city_watch").expect("valid id"),
                    to: FactionId::new("lantern_guild").expect("valid id"),
                },
                value: h(-30_00),
            }]
        );
    }

    #[test]
    fn reports_relation_mistakes_at_their_keys() {
        assert_eq!(
            problems(relations_of(&format!("{WATCH}{GUILD}"), RELATIONS)),
            [
                "relations.toml: relation[1]: city_watch → lantern_guild is already set by relation[0]"
            ]
        );
        let text = r#"
            [[relation]]
            between = ["city_watch", "lantern_gild"]
            value = -80.0

            [[relation]]
            from = "city_watch"
            to = "city_watch"
            value = 120.0

            [[relation]]
            between = ["city_watch"]
            from = "city_watch"
            value = 1

            [[relation]]
            value = 1
            colour = "red"

            [[relation]]
            between = ["city_watch", "Lantern Guild"]
        "#;
        assert_eq!(
            problems(relations_of(&format!("{WATCH}{GUILD}"), text)),
            [
                "relations.toml: relation[2].between: expected 2 factions, like [\"city_watch\", \"lantern_guild\"]",
                "relations.toml: relation[2]: give either between = [a, b], or from and to, not both",
                "relations.toml: relation[3]: give either between = [a, b], or from and to",
                "relations.toml: relation[3]: unknown key 'colour'",
                "relations.toml: relation[4].between[1]: 'Lantern Guild' isn't a valid id: use lowercase letters, digits and _, starting with a letter",
                "relations.toml: relation[4]: missing 'value'",
                "relations.toml: relation[0].between[1]: unknown faction 'lantern_gild' (did you mean 'lantern_guild'?)",
                "relations.toml: relation[1]: a faction can't have a relation with itself",
                "relations.toml: relation[1].value: 120.00 is outside -100.00..100.00",
            ]
        );
        assert_eq!(
            problems(relations_of(WATCH, "relation = 3")),
            ["relations.toml: relation: expected a list of [[relation]] tables"]
        );
        assert_eq!(
            problems(relations_of(WATCH, "[[relations]]\nvalue = 1")),
            ["relations.toml: unknown key 'relations' (did you mean 'relation'?)"]
        );
    }

    #[test]
    fn reads_the_conflict_threshold_and_relation_bands() {
        let content = balance(
            r#"
            [relations]
            conflict_threshold = -40.0
            bands = [{ name = "foe", up_to = 0 }, { name = "friend" }]
            "#,
        )
        .expect("valid content");
        assert_eq!(content.balance.conflict_threshold, h(-40_00));
        let names: Vec<&str> = content
            .balance
            .relation_bands
            .iter()
            .map(|b| b.name.as_str())
            .collect();
        assert_eq!(names, ["foe", "friend"]);
        assert_eq!(
            problems(balance("[relations]\nconflict_threshold = -120.0")),
            ["balance.toml: relations.conflict_threshold: -120.00 is outside -100.00..100.00"]
        );
        assert_eq!(
            problems(balance(
                "[relations]\nbands = [{ name = \"foe\", up_to = 0 }]"
            )),
            [
                "balance.toml: relations.bands[0].up_to: the last band can't have 'up_to': it takes every score above the band before it"
            ]
        );
    }

    #[test]
    fn no_one_starts_in_two_factions_in_conflict() {
        let found = problems(parse_content(Sources {
            factions: Some(&format!("{WATCH}{GUILD}")),
            relations: Some(
                "[[relation]]\nbetween = [\"city_watch\", \"lantern_guild\"]\nvalue = -80.0",
            ),
            characters: Some(&format!(
                "{VEX}memberships = [{{ faction = \"lantern_guild\" }}, {{ faction = \"city_watch\" }}]"
            )),
            ..Sources::default()
        }));
        assert_eq!(
            found,
            [
                "characters.toml: vex.memberships[1].faction: vex can't start in both lantern_guild and city_watch: they're in conflict (-80.00)"
            ]
        );
    }

    // Standing (M3)

    fn sources<'a>(
        factions: &'a str,
        characters: &'a str,
        actions: &'a str,
        outcomes: &'a str,
    ) -> Result<Content, ContentError> {
        parse_content(Sources {
            factions: Some(factions),
            characters: Some(characters),
            actions: Some(actions),
            outcomes: Some(outcomes),
            ..Sources::default()
        })
    }

    fn fixed(text: &str) -> Fixed {
        text.parse().expect("a number")
    }

    #[test]
    fn reads_standing_in_characters_actions_and_outcomes() {
        let characters = format!(
            "{VEX}standing = {{ factions = {{ lantern_guild = 30.0 }}, characters = {{ vex2 = -5 }} }}\n[vex2]\nname = \"Vex Two\"\nalignment = {{ law = 0.0, good = 0.0 }}"
        );
        let actions = r#"
            [steal]
            alignment = { law = -5.0, good = -3.0 }
            standing = { target = -20.0, target_factions = -10.0 }

            [donate_to_temple]
            standing = { factions = { lantern_guild = 10.0 }, characters = { vex = 2.5 } }
        "#;
        let outcomes = r#"
            [fined_by_watch]
            standing = { factions = { lantern_guild = -20.0 }, characters = { vex = -10.0 } }

            [rescued_merchant]
            alignment = { good = 6.0 }
        "#;
        let content = sources(GUILD, &characters, actions, outcomes).expect("valid content");
        let vex = &content.characters[&CharacterId::new("vex").expect("valid")];
        assert_eq!(
            vex.standing
                .parties()
                .iter()
                .map(|(p, v)| (p.to_string(), *v))
                .collect::<Vec<_>>(),
            [
                ("lantern_guild".to_owned(), fixed("30")),
                ("vex2".to_owned(), fixed("-5"))
            ]
        );
        let steal = &content.actions[&ActionId::new("steal").expect("valid")].standing;
        assert_eq!(
            (steal.target, steal.target_factions),
            (Some(fixed("-20")), Some(fixed("-10")))
        );
        assert!(steal.named.parties().is_empty());
        let donate = &content.actions[&ActionId::new("donate_to_temple").expect("valid")].standing;
        assert_eq!((donate.target, donate.target_factions), (None, None));
        assert_eq!(donate.named.parties().len(), 2);
        let fined = &content.outcomes[&OutcomeId::new("fined_by_watch").expect("valid")];
        assert_eq!(fined.effects.standing.parties().len(), 2);
        assert_eq!(fined.effects.alignment, AlignmentDelta::default());
        let rescued = &content.outcomes[&OutcomeId::new("rescued_merchant").expect("valid")];
        assert_eq!(rescued.effects.alignment.good, fixed("6"));
        assert!(rescued.effects.standing.parties().is_empty());
    }

    #[test]
    fn reports_standing_mistakes_at_their_keys() {
        let characters =
            format!("{VEX}standing = {{ factions = {{ lantern_gild = 30.0 }}, friends = {{}} }}");
        let actions = r#"
            [report_crime]
            standing = { factions = { city_wach = 5.0 }, target = 150.0 }
        "#;
        let outcomes = r#"
            [fined_by_watch]
            standing = { characters = { "Captain Hale" = -10.0, vx = -5.0 } }
            ["Bad Outcome"]
        "#;
        assert_eq!(
            problems(sources(
                &format!("{WATCH}{GUILD}"),
                &characters,
                actions,
                outcomes
            )),
            [
                // Each file's own problems first...
                "characters.toml: vex.standing: unknown key 'friends'",
                "outcomes.toml: Bad Outcome: 'Bad Outcome' isn't a valid id: use lowercase letters, digits and _, starting with a letter",
                "outcomes.toml: fined_by_watch.standing.characters: 'Captain Hale' isn't a valid id: use lowercase letters, digits and _, starting with a letter",
                // ...then the world's checks across files.
                "characters.toml: vex.standing.factions.lantern_gild: unknown faction 'lantern_gild' (did you mean 'lantern_guild'?)",
                "actions.toml: report_crime.standing.target: 150.00 is outside -100.00..100.00",
                "actions.toml: report_crime.standing.factions.city_wach: unknown faction 'city_wach' (did you mean 'city_watch'?)",
                "outcomes.toml: fined_by_watch.standing.characters.vx: unknown character 'vx' (did you mean 'vex'?)",
            ]
        );
    }

    #[test]
    fn reads_and_checks_leave_standing_change() {
        let content = factions(&WATCH.replace(
            "tolerance = 40.0",
            "tolerance = 40.0\nleave_standing_change = -15.0",
        ))
        .expect("valid content");
        let watch = &content.factions[&FactionId::new("city_watch").expect("valid")];
        assert_eq!(watch.leave_standing_change, fixed("-15"));
        let plain = factions(WATCH).expect("valid content");
        assert_eq!(
            plain.factions[&FactionId::new("city_watch").expect("valid")].leave_standing_change,
            Fixed::ZERO
        );
        assert_eq!(
            problems(factions(&WATCH.replace(
                "tolerance = 40.0",
                "tolerance = 40.0\nleave_standing_change = -120.0"
            ))),
            ["factions.toml: city_watch.leave_standing_change: -120.00 is outside -100.00..100.00"]
        );
    }

    // actions.toml

    const STEAL: &str = r#"
        [steal]
        alignment = { law = -5.0, good = -3.0 }
    "#;

    fn delta(content: &Content, action: &str) -> AlignmentDelta {
        content.actions[&ActionId::new(action).expect("valid id")].alignment
    }

    #[test]
    fn reads_actions_and_their_alignment_effects() {
        let text = format!("{STEAL}\n[help_stranger]\nalignment = {{ good = 4.0 }}");
        let content = actions(&text).expect("valid content");
        assert_eq!(
            delta(&content, "steal"),
            AlignmentDelta {
                law: h(-5_00),
                good: h(-3_00)
            }
        );
        assert_eq!(
            delta(&content, "help_stranger"),
            AlignmentDelta {
                law: h(0),
                good: h(4_00)
            },
            "an axis left out isn't moved"
        );
        let ids: Vec<&str> = content.actions.keys().map(ActionId::as_str).collect();
        assert_eq!(ids, ["help_stranger", "steal"]);
    }

    #[test]
    fn an_action_without_an_alignment_effect_moves_nothing() {
        let content = actions("[wave]").expect("valid content");
        assert_eq!(delta(&content, "wave"), AlignmentDelta::default());
    }

    #[test]
    fn an_actions_alignment_names_only_law_and_good() {
        assert_eq!(
            problems(actions(&STEAL.replace("good =", "goood ="))),
            ["actions.toml: steal.alignment: unknown key 'goood' (did you mean 'good'?)"]
        );
        assert_eq!(
            problems(actions(&STEAL.replace("good =", "chaos ="))),
            ["actions.toml: steal.alignment: unknown key 'chaos'"]
        );
    }

    #[test]
    fn reports_mistakes_in_an_action() {
        let text = r#"
            bow = 3

            ["Steal"]
            alignment = { law = -5.0 }

            [extort]
            alignment = "evil"

            [murder]
            alignment = { law = -10.005 }
            standing = { target = -100.0 }
            witnesses = "all"
        "#;
        assert_eq!(
            problems(actions(text)),
            [
                "actions.toml: Steal: 'Steal' isn't a valid id: use lowercase letters, digits and _, starting with a letter",
                "actions.toml: bow: expected a table of action fields, like [bow]",
                "actions.toml: extort.alignment: expected a table, like { law = 0.0, good = 0.0 }",
                "actions.toml: murder.alignment.law: -10.005 has more than 2 decimal places",
                "actions.toml: murder: unknown key 'witnesses'",
            ]
        );
    }

    // Mistakes in balance.toml

    #[test]
    fn reports_an_unknown_section_in_balance() {
        assert_eq!(
            problems(balance("[alignmnt]\nlabel_threshold = 40")),
            ["balance.toml: unknown key 'alignmnt' (did you mean 'alignment'?)"]
        );
    }

    #[test]
    fn reports_an_unknown_key_in_a_balance_section() {
        assert_eq!(
            problems(balance("[alignment]\nlabel_treshold = 40")),
            [
                "balance.toml: alignment: unknown key 'label_treshold' (did you mean 'label_threshold'?)"
            ]
        );
    }

    #[test]
    fn reports_a_label_threshold_out_of_range() {
        for value in ["0", "100.01", "-5"] {
            let shown: Fixed = value.parse().expect("a number");
            assert_eq!(
                problems(balance(&format!("[alignment]\nlabel_threshold = {value}"))),
                [format!(
                    "balance.toml: alignment.label_threshold: {shown} must be between 0.01 and 100.00"
                )]
            );
        }
        assert!(balance("[alignment]\nlabel_threshold = 0.01").is_ok());
        assert!(balance("[alignment]\nlabel_threshold = 100").is_ok());
    }

    #[test]
    fn reports_a_balance_section_that_is_not_a_table() {
        assert_eq!(
            problems(balance("alignment = 3")),
            ["balance.toml: alignment: expected a table, like [alignment]"]
        );
    }

    // All at once

    #[test]
    fn reports_every_problem_in_every_file_in_order() {
        let found = problems(parse_content(Sources {
            balance: Some("[alignment]\nlabel_threshold = 0"),
            characters: Some(
                "[zed]\nalignment = { law = 0.0, good = 0.0 }\n[abe]\nname = 1\nalignment = { law = 0.0, good = 0.0 }",
            ),
            actions: Some("[steal]\nalignment = { evil = 3.0 }"),
            factions: Some("[watch]\nname = \"The Watch\""),
            relations: Some("[[relation]]\nfrom = \"watch\"\nto = \"guild\""),
            outcomes: Some("[fine]\nweight = 3"),
            ..Sources::default()
        }));
        assert_eq!(
            found,
            [
                "balance.toml: alignment.label_threshold: 0.00 must be between 0.01 and 100.00",
                "factions.toml: watch: missing 'alignment'",
                "factions.toml: watch: missing 'tolerance'",
                "relations.toml: relation[0]: missing 'value'",
                "characters.toml: abe.name: expected text in quotes",
                "characters.toml: zed: missing 'name'",
                "actions.toml: steal.alignment: unknown key 'evil'",
                "outcomes.toml: fine: unknown key 'weight'",
            ]
        );
    }

    #[test]
    fn a_diagnostic_without_a_key_names_just_the_file() {
        let diagnostic = Diagnostic {
            file: "characters.toml".into(),
            key: None,
            message: "line 2: broken".into(),
        };
        assert_eq!(diagnostic.to_string(), "characters.toml: line 2: broken");
        let error = ContentError {
            diagnostics: vec![diagnostic.clone(), diagnostic],
        };
        assert_eq!(
            error.to_string(),
            "characters.toml: line 2: broken\ncharacters.toml: line 2: broken"
        );
    }

    // Curves

    #[test]
    fn parses_a_curve_list_of_points() {
        let curve = parse_curve("[[0, 1.0], [100, 0.5]]").expect("a valid curve");
        assert_eq!(curve.at(h(5000)), h(75));
    }

    #[test]
    fn parses_a_number_as_a_flat_curve() {
        let curve = parse_curve("1.5").expect("a valid curve");
        assert_eq!(curve.at(h(-99_900)), h(150));
    }

    #[test]
    fn reports_a_curves_own_validation_errors() {
        assert_eq!(
            parse_curve("[[0, 1.0]]"),
            Err("a curve is a single number or at least 2 points".into())
        );
    }

    #[test]
    fn reports_curve_text_that_is_not_toml() {
        let error = parse_curve("[[0, 1.0], [5").unwrap_err();
        assert!(!error.is_empty());
    }

    // Relation effects (Q2)

    const TWO_FACTIONS: &str = r#"
        [watch]
        name = "The Watch"
        alignment = { law = 60.0, good = 10.0 }
        tolerance = 40.0
        [[watch.ranks]]
        id = "recruit"

        [temple]
        name = "The Temple"
        alignment = { law = 30.0, good = 80.0 }
        tolerance = 40.0
        [[temple.ranks]]
        id = "acolyte"
    "#;

    fn outcomes_with(text: &str) -> Result<Content, ContentError> {
        parse_content(Sources {
            factions: Some(TWO_FACTIONS),
            outcomes: Some(text),
            ..Sources::default()
        })
    }

    #[test]
    fn reads_an_outcomes_relation_shifts_in_the_order_written() {
        let content = outcomes_with(
            r#"
            [discord]
            relations = [
              { between = ["watch", "temple"], by = -40.0 },
              { from = "temple", to = "watch", by = 5 },
            ]
            "#,
        );
        // The second shifts a direction the first already does.
        assert_eq!(
            problems(content),
            [
                "outcomes.toml: discord.relations[1]: temple → watch is already shifted by relations[0]"
            ]
        );
        let content = outcomes_with(
            r#"
            [discord]
            relations = [
              { between = ["watch", "temple"], by = -40.0 },
              { from = "temple", to = "temple_x", by = 5 },
            ]
            "#,
        );
        assert_eq!(
            problems(content),
            [
                "outcomes.toml: discord.relations[1].to: unknown faction 'temple_x' (did you mean 'temple'?)"
            ]
        );
        let content = outcomes_with(
            r#"
            [discord]
            relations = [{ between = ["watch", "temple"], by = -40.0 }, { from = "watch", to = "temple", by = 0 }]
            [other]
            relations = [{ from = "temple", to = "watch", by = 200.0 }]
            "#,
        );
        assert_eq!(
            problems(content),
            [
                "outcomes.toml: discord.relations[1]: watch → temple is already shifted by relations[0]"
            ]
        );
        let content = outcomes_with(
            r#"
            [discord]
            relations = [{ between = ["watch", "temple"], by = -40.0 }, { from = "watch", to = "watch", by = 1 }]
            [other]
            relations = [{ from = "temple", to = "watch", by = 250.0 }, { between = ["temple", "temple"], by = 1 }]
            "#,
        );
        assert_eq!(
            problems(content),
            [
                "outcomes.toml: discord.relations[1]: a faction can't have a relation with itself",
                "outcomes.toml: other.relations[0].by: 250.00 is outside -200.00..200.00",
                "outcomes.toml: other.relations[1]: a faction can't have a relation with itself",
            ]
        );
        let content = outcomes_with(
            r#"
            [discord]
            relations = [{ between = ["wach", "temple"], by = -40.0 }, { from = "temple", to = "watch", by = -200 }]
            "#,
        );
        assert_eq!(
            problems(content),
            [
                "outcomes.toml: discord.relations[0].between[0]: unknown faction 'wach' (did you mean 'watch'?)"
            ]
        );
        let shifts = |text: &str| {
            outcomes_with(text).map(|content| {
                content.outcomes[&OutcomeId::new("discord").expect("valid")]
                    .effects
                    .relations
                    .clone()
            })
        };
        let faction = |id: &str| FactionId::new(id).expect("valid id");
        assert_eq!(
            shifts(
                r#"
                [discord]
                relations = [{ from = "watch", to = "temple", by = -40.0 }, { from = "temple", to = "watch", by = -200 }]
                "#
            ),
            Ok(vec![
                RelationShift {
                    ends: RelationEnds::Directed {
                        from: faction("watch"),
                        to: faction("temple"),
                    },
                    by: h(-40_00),
                },
                RelationShift {
                    ends: RelationEnds::Directed {
                        from: faction("temple"),
                        to: faction("watch"),
                    },
                    by: h(-200_00),
                },
            ])
        );
        assert_eq!(
            shifts("[discord]\nrelations = [{ between = [\"temple\", \"watch\"], by = 200.0 }]"),
            Ok(vec![RelationShift {
                ends: RelationEnds::Between(faction("temple"), faction("watch")),
                by: h(200_00),
            }])
        );
        assert_eq!(shifts("[discord]"), Ok(Vec::new()));
    }

    #[test]
    fn relation_shifts_are_written_like_relations_with_by() {
        assert_eq!(
            problems(outcomes_with(
                r#"
                [discord]
                relations = [
                  { between = ["watch", "temple"], from = "watch", by = 1 },
                  { from = "watch", to = "temple", value = 1 },
                  { between = ["watch"], by = 1 },
                  { between = ["watch", "temple"], by = "a lot" },
                ]
                "#
            )),
            [
                "outcomes.toml: discord.relations[0]: give either between = [a, b], or from and to, not both",
                "outcomes.toml: discord.relations[1]: missing 'by'",
                "outcomes.toml: discord.relations[1]: unknown key 'value'",
                "outcomes.toml: discord.relations[2].between: expected 2 factions, like [\"city_watch\", \"lantern_guild\"]",
                "outcomes.toml: discord.relations[3].by: expected a number, like 25.0",
            ]
        );
        assert_eq!(
            problems(outcomes_with(
                "[discord]\nrelations = { between = [\"watch\", \"temple\"] }"
            )),
            [
                "outcomes.toml: discord.relations: expected a list, like [{ between = [\"city_watch\", \"temple\"], by = -10.0 }]"
            ]
        );
        assert_eq!(
            problems(outcomes_with("[discord]\nrelations = [3]")),
            [
                "outcomes.toml: discord.relations[0]: expected a table, like { between = [\"city_watch\", \"temple\"], by = -10.0 }"
            ]
        );
    }
}
