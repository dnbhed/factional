//! Loads designer content (TOML) from disk, validates it, and turns mistakes into diagnostics
//! that name the file and the key path (DESIGN.md §12.1).

mod reader;

use std::collections::BTreeMap;
use std::path::Path;
use std::{fmt, fs, io};

use factional_core::{Curve, Fixed, suggest};
use factional_reputation::{
    Action, ActionId, ActionStanding, Alignment, AlignmentDelta, Balance, Band, BandProblem, Bands,
    Character, CharacterId, ComponentKind, Content, ContentProblem, ContentWarning,
    DispositionWeights, Effects, Faction, FactionId, InvalidId, Metric, Outcome, OutcomeId, Party,
    Rank, RankId, RankKey, Relation, RelationEnds, RelationSide, StandingEffects, StandingKey,
    StandingOwner, StartingMembership, ToleranceProblem, Tolerances, WeightProblem, Weights,
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
}

const BALANCE_FILE: &str = "balance.toml";
const FACTIONS_FILE: &str = "factions.toml";
const CHARACTERS_FILE: &str = "characters.toml";
const ACTIONS_FILE: &str = "actions.toml";
const RELATIONS_FILE: &str = "relations.toml";
const OUTCOMES_FILE: &str = "outcomes.toml";

/// Reads and validates the content files in `dir`.
pub fn load_dir(dir: &Path) -> Result<Content, ContentError> {
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
    let balance = read(BALANCE_FILE)?;
    let factions = read(FACTIONS_FILE)?;
    let characters = read(CHARACTERS_FILE)?;
    let actions = read(ACTIONS_FILE)?;
    let outcomes = read(OUTCOMES_FILE)?;
    let relations = read(RELATIONS_FILE)?;
    parse_content(Sources {
        balance: balance.as_deref(),
        factions: factions.as_deref(),
        characters: characters.as_deref(),
        actions: actions.as_deref(),
        relations: relations.as_deref(),
        outcomes: outcomes.as_deref(),
    })
}

/// Validates content from the text of its files, reporting every problem at once: each
/// file's own, in file order, then problems across files (P-32).
pub fn parse_content(sources: Sources<'_>) -> Result<Content, ContentError> {
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

    let mut diagnostics: Vec<Diagnostic> = reports
        .into_iter()
        .flat_map(|report| report.diagnostics)
        .collect();
    diagnostics.extend(content.problems().iter().map(|problem| {
        let (file, key) = match problem {
            ContentProblem::SharedId(id) => (FACTIONS_FILE, id.to_string()),
            ContentProblem::AffinityOutOfRange(_) => (BALANCE_FILE, "disposition.affinity".into()),
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
        };
        Diagnostic {
            file: file.to_owned(),
            key: Some(key),
            message: problem.to_string(),
        }
    }));
    if diagnostics.is_empty() {
        Ok(content)
    } else {
        Err(ContentError { diagnostics })
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
        disposition.finish(report);
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
    file.finish(report);
    balance
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
    let memberships = read_memberships(&mut section, report);
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
        memberships: memberships?,
        standing,
    })
}

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
        membership.finish(report);
        factions.push(
            faction
                .zip(rank)
                .map(|(faction, rank)| StartingMembership { faction, rank }),
        );
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
    let ends = match (between, directed) {
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
    };
    let value = section.fixed("value", report);
    section.finish(report);
    Some(Relation {
        ends: ends?,
        value: value?,
    })
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
    section.finish(report);
    Some(Faction {
        id,
        name: name?,
        alignment: alignment?,
        weights,
        tolerances: tolerances?,
        leave_standing_change,
        ranks: ranks?,
    })
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
    section.finish(report);
    Action {
        id,
        alignment,
        standing,
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
    section.finish(report);
    Outcome {
        id,
        effects: Effects {
            alignment,
            standing,
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
    use factional_reputation::{CharacterId, FactionId, Metric, Weights};

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
            drift = "flag"
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
                "factions.toml: watch: unknown key 'drift'",
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
                "characters.toml: vex.memberships[0]: vex starts 95.52 from The Lantern Guild, outside its member tolerance of 60.00"
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
}
